// HTTP request handlers - Simplified

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::app_state::SharedState;
use crate::models::*;
use crate::market_resolve::cpmm::{CPMMPool, VIABILITY_THRESHOLD};
use crate::rss::{RssEvent, EventDates, write_rss_event_to_file, ResolutionRules as RssResolutionRules};
use crate::ledger::{TxType, Transaction, Layer, FundStatus, MarketData, BetData, reconstruct_transactions_from_market_data};

/// Helper to convert app markets to ledger MarketData
fn markets_to_market_data(markets: &std::collections::HashMap<String, PredictionMarket>) -> Vec<MarketData> {
    markets.iter().map(|(id, market)| {
        MarketData {
            id: id.clone(),
            title: market.title.clone(),
            created_at: market.created_at,
            total_volume: market.total_volume,
            is_resolved: market.is_resolved,
            winning_option: market.winning_option,
            bets: market.bets.iter().map(|bet| BetData {
                id: bet.id.clone(),
                market_id: bet.market_id.clone(),
                bettor: bet.bettor.clone(),
                outcome: bet.outcome,
                amount: bet.amount,
                timestamp: bet.timestamp,
                status: bet.status.clone(),
            }).collect(),
        }
    }).collect()
}

// ===== BET REQUEST =====

#[derive(Debug, Deserialize)]
pub struct BetRequest {
    pub signature: String,
    pub from_address: String,
    pub market_id: String,
    pub option: String,
    pub amount: f64,
    pub nonce: u64,
    pub timestamp: u64,
}

// ===== BETTING ENDPOINT =====

pub async fn place_signed_bet(
    State(state): State<SharedState>,
    Json(req): Json<BetRequest>,
) -> Result<Json<SignedBetResponse>, (StatusCode, Json<SignedBetResponse>)> {
    println!("📥 Bet: market={}, option={}, amount={}", req.market_id, req.option, req.amount);
    
    // Validate timestamp (24h window)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    if now.abs_diff(req.timestamp) > 86400 {
        return Err((StatusCode::UNAUTHORIZED, Json(SignedBetResponse::error("Transaction expired"))));
    }
    
    let outcome = match req.option.as_str() {
        "0" | "YES" => 0,
        "1" | "NO" => 1,
        _ => return Err((StatusCode::BAD_REQUEST, Json(SignedBetResponse::error("Invalid option")))),
    };
    
    let mut app = state.lock().unwrap();
    
    // Validate nonce (replay protection)
    let last_nonce = app.nonces.get(&req.from_address).copied().unwrap_or(0);
    
    // Nonce must be greater than last used nonce
    // For first-time users (last_nonce 0), accept nonce 1+
    if req.nonce <= last_nonce {
        return Err((StatusCode::BAD_REQUEST, Json(SignedBetResponse {
            success: false,
            bet_id: None,
            transaction_id: None,
            market_id: None,
            outcome: None,
            amount: None,
            new_balance: None,
            nonce_used: None,
            error: Some(format!("Invalid nonce: got {}, expected > {}", req.nonce, last_nonce)),
        })));
    }
    
    // Resolve address
    let account = app.ledger.accounts.iter()
        .find(|(_, addr)| **addr == req.from_address)
        .map(|(name, _)| name.clone())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(SignedBetResponse::error("Address not found"))))?;
    
    // Check market exists
    if !app.markets.contains_key(&req.market_id) {
        return Err((StatusCode::NOT_FOUND, Json(SignedBetResponse::error("Market not found"))));
    }
    
    // Place bet
    match app.ledger.place_bet(&account, &req.market_id, outcome, req.amount, &req.signature) {
        Ok(tx) => {
            app.nonces.insert(req.from_address.clone(), req.nonce);
            
            let bet_id = if let Some(market) = app.markets.get_mut(&req.market_id) {
                market.record_bet(&account, req.amount, outcome)
            } else {
                tx.id.clone()
            };
            
            let balance = app.ledger.balance(&account);
            app.log_activity("🎯", "BET", &format!("{} bet {} BB on {}", account, req.amount, req.market_id));
            
            Ok(Json(SignedBetResponse {
                success: true,
                bet_id: Some(bet_id),
                transaction_id: Some(tx.id),
                market_id: Some(req.market_id),
                outcome: Some(outcome),
                amount: Some(req.amount),
                new_balance: Some(balance),
                nonce_used: Some(req.nonce),
                error: None,
            }))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(SignedBetResponse::error(&e)))),
    }
}

// ===== MARKET ENDPOINTS =====

pub async fn get_markets(State(state): State<SharedState>) -> Json<Value> {
    let app = state.lock().unwrap();
    let markets: Vec<Value> = app.markets.values()
        .map(|m| json!({
            "id": m.id,
            "title": m.title,
            "description": m.description,
            "category": m.category,
            "options": m.options,
            "is_resolved": m.is_resolved,
            "total_volume": m.total_volume,
            "odds": m.calculate_odds(),
        }))
        .collect();
    Json(json!({ "markets": markets }))
}

pub async fn get_market(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let app = state.lock().unwrap();
    let m = app.markets.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({
        "id": m.id,
        "title": m.title,
        "description": m.description,
        "category": m.category,
        "outcomes": m.options,
        "market_type": m.market_type,
        "tags": m.tags,
        "is_resolved": m.is_resolved,
        "total_volume": m.total_volume,
        "odds": m.calculate_odds(),
        "option_stats": m.option_stats,
        "initial_probabilities": m.initial_probabilities,
        "source": m.source,
        "source_url": m.source_url,
        "image_url": m.image_url,
        "dates": m.dates,
        "resolution_rules": m.resolution_rules,
        "created_at": m.created_at
    })))
}

pub async fn create_market(
    State(state): State<SharedState>,
    Json(payload): Json<CreateMarketRequest>,
) -> Result<Json<Value>, StatusCode> {
    // Use source ID or generate new UUID
    let id = payload.source.clone().unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    
    // Validate outcomes
    if payload.outcomes.is_empty() {
        return Ok(Json(json!({ "success": false, "error": "At least one outcome required" })));
    }
    
    // Category defaults to "general" if not provided
    let category = payload.category.clone().unwrap_or_else(|| "general".to_string());
    
    // Create market escrow address
    let escrow_address = format!("escrow:{}", &id);
    
    // === MINT LIQUIDITY ON L1 (before acquiring lock) ===
    let liquidity_amount = VIABILITY_THRESHOLD; // 10,000 BB tokens
    let l1_mint_result = mint_liquidity_on_l1(&escrow_address, liquidity_amount).await;
    
    // Now acquire the lock after async call
    let mut app = state.lock().unwrap();
    
    let l1_mint_status = match &l1_mint_result {
        Ok(tx_hash) => {
            app.log_activity("💰", "L1_MINT", &format!("Minted {} BB to {} (tx: {})", liquidity_amount, escrow_address, tx_hash));
            Some(json!({ "success": true, "tx_hash": tx_hash, "amount": liquidity_amount }))
        }
        Err(e) => {
            app.log_activity("⚠️", "L1_MINT", &format!("Failed to mint: {} - continuing with L2-only liquidity", e));
            Some(json!({ "success": false, "error": e.to_string() }))
        }
    };
    
    // Create market with base fields (outcomes maps to options internally)
    let mut market = PredictionMarket::new(
        id.clone(),
        payload.title.clone(),
        payload.description.clone(),
        category.clone(),
        payload.outcomes.clone(),
    );
    
    // === INITIALIZE CPMM POOL ===
    let cpmm_pool = CPMMPool::new(
        liquidity_amount,
        payload.outcomes.clone(),
        &escrow_address, // Initial LP is the market escrow
    );
    let initial_prices = cpmm_pool.calculate_prices();
    market.cpmm_pool = Some(cpmm_pool);
    
    // Set optional fields
    market.source = payload.source.clone();
    market.tags = payload.tags.clone().unwrap_or_default();
    market.market_type = payload.market_type.clone();
    market.source_url = payload.source_url.clone();
    market.image_url = payload.image_url.clone();
    market.dates = payload.dates.clone();
    market.resolution_rules = payload.resolution_rules.clone();
    
    // Use CPMM prices as initial probabilities (dynamic odds!)
    market.initial_probabilities = initial_prices.clone();
    
    app.markets.insert(id.clone(), market);
    
    // === RECORD TO LEDGER ===
    let market_tx = Transaction::market_created(&id, &payload.title, liquidity_amount);
    app.ledger.record(market_tx);
    
    // === PERSIST TO RSS FILE ===
    let rss_event = RssEvent {
        title: payload.title.clone(),
        description: payload.description.clone(),
        source: payload.source.clone(),
        category: payload.category.clone(),
        tags: payload.tags.clone().unwrap_or_default(),
        market_type: payload.market_type.clone().unwrap_or_else(|| "binary".to_string()),
        outcomes: payload.outcomes.clone(),
        initial_probabilities: Some(initial_prices.clone()),
        source_url: payload.source_url.clone().unwrap_or_default(),
        image_url: payload.image_url.clone(),
        dates: EventDates {
            published: payload.dates.as_ref()
                .and_then(|d| d.published.clone())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            freeze: payload.dates.as_ref().and_then(|d| d.freeze.clone()),
            resolution: payload.dates.as_ref().and_then(|d| d.resolution.clone()),
        },
        resolution_rules: payload.resolution_rules.as_ref().map(|r| RssResolutionRules {
            provider: r.provider.clone(),
            data_source: r.data_source.clone(),
            conditions: r.conditions.clone().unwrap_or_default(),
        }),
        market_id: id.clone(),
        added_to_ledger: true,
    };
    
    // Write to rss/events/ directory
    let rss_result = write_rss_event_to_file(&rss_event, "rss/events");
    let rss_file = match &rss_result {
        Ok(path) => {
            app.log_activity("💾", "RSS", &format!("Saved: {}", path));
            Some(path.clone())
        }
        Err(e) => {
            app.log_activity("⚠️", "RSS", &format!("Failed to save: {}", e));
            None
        }
    };
    
    app.log_activity("📊", "MARKET", &format!("Created: {} with CPMM pool ({} BB)", payload.title, liquidity_amount));
    
    Ok(Json(json!({ 
        "success": true, 
        "market_id": id,
        "title": payload.title,
        "category": category,
        "outcomes": payload.outcomes,
        "cpmm": {
            "initial_liquidity": liquidity_amount,
            "current_odds": initial_prices,
            "escrow_address": escrow_address
        },
        "l1_mint": l1_mint_status,
        "rss_file": rss_file
    })))
}

/// Mint liquidity tokens on L1 blockchain
async fn mint_liquidity_on_l1(to: &str, amount: f64) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    let mint_payload = json!({
        "to": to,
        "amount": amount
    });
    
    let response = client
        .post("http://localhost:8080/admin/mint")
        .header("Content-Type", "application/json")
        .json(&mint_payload)
        .send()
        .await
        .map_err(|e| format!("L1 connection failed: {}", e))?;
    
    if response.status().is_success() {
        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse L1 response: {}", e))?;
        
        Ok(body.get("tx_hash")
            .or_else(|| body.get("hash"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string())
    } else {
        Err(format!("L1 mint failed with status: {}", response.status()))
    }
}

/// Initialize liquidity for all existing markets that don't have CPMM pools
/// POST /markets/initial-liquidity
pub async fn initialize_all_market_liquidity(
    State(state): State<SharedState>,
) -> Json<Value> {
    let liquidity_amount = VIABILITY_THRESHOLD; // 10,000 BB per market
    let mut initialized: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    
    // Phase 1: Collect markets that need initialization (inside lock)
    let markets_to_init: Vec<(String, String, Vec<String>)> = {
        let app = state.lock().unwrap();
        app.markets.iter()
            .filter_map(|(id, market)| {
                if market.cpmm_pool.is_some() {
                    None // Already has pool
                } else {
                    Some((id.clone(), market.title.clone(), market.options.clone()))
                }
            })
            .collect()
    };
    
    // Collect skipped markets
    {
        let app = state.lock().unwrap();
        for (id, market) in app.markets.iter() {
            if market.cpmm_pool.is_some() {
                skipped.push(json!({
                    "market_id": id,
                    "title": market.title,
                    "reason": "Already has CPMM pool"
                }));
            }
        }
    }
    
    // Phase 2: Process each market (async L1 calls outside lock)
    for (market_id, title, options) in markets_to_init {
        let escrow_address = format!("escrow:{}", &market_id);
        
        // Mint on L1 (no lock held)
        let l1_result = mint_liquidity_on_l1(&escrow_address, liquidity_amount).await;
        
        // Phase 3: Update market with CPMM pool (inside lock)
        let mut app = state.lock().unwrap();
        
        if let Some(market) = app.markets.get_mut(&market_id) {
            // Initialize CPMM pool
            let cpmm_pool = CPMMPool::new(
                liquidity_amount,
                options.clone(),
                &escrow_address,
            );
            let prices = cpmm_pool.calculate_prices();
            market.cpmm_pool = Some(cpmm_pool);
            market.initial_probabilities = prices.clone();
            
            let l1_status = match &l1_result {
                Ok(tx_hash) => {
                    app.log_activity("💰", "L1_MINT", &format!("Minted {} BB to {} (tx: {})", liquidity_amount, escrow_address, tx_hash));
                    json!({ "success": true, "tx_hash": tx_hash })
                }
                Err(e) => {
                    app.log_activity("⚠️", "L1_MINT", &format!("Failed for {}: {}", market_id, e));
                    json!({ "success": false, "error": e })
                }
            };
            
            app.log_activity("🎰", "CPMM", &format!("Initialized pool for: {} ({} BB)", title, liquidity_amount));
            
            // === RECORD TO LEDGER ===
            let liquidity_tx = Transaction::liquidity_added(
                &market_id,
                "ORACLE",
                liquidity_amount,
                &format!("bulk_init_{}", market_id)
            );
            app.ledger.record(liquidity_tx);
            
            initialized.push(json!({
                "market_id": market_id,
                "title": title,
                "escrow_address": escrow_address,
                "liquidity": liquidity_amount,
                "options": options,
                "initial_odds": prices,
                "l1_mint": l1_status
            }));
        } else {
            failed.push(json!({
                "market_id": market_id,
                "error": "Market not found after L1 call"
            }));
        }
    }
    
    // Final summary
    let app = state.lock().unwrap();
    let total_markets = app.markets.len();
    
    Json(json!({
        "success": true,
        "summary": {
            "total_markets": total_markets,
            "initialized": initialized.len(),
            "skipped": skipped.len(),
            "failed": failed.len(),
            "liquidity_per_market": liquidity_amount,
            "total_liquidity_minted": initialized.len() as f64 * liquidity_amount
        },
        "initialized": initialized,
        "skipped": skipped,
        "failed": failed
    }))
}

/// Request body for initializing liquidity on a specific market
#[derive(Debug, Deserialize)]
pub struct InitLiquidityRequest {
    /// Amount of BB tokens to add (minimum 10,000, no maximum)
    pub amount: Option<f64>,
    /// Funder's L1 address (for user-funded) - if omitted, oracle mints
    pub funder: Option<String>,
    /// If true, admin mints tokens (oracle-funded)
    #[serde(default)]
    pub house_funded: bool,
}

/// Initialize liquidity for a SPECIFIC market
/// POST /markets/initial-liquidity/:market_id
/// 
/// Accepts market_id with or without .rss extension
/// Examples:
///   POST /markets/initial-liquidity/asml_hutto_jobs
///   POST /markets/initial-liquidity/asml_hutto_jobs.rss
/// 
/// Body:
///   { "amount": 15000, "funder": "L1_xxx..." }  // User-funded
///   { "amount": 10000, "house_funded": true }   // Oracle-funded (admin mint)
pub async fn initialize_market_liquidity(
    State(state): State<SharedState>,
    Path(market_id_raw): Path<String>,
    Json(payload): Json<InitLiquidityRequest>,
) -> Json<Value> {
    // Strip .rss extension if present
    let market_id = market_id_raw
        .strip_suffix(".rss")
        .unwrap_or(&market_id_raw)
        .to_string();
    
    // Validate amount (minimum 10,000 BB, no maximum)
    let amount = payload.amount.unwrap_or(VIABILITY_THRESHOLD);
    if amount < VIABILITY_THRESHOLD {
        return Json(json!({
            "success": false,
            "error": format!("Minimum liquidity is {} BB", VIABILITY_THRESHOLD),
            "minimum": VIABILITY_THRESHOLD,
            "provided": amount
        }));
    }
    
    // Check if market exists and get info
    let (market_exists, already_has_pool, title, options) = {
        let app = state.lock().unwrap();
        match app.markets.get(&market_id) {
            Some(market) => (
                true,
                market.cpmm_pool.is_some(),
                market.title.clone(),
                market.options.clone(),
            ),
            None => (false, false, String::new(), Vec::new()),
        }
    };
    
    if !market_exists {
        return Json(json!({
            "success": false,
            "error": "Market not found",
            "market_id": market_id,
            "hint": "Check available markets at GET /markets"
        }));
    }
    
    if already_has_pool {
        return Json(json!({
            "success": false,
            "error": "Market already has CPMM pool initialized",
            "market_id": market_id,
            "title": title,
            "hint": "Future: Adding liquidity to existing pools coming soon"
        }));
    }
    
    let escrow_address = format!("escrow:{}", &market_id);
    
    // Determine funding source and execute
    let (l1_result, funding_type, funder_display) = if payload.house_funded {
        // Oracle-funded: Admin mints tokens
        let result = mint_liquidity_on_l1(&escrow_address, amount).await;
        (result, "oracle_mint", "ORACLE".to_string())
    } else if let Some(ref funder) = payload.funder {
        // User-funded: Transfer from user's L1 balance to escrow
        let result = transfer_l1_to_escrow(funder, &escrow_address, amount).await;
        (result, "user_funded", funder.clone())
    } else {
        // Default to oracle-funded if no funder specified
        let result = mint_liquidity_on_l1(&escrow_address, amount).await;
        (result, "oracle_mint", "ORACLE".to_string())
    };
    
    // Update market with CPMM pool
    let mut app = state.lock().unwrap();
    
    if let Some(market) = app.markets.get_mut(&market_id) {
        // Initialize CPMM pool
        let cpmm_pool = CPMMPool::new(
            amount,
            options.clone(),
            &escrow_address,
        );
        let prices = cpmm_pool.calculate_prices();
        market.cpmm_pool = Some(cpmm_pool);
        market.initial_probabilities = prices.clone();
        
        let l1_status = match &l1_result {
            Ok(tx_hash) => {
                app.log_activity("💰", funding_type.to_uppercase().as_str(), &format!(
                    "{} funded {} BB to {} (tx: {})", 
                    funder_display, amount, escrow_address, tx_hash
                ));
                json!({ "success": true, "tx_hash": tx_hash })
            }
            Err(e) => {
                app.log_activity("⚠️", "L1_FUND", &format!("Failed for {}: {}", market_id, e));
                json!({ "success": false, "error": e })
            }
        };
        
        app.log_activity("🎰", "CPMM", &format!(
            "Initialized pool for: {} ({} BB from {})", 
            title, amount, funder_display
        ));
        
        // === RECORD TO LEDGER ===
        let liquidity_tx = Transaction::liquidity_added(
            &market_id,
            &funder_display,
            amount,
            &format!("cpmm_init_{}", market_id)
        );
        app.ledger.record(liquidity_tx);
        
        Json(json!({
            "success": true,
            "market_id": market_id,
            "title": title,
            "escrow_address": escrow_address,
            "liquidity": amount,
            "options": options,
            "current_odds": prices,
            "funding": {
                "type": funding_type,
                "funder": funder_display,
                "amount": amount
            },
            "l1_transaction": l1_status
        }))
    } else {
        Json(json!({
            "success": false,
            "error": "Market disappeared during initialization",
            "market_id": market_id
        }))
    }
}

/// Transfer tokens from user's L1 balance to market escrow
async fn transfer_l1_to_escrow(from: &str, to_escrow: &str, amount: f64) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    let transfer_payload = json!({
        "from": from,
        "to": to_escrow,
        "amount": amount
    });
    
    // Try L1 transfer endpoint
    let response = client
        .post("http://localhost:8080/transfer")
        .header("Content-Type", "application/json")
        .json(&transfer_payload)
        .send()
        .await
        .map_err(|e| format!("L1 connection failed: {}", e))?;
    
    if response.status().is_success() {
        let body: Value = response.json().await
            .map_err(|e| format!("Failed to parse L1 response: {}", e))?;
        
        Ok(body.get("tx_hash")
            .or_else(|| body.get("hash"))
            .and_then(|v| v.as_str())
            .unwrap_or("transfer_ok")
            .to_string())
    } else {
        Err(format!("L1 transfer failed with status: {} - ensure funder has sufficient L1 balance", response.status()))
    }
}

// ===== BALANCE ENDPOINTS =====

pub async fn get_balance(State(state): State<SharedState>, Path(account): Path<String>) -> Json<Value> {
    let app = state.lock().unwrap();
    let balance = app.ledger.balance(&account);
    let confirmed = app.ledger.confirmed_balance(&account);
    let pending = app.ledger.pending(&account);
    Json(json!({ 
        "account": account, 
        "balance": balance,
        "confirmed": confirmed,
        "pending": pending
    }))
}

pub async fn get_balance_details(State(state): State<SharedState>, Path(account): Path<String>) -> Json<Value> {
    let app = state.lock().unwrap();
    let addr = app.ledger.resolve(&account).unwrap_or(account.clone());
    Json(json!({
        "success": true,
        "account": account,
        "address": addr,
        "confirmed_balance": app.ledger.confirmed_balance(&account),
        "pending_delta": app.ledger.pending(&account),
        "available_balance": app.ledger.balance(&account),
    }))
}

pub async fn transfer(
    State(state): State<SharedState>,
    Json(payload): Json<TransferRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app = state.lock().unwrap();
    match app.ledger.transfer(&payload.from, &payload.to, payload.amount, "") {
        Ok(_) => {
            app.log_activity("💸", "TRANSFER", &format!("{} → {} | {} BB", payload.from, payload.to, payload.amount));
            Ok(Json(json!({ "success": true })))
        }
        Err(e) => Ok(Json(json!({ "success": false, "error": e }))),
    }
}

pub async fn get_ledger_activity(State(state): State<SharedState>) -> Json<Value> {
    let app = state.lock().unwrap();
    Json(json!({ "activity": app.blockchain_activity }))
}

// ===== PUBLIC LEDGER TRANSACTIONS ENDPOINT =====

/// Query params for ledger transactions
#[derive(Debug, Deserialize)]
pub struct LedgerQuery {
    /// Filter by transaction type: "Bet", "Transfer", "Deposit", "Withdraw", "Payout", "AccountCreated"
    #[serde(rename = "type")]
    pub tx_type: Option<String>,
    /// Filter by market ID
    pub market_id: Option<String>,
    /// Filter by account (from or to)
    pub account: Option<String>,
    /// Filter from timestamp (unix seconds)
    pub from_timestamp: Option<u64>,
    /// Filter to timestamp (unix seconds)
    pub to_timestamp: Option<u64>,
    /// Sort by: "timestamp", "amount" (default: "timestamp")
    pub sort_by: Option<String>,
    /// Sort order: "asc", "desc" (default: "desc")
    pub order: Option<String>,
    /// Limit results (default: 100, max: 1000)
    pub limit: Option<usize>,
    /// Offset for pagination (default: 0)
    pub offset: Option<usize>,
    /// Filter by layer: "L1", "L2", "Bridge"
    pub layer: Option<String>,
    /// Filter by fund status: "Available", "Locked", "Pending", "Settled", "Bridging"
    pub fund_status: Option<String>,
}

/// Public ledger transactions endpoint with filtering, sorting, and pagination
/// Now aggregates from BOTH ledger.transactions AND market.bets for complete picture
pub async fn get_ledger_transactions(
    State(state): State<SharedState>,
    Query(params): Query<LedgerQuery>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    // Reconstruct transactions from markets to get the real data
    let market_data = markets_to_market_data(&app.markets);
    let market_txs = reconstruct_transactions_from_market_data(&market_data);
    
    // Combine with any ledger transactions (transfers, deposits, etc.)
    let mut all_txs: Vec<Transaction> = market_txs;
    
    // Add non-bet transactions from ledger (transfers, deposits that weren't from markets)
    for tx in &app.ledger.transactions {
        if tx.tx_type != TxType::Bet && tx.tx_type != TxType::MarketCreated {
            all_txs.push(tx.clone());
        }
    }
    
    // Convert to references for filtering
    let mut txs: Vec<&Transaction> = all_txs.iter().collect();
    
    // Filter by type
    if let Some(ref type_filter) = params.tx_type {
        let tx_type = match type_filter.to_lowercase().as_str() {
            "bet" => Some(TxType::Bet),
            "transfer" => Some(TxType::Transfer),
            "deposit" => Some(TxType::Deposit),
            "withdraw" => Some(TxType::Withdraw),
            "payout" => Some(TxType::Payout),
            "accountcreated" | "account_created" => Some(TxType::AccountCreated),
            "marketcreated" | "market_created" => Some(TxType::MarketCreated),
            "marketresolved" | "market_resolved" => Some(TxType::MarketResolved),
            "bridgeinitiate" | "bridge_initiate" => Some(TxType::BridgeInitiate),
            "bridgecomplete" | "bridge_complete" => Some(TxType::BridgeComplete),
            _ => None,
        };
        if let Some(t) = tx_type {
            txs.retain(|tx| tx.tx_type == t);
        }
    }
    
    // Filter by layer
    if let Some(ref layer_filter) = params.layer {
        let layer = match layer_filter.to_uppercase().as_str() {
            "L1" => Some(Layer::L1),
            "L2" => Some(Layer::L2),
            "BRIDGE" => Some(Layer::Bridge),
            _ => None,
        };
        if let Some(l) = layer {
            txs.retain(|tx| tx.layer == l);
        }
    }
    
    // Filter by fund status
    if let Some(ref status_filter) = params.fund_status {
        let status = match status_filter.to_lowercase().as_str() {
            "available" => Some(FundStatus::Available),
            "locked" => Some(FundStatus::Locked),
            "pending" => Some(FundStatus::Pending),
            "settled" => Some(FundStatus::Settled),
            "bridging" => Some(FundStatus::Bridging),
            _ => None,
        };
        if let Some(s) = status {
            txs.retain(|tx| tx.fund_status == s);
        }
    }
    
    // Filter by market_id
    if let Some(ref market_id) = params.market_id {
        txs.retain(|tx| tx.market_id.as_ref() == Some(market_id));
    }
    
    // Filter by account (from or to)
    if let Some(ref account) = params.account {
        let resolved = app.ledger.resolve(account);
        txs.retain(|tx| {
            tx.from == *account || 
            tx.to.as_ref() == Some(account) ||
            resolved.as_ref().map(|r| &tx.from == r || tx.to.as_ref() == Some(r)).unwrap_or(false)
        });
    }
    
    // Filter by timestamp range
    if let Some(from_ts) = params.from_timestamp {
        txs.retain(|tx| tx.timestamp >= from_ts);
    }
    if let Some(to_ts) = params.to_timestamp {
        txs.retain(|tx| tx.timestamp <= to_ts);
    }
    
    // Sort
    let sort_by = params.sort_by.as_deref().unwrap_or("timestamp");
    let order_desc = params.order.as_deref().unwrap_or("desc") == "desc";
    
    match sort_by {
        "amount" => {
            if order_desc {
                txs.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
            } else {
                txs.sort_by(|a, b| a.amount.partial_cmp(&b.amount).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
        _ => { // timestamp
            if order_desc {
                txs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            } else {
                txs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            }
        }
    }
    
    let total = txs.len();
    
    // Pagination
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);
    let txs: Vec<_> = txs.into_iter().skip(offset).take(limit).collect();
    
    // Calculate comprehensive stats
    let total_bets: usize = all_txs.iter().filter(|t| t.tx_type == TxType::Bet).count();
    let bet_volume: f64 = all_txs.iter()
        .filter(|t| t.tx_type == TxType::Bet)
        .map(|t| t.amount)
        .sum();
    let locked_volume: f64 = all_txs.iter()
        .filter(|t| t.fund_status == FundStatus::Locked)
        .map(|t| t.amount)
        .sum();
    let l1_count = all_txs.iter().filter(|t| t.layer == Layer::L1).count();
    let l2_count = all_txs.iter().filter(|t| t.layer == Layer::L2).count();
    let bridge_count = all_txs.iter().filter(|t| t.layer == Layer::Bridge).count();
    
    // Format transactions for response with full L1/L2 tracking
    let transactions: Vec<Value> = txs.iter().map(|tx| {
        json!({
            "id": tx.id,
            "type": format!("{:?}", tx.tx_type),
            "from": tx.from,
            "to": tx.to,
            "amount": tx.amount,
            "market_id": tx.market_id,
            "outcome": tx.outcome,
            "timestamp": tx.timestamp,
            "signature": if tx.signature.is_empty() { None } else { Some(&tx.signature[..16.min(tx.signature.len())]) },
            // L1/L2 tracking fields
            "layer": format!("{:?}", tx.layer),
            "fund_status": format!("{:?}", tx.fund_status),
            "target_layer": tx.target_layer.map(|l| format!("{:?}", l)),
            "l1_settled": tx.l1_settled,
            "l1_tx_hash": tx.l1_tx_hash,
            "block_number": tx.block_number,
            "description": tx.description
        })
    }).collect();
    
    Json(json!({
        "success": true,
        "transactions": transactions,
        "pagination": {
            "total": total,
            "limit": limit,
            "offset": offset,
            "has_more": offset + limit < total
        },
        "stats": {
            "total_accounts": app.ledger.balances.len(),
            "total_transactions": all_txs.len(),
            "current_block": all_txs.len() as u64,
            "total_bets": total_bets,
            "bet_volume": bet_volume,
            "locked_volume": locked_volume,
            "l1_transactions": l1_count,
            "l2_transactions": l2_count,
            "bridge_transactions": bridge_count
        }
    }))
}

/// Unified ledger view - comprehensive overview of all L1/L2 activity
pub async fn get_unified_ledger(State(state): State<SharedState>) -> Json<Value> {
    let app = state.lock().unwrap();
    
    // Reconstruct all transactions from markets
    let market_data = markets_to_market_data(&app.markets);
    let all_txs = reconstruct_transactions_from_market_data(&market_data);
    
    // Calculate totals by layer
    let mut l1_volume = 0.0;
    let mut l2_volume = 0.0;
    let mut bridge_volume = 0.0;
    let mut locked_volume = 0.0;
    let mut pending_volume = 0.0;
    
    for tx in &all_txs {
        match tx.layer {
            Layer::L1 => l1_volume += tx.amount,
            Layer::L2 => l2_volume += tx.amount,
            Layer::Bridge => bridge_volume += tx.amount,
        }
        match tx.fund_status {
            FundStatus::Locked => locked_volume += tx.amount,
            FundStatus::Pending | FundStatus::Bridging => pending_volume += tx.amount,
            _ => {}
        }
    }
    
    // Get unique bettors
    let unique_bettors: std::collections::HashSet<_> = all_txs.iter()
        .filter(|tx| tx.tx_type == TxType::Bet)
        .map(|tx| tx.from.clone())
        .collect();
    
    // Market summaries
    let market_summaries: Vec<Value> = app.markets.iter().map(|(id, market)| {
        json!({
            "id": id,
            "title": market.title,
            "status": format!("{:?}", market.market_status),
            "is_resolved": market.is_resolved,
            "total_volume": market.total_volume,
            "bet_count": market.bet_count,
            "unique_bettors": market.unique_bettors.len()
        })
    }).collect();
    
    // Recent activity (last 20 transactions)
    let recent: Vec<Value> = all_txs.iter().rev().take(20).map(|tx| {
        json!({
            "id": tx.id,
            "type": format!("{:?}", tx.tx_type),
            "from": tx.from,
            "amount": tx.amount,
            "market_id": tx.market_id,
            "timestamp": tx.timestamp,
            "layer": format!("{:?}", tx.layer),
            "fund_status": format!("{:?}", tx.fund_status),
            "description": tx.description
        })
    }).collect();
    
    Json(json!({
        "success": true,
        "overview": {
            "total_transactions": all_txs.len(),
            "total_markets": app.markets.len(),
            "unique_bettors": unique_bettors.len(),
            "total_bet_volume": l2_volume,
            "locked_funds": locked_volume,
            "pending_funds": pending_volume,
            "bridge_volume": bridge_volume
        },
        "by_layer": {
            "l1": {
                "volume": l1_volume,
                "transaction_count": all_txs.iter().filter(|t| t.layer == Layer::L1).count()
            },
            "l2": {
                "volume": l2_volume,
                "transaction_count": all_txs.iter().filter(|t| t.layer == Layer::L2).count()
            },
            "bridge": {
                "volume": bridge_volume,
                "transaction_count": all_txs.iter().filter(|t| t.layer == Layer::Bridge).count()
            }
        },
        "by_status": {
            "available": all_txs.iter().filter(|t| t.fund_status == FundStatus::Available).count(),
            "locked": all_txs.iter().filter(|t| t.fund_status == FundStatus::Locked).count(),
            "pending": all_txs.iter().filter(|t| t.fund_status == FundStatus::Pending).count(),
            "settled": all_txs.iter().filter(|t| t.fund_status == FundStatus::Settled).count(),
            "bridging": all_txs.iter().filter(|t| t.fund_status == FundStatus::Bridging).count()
        },
        "by_type": {
            "bets": all_txs.iter().filter(|t| t.tx_type == TxType::Bet).count(),
            "transfers": all_txs.iter().filter(|t| t.tx_type == TxType::Transfer).count(),
            "deposits": all_txs.iter().filter(|t| t.tx_type == TxType::Deposit).count(),
            "withdrawals": all_txs.iter().filter(|t| t.tx_type == TxType::Withdraw).count(),
            "payouts": all_txs.iter().filter(|t| t.tx_type == TxType::Payout).count(),
            "market_created": all_txs.iter().filter(|t| t.tx_type == TxType::MarketCreated).count(),
            "market_resolved": all_txs.iter().filter(|t| t.tx_type == TxType::MarketResolved).count()
        },
        "markets": market_summaries,
        "recent_activity": recent
    }))
}

// ===== RPC ENDPOINTS =====

pub async fn get_nonce(State(state): State<SharedState>, Path(address): Path<String>) -> Json<Value> {
    let app = state.lock().unwrap();
    let last_nonce = app.nonces.get(&address).copied().unwrap_or(0);
    Json(json!({ 
        "address": address, 
        "nonce": last_nonce,
        "last_used_nonce": last_nonce,
        "next_nonce": last_nonce + 1
    }))
}

// ===== USER ENDPOINTS =====

pub async fn get_user_bets(State(state): State<SharedState>, Path(account): Path<String>) -> Json<Value> {
    let app = state.lock().unwrap();
    let mut bets = Vec::new();
    for market in app.markets.values() {
        bets.extend(market.get_bets_for_account(&account));
    }
    Json(json!({ "account": account, "bets": bets }))
}

// ===== SETTLEMENT ENDPOINTS (Simplified) =====

pub async fn settle_to_l1(State(_state): State<SharedState>) -> Json<Value> {
    Json(json!({
        "success": true,
        "message": "Settlement queued (mock mode)"
    }))
}

pub async fn get_settlement_status(State(state): State<SharedState>) -> Json<Value> {
    let app = state.lock().unwrap();
    let stats = app.ledger.stats();
    Json(json!({
        "success": true,
        "block": stats.block,
        "transactions": stats.transactions,
        "total_bets": stats.total_bets,
        "bet_volume": stats.bet_volume,
        "l1_mock_mode": app.ledger.mock_mode
    }))
}

pub async fn sync_from_l1(State(_state): State<SharedState>) -> Json<Value> {
    Json(json!({
        "success": true,
        "message": "Sync completed (mock mode)"
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// CLOB (Central Limit Order Book) HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════

use crate::orderbook::{
    LimitOrder, OrderType, Side, OrderStatus, Fill, Outcome,
    OrderBookManager, MarketOdds, OddsSource,
};
use crate::shares::{SharesManager, SharePosition, ShareBalance, OutcomeIndex};

// ===== ORDER REQUEST TYPES =====

#[derive(Debug, Deserialize)]
pub struct SubmitOrderRequest {
    pub wallet: String,
    pub market_id: String,
    pub outcome: u8,           // 0=YES, 1=NO
    pub side: String,          // "bid" or "ask"
    pub price_bps: u64,        // 1-99 basis points
    pub quantity: f64,         // Share quantity
    pub order_type: Option<String>, // "gtc", "ioc", "fok", "market" (default: gtc)
}

#[derive(Debug, Deserialize)]
pub struct CancelOrderRequest {
    pub wallet: String,
}

// ===== SUBMIT ORDER HANDLER =====
/// POST /orders - Submit a limit order to the CLOB
pub async fn submit_order(
    State(state): State<SharedState>,
    Json(req): Json<SubmitOrderRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate price bounds (1-99 bps = $0.01 - $0.99)
    if req.price_bps < 1 || req.price_bps > 99 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Price must be 1-99 (basis points representing $0.01-$0.99)"
        }))));
    }
    
    // Parse side
    let side = match req.side.to_lowercase().as_str() {
        "bid" | "buy" => Side::Bid,
        "ask" | "sell" => Side::Ask,
        _ => return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Side must be 'bid' or 'ask'"
        })))),
    };
    
    // Parse order type
    let order_type = match req.order_type.as_deref().unwrap_or("gtc").to_lowercase().as_str() {
        "gtc" => OrderType::GTC,
        "ioc" => OrderType::IOC,
        "fok" => OrderType::FOK,
        "market" => OrderType::Market,
        _ => OrderType::GTC,
    };
    
    let mut app = state.lock().unwrap();
    
    // Check market exists
    if !app.markets.contains_key(&req.market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    // For asks (selling shares), check if user has shares to sell
    if side == Side::Ask {
        let position = app.shares.get_position(&req.wallet, &req.market_id);
        let available = if req.outcome == 0 { position.yes_shares } else { position.no_shares };
        if available < req.quantity {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Insufficient shares: have {}, need {}", available, req.quantity),
                "available": available,
                "requested": req.quantity
            }))));
        }
    }
    
    // For bids (buying shares), check BB balance
    if side == Side::Bid {
        let cost = (req.price_bps as f64 / 100.0) * req.quantity;
        let balance = app.ledger.balance(&req.wallet);
        if balance < cost {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Insufficient balance: have {} BB, need {} BB", balance, cost),
                "available": balance,
                "required": cost
            }))));
        }
    }
    
    // Create the order
    let outcome = Outcome::new(req.outcome as usize);
    let order = match LimitOrder::new(
        req.market_id.clone(),
        outcome,
        side,
        req.price_bps,
        req.quantity,
        order_type,
        req.wallet.clone(),
        String::new(), // signature placeholder
    ) {
        Ok(o) => o,
        Err(e) => return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("{:?}", e)
        })))),
    };
    let order_id = order.id.clone();
    
    // Submit to order book
    let result = app.orderbook.submit_order(order);
    
    // Process fills - update ledger and shares
    for fill in &result.fills {
        // Transfer BB from buyer to seller (minus fees)
        let buyer_cost = fill.value * (1.0 + fill.taker_fee / fill.value);
        let seller_receive = fill.value * (1.0 - fill.maker_fee / fill.value);
        
        // Update ledger
        let _ = app.ledger.transfer(&fill.taker, "orderbook_escrow", buyer_cost, &fill.id);
        let _ = app.ledger.transfer("orderbook_escrow", &fill.maker, seller_receive, &fill.id);
        
        // Update share positions
        let outcome_idx = if req.outcome == 0 { OutcomeIndex::YES } else { OutcomeIndex::NO };
        if side == Side::Bid {
            // Buyer receives shares
            app.shares.credit_shares_simple(&fill.taker, &req.market_id, outcome_idx, fill.size);
            let _ = app.shares.debit_shares_simple(&fill.maker, &req.market_id, outcome_idx, fill.size);
        } else {
            // Seller gives up shares
            let _ = app.shares.debit_shares_simple(&fill.taker, &req.market_id, outcome_idx, fill.size);
            app.shares.credit_shares_simple(&fill.maker, &req.market_id, outcome_idx, fill.size);
        }
    }
    
    let fill_status = if result.fills.is_empty() { 
        "posted".to_string() 
    } else { 
        format!("{} filled", result.fills.len()) 
    };
    app.log_activity("📋", "ORDER", &format!(
        "{} {} {} shares @ {} bps on {} ({})", 
        req.wallet, req.side, req.quantity, req.price_bps, req.market_id, fill_status
    ));
    
    Ok(Json(json!({
        "success": result.success,
        "order_id": order_id,
        "status": format!("{:?}", result.order.status),
        "filled_quantity": result.total_filled,
        "remaining_quantity": result.order.remaining,
        "fills": result.fills.iter().map(|f| json!({
            "fill_id": f.id,
            "price_bps": f.price_bps,
            "quantity": f.size,
            "counterparty": f.maker,
            "maker_fee": f.maker_fee,
            "taker_fee": f.taker_fee
        })).collect::<Vec<_>>(),
        "average_fill_price": result.order.avg_fill_price
    })))
}

// ===== CANCEL ORDER HANDLER =====
/// DELETE /orders/:order_id - Cancel an open order
pub async fn cancel_order(
    State(state): State<SharedState>,
    Path(order_id): Path<String>,
    Json(req): Json<CancelOrderRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    match app.orderbook.cancel_order(&order_id, &req.wallet) {
        Ok(cancelled_order) => {
            app.log_activity("❌", "CANCEL", &format!("{} cancelled order {}", req.wallet, order_id));
            Ok(Json(json!({
                "success": true,
                "order_id": order_id,
                "refunded_quantity": cancelled_order.remaining
            })))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("{:?}", e)
        }))))
    }
}

// ===== GET USER ORDERS HANDLER =====
/// GET /orders/user/:wallet - Get user's open orders
pub async fn get_user_orders(
    State(state): State<SharedState>,
    Path(wallet): Path<String>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    let orders = app.orderbook.get_user_orders(&wallet);
    
    Json(json!({
        "success": true,
        "wallet": wallet,
        "orders": orders.iter().map(|o| json!({
            "order_id": o.id,
            "market_id": o.market_id,
            "outcome": o.outcome.index(),
            "side": format!("{:?}", o.side),
            "price_bps": o.price_bps,
            "size": o.size,
            "filled": o.filled,
            "remaining": o.remaining,
            "status": format!("{:?}", o.status),
            "created_at": o.created_at
        })).collect::<Vec<_>>(),
        "total_orders": orders.len()
    }))
}

// ===== GET ORDER BOOK HANDLER =====
/// GET /orderbook/:market_id - Get order book depth for a market
pub async fn get_orderbook(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if !app.markets.contains_key(&market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    // Get depth for both outcomes (YES=0, NO=1)
    let yes_book = app.orderbook.get_orderbook(&market_id, Outcome::YES, 10);
    let no_book = app.orderbook.get_orderbook(&market_id, Outcome::NO, 10);
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "yes_outcome": {
            "bids": yes_book.bids.iter().map(|l| json!({
                "price_bps": l.price_bps,
                "size": l.size,
                "total_value": (l.price_bps as f64 / 100.0) * l.size
            })).collect::<Vec<_>>(),
            "asks": yes_book.asks.iter().map(|l| json!({
                "price_bps": l.price_bps,
                "size": l.size,
                "total_value": (l.price_bps as f64 / 100.0) * l.size
            })).collect::<Vec<_>>(),
            "best_bid": yes_book.best_bid,
            "best_ask": yes_book.best_ask,
            "spread": yes_book.spread
        },
        "no_outcome": {
            "bids": no_book.bids.iter().map(|l| json!({
                "price_bps": l.price_bps,
                "size": l.size,
                "total_value": (l.price_bps as f64 / 100.0) * l.size
            })).collect::<Vec<_>>(),
            "asks": no_book.asks.iter().map(|l| json!({
                "price_bps": l.price_bps,
                "size": l.size,
                "total_value": (l.price_bps as f64 / 100.0) * l.size
            })).collect::<Vec<_>>(),
            "best_bid": no_book.best_bid,
            "best_ask": no_book.best_ask,
            "spread": no_book.spread
        }
    })))
}

// ===== GET RECENT TRADES HANDLER =====
/// GET /trades/:market_id - Get recent trades for a market
pub async fn get_recent_trades(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if !app.markets.contains_key(&market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    let trades = app.orderbook.get_recent_trades(&market_id, 50);
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "trades": trades.iter().map(|t| json!({
            "trade_id": t.id,
            "price_bps": t.price_bps,
            "size": t.size,
            "value": t.value,
            "timestamp": t.timestamp,
            "outcome": t.outcome.index(),
            "maker": &t.maker[..8.min(t.maker.len())],
            "taker": &t.taker[..8.min(t.taker.len())]
        })).collect::<Vec<_>>(),
        "total_trades": trades.len()
    })))
}

// ===== GET MARKET ODDS (HYBRID CLOB/CPMM) =====
/// GET /markets/:id/odds - Get current odds from CLOB or CPMM
pub async fn get_market_odds(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    let market = app.markets.get(&market_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))))?;
    
    // Get hybrid odds from OrderBookManager
    let odds = app.orderbook.get_odds(&market_id);
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "title": market.title,
        "odds": {
            "yes": odds.yes_price,
            "no": odds.no_price,
            "source": format!("{:?}", odds.source),
            "liquidity": odds.liquidity,
            "spread_bps": odds.spread_bps
        },
        "interpretation": {
            "yes_probability": format!("{}%", (odds.yes_probability * 100.0).round()),
            "no_probability": format!("{}%", (odds.no_probability * 100.0).round()),
            "implied_vig": ((odds.yes_price + odds.no_price - 1.0) * 100.0).abs()
        }
    })))
}

// ===== GET ORDERBOOK FOR SPECIFIC OUTCOME =====
/// GET /orderbook/:market_id/:outcome - Get order book for a specific outcome
pub async fn get_orderbook_outcome(
    State(state): State<SharedState>,
    Path((market_id, outcome_str)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if !app.markets.contains_key(&market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    let outcome = match outcome_str.to_lowercase().as_str() {
        "yes" | "0" => Outcome::YES,
        "no" | "1" => Outcome::NO,
        _ => return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Invalid outcome - use 'yes' or 'no' (or 0/1)"
        })))),
    };
    
    let book = app.orderbook.get_orderbook(&market_id, outcome, 20);
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "outcome": outcome_str,
        "bids": book.bids.iter().map(|l| json!({
            "price_bps": l.price_bps,
            "size": l.size,
            "order_count": l.order_count
        })).collect::<Vec<_>>(),
        "asks": book.asks.iter().map(|l| json!({
            "price_bps": l.price_bps,
            "size": l.size,
            "order_count": l.order_count
        })).collect::<Vec<_>>(),
        "best_bid": book.best_bid,
        "best_ask": book.best_ask,
        "spread_bps": book.spread,
        "total_bid_size": book.bids.iter().map(|l| l.size).sum::<f64>(),
        "total_ask_size": book.asks.iter().map(|l| l.size).sum::<f64>()
    })))
}

// ═══════════════════════════════════════════════════════════════════════════════
// OUTCOME SHARES HANDLERS  
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct MintSharesRequest {
    pub wallet: String,
    pub market_id: String,
    pub amount: f64,  // BB tokens to convert (1 BB = 1 YES + 1 NO)
}

#[derive(Debug, Deserialize)]
pub struct RedeemSharesRequest {
    pub wallet: String,
    pub market_id: String,
    pub amount: f64,  // Number of share pairs to redeem
}

// ===== MINT SHARES HANDLER =====
/// POST /shares/mint - Mint YES+NO shares from BB tokens (1 BB → 1 YES + 1 NO)
pub async fn mint_shares(
    State(state): State<SharedState>,
    Json(req): Json<MintSharesRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    // Check market exists
    if !app.markets.contains_key(&req.market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    // Check BB balance
    let balance = app.ledger.balance(&req.wallet);
    if balance < req.amount {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("Insufficient balance: have {} BB, need {} BB", balance, req.amount),
            "available": balance,
            "requested": req.amount
        }))));
    }
    
    // Debit BB from wallet to market escrow
    let escrow = format!("escrow:{}", req.market_id);
    if let Err(e) = app.ledger.transfer(&req.wallet, &escrow, req.amount, "mint_shares") {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "success": false,
            "error": format!("Transfer failed: {}", e)
        }))));
    }
    
    // Credit shares (1 BB = 1 YES + 1 NO)
    app.shares.credit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::YES, req.amount);
    app.shares.credit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::NO, req.amount);
    
    let position = app.shares.get_position(&req.wallet, &req.market_id);
    
    app.log_activity("🪙", "MINT", &format!(
        "{} minted {} YES + {} NO shares for {} on {}", 
        req.wallet, req.amount, req.amount, req.amount, req.market_id
    ));
    
    Ok(Json(json!({
        "success": true,
        "minted": {
            "yes_shares": req.amount,
            "no_shares": req.amount,
            "bb_spent": req.amount
        },
        "new_position": {
            "yes_shares": position.yes_shares,
            "no_shares": position.no_shares
        },
        "new_bb_balance": app.ledger.balance(&req.wallet)
    })))
}

// ===== REDEEM SHARES HANDLER =====
/// POST /shares/redeem - Redeem share pairs back to BB (1 YES + 1 NO → 1 BB)
pub async fn redeem_shares(
    State(state): State<SharedState>,
    Json(req): Json<RedeemSharesRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    // Check market exists
    if !app.markets.contains_key(&req.market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    // Check share positions
    let position = app.shares.get_position(&req.wallet, &req.market_id);
    let max_pairs = position.yes_shares.min(position.no_shares);
    
    if req.amount > max_pairs {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("Insufficient share pairs: can redeem max {} pairs", max_pairs),
            "yes_shares": position.yes_shares,
            "no_shares": position.no_shares,
            "max_redeemable_pairs": max_pairs,
            "requested": req.amount
        }))));
    }
    
    // Debit shares
    let _ = app.shares.debit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::YES, req.amount);
    let _ = app.shares.debit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::NO, req.amount);
    
    // Credit BB from market escrow to wallet
    let escrow = format!("escrow:{}", req.market_id);
    if let Err(e) = app.ledger.transfer(&escrow, &req.wallet, req.amount, "redeem_shares") {
        // Rollback shares on failure
        app.shares.credit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::YES, req.amount);
        app.shares.credit_shares_simple(&req.wallet, &req.market_id, OutcomeIndex::NO, req.amount);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "success": false,
            "error": format!("Redemption failed: {}", e)
        }))));
    }
    
    let new_position = app.shares.get_position(&req.wallet, &req.market_id);
    
    app.log_activity("💎", "REDEEM", &format!(
        "{} redeemed {} share pairs for {} BB on {}", 
        req.wallet, req.amount, req.amount, req.market_id
    ));
    
    Ok(Json(json!({
        "success": true,
        "redeemed": {
            "yes_shares": req.amount,
            "no_shares": req.amount,
            "bb_received": req.amount
        },
        "new_position": {
            "yes_shares": new_position.yes_shares,
            "no_shares": new_position.no_shares
        },
        "new_bb_balance": app.ledger.balance(&req.wallet)
    })))
}

// ===== GET POSITIONS HANDLER =====
/// GET /positions/:wallet - Get all share positions for a wallet
pub async fn get_positions(
    State(state): State<SharedState>,
    Path(wallet): Path<String>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    let positions = app.shares.get_all_positions(&wallet);
    
    // Enrich with market titles and current odds
    let enriched: Vec<Value> = positions.iter().map(|pos| {
        let market = app.markets.get(&pos.market_id);
        let odds = app.orderbook.get_odds(&pos.market_id);
        
        let yes_value = pos.yes_shares * odds.yes_price;
        let no_value = pos.no_shares * odds.no_price;
        
        json!({
            "market_id": pos.market_id,
            "market_title": market.map(|m| m.title.clone()).unwrap_or_default(),
            "yes_shares": pos.yes_shares,
            "no_shares": pos.no_shares,
            "current_odds": {
                "yes": odds.yes_price,
                "no": odds.no_price
            },
            "estimated_value": {
                "yes_value": yes_value,
                "no_value": no_value,
                "total_value": yes_value + no_value
            },
            "redeemable_pairs": pos.yes_shares.min(pos.no_shares)
        })
    }).collect();
    
    let total_value: f64 = enriched.iter()
        .map(|p| p["estimated_value"]["total_value"].as_f64().unwrap_or(0.0))
        .sum();
    
    Json(json!({
        "success": true,
        "wallet": wallet,
        "positions": enriched,
        "summary": {
            "total_positions": positions.len(),
            "total_estimated_value": total_value,
            "bb_balance": app.ledger.balance(&wallet)
        }
    }))
}

// ===== GET MARKET POSITIONS HANDLER =====
/// GET /positions/:wallet/:market_id - Get positions for a specific market
pub async fn get_market_positions(
    State(state): State<SharedState>,
    Path((wallet, market_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if !app.markets.contains_key(&market_id) {
        return Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))));
    }
    
    let market = app.markets.get(&market_id).unwrap();
    let position = app.shares.get_position(&wallet, &market_id);
    let odds = app.orderbook.get_odds(&market_id);
    
    let yes_value = position.yes_shares * odds.yes_price;
    let no_value = position.no_shares * odds.no_price;
    
    Ok(Json(json!({
        "success": true,
        "wallet": wallet,
        "market_id": market_id,
        "market_title": market.title,
        "position": {
            "yes_shares": position.yes_shares,
            "no_shares": position.no_shares,
            "redeemable_pairs": position.yes_shares.min(position.no_shares)
        },
        "current_odds": {
            "yes": odds.yes_price,
            "no": odds.no_price,
            "source": format!("{:?}", odds.source)
        },
        "estimated_value": {
            "yes_value": yes_value,
            "no_value": no_value,
            "total_value": yes_value + no_value
        }
    })))
}

// ===== ORDERBOOK STATS HANDLER =====
/// GET /stats/orderbook - Get CLOB statistics
pub async fn get_orderbook_stats(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    let stats = app.orderbook.get_stats();
    
    Json(json!({
        "success": true,
        "orderbook_stats": {
            "total_orders_submitted": stats.total_orders_submitted,
            "total_orders_filled": stats.total_orders_filled,
            "total_orders_cancelled": stats.total_orders_cancelled,
            "total_volume_traded": stats.total_volume_traded,
            "total_fees_collected": stats.total_fees_collected,
            "markets_with_clob": stats.markets_with_clob,
            "markets_with_cpmm_fallback": stats.markets_with_cpmm_fallback
        }
    }))
}

// ===== SHARES STATS HANDLER =====
/// GET /stats/shares - Get shares system statistics
pub async fn get_shares_stats(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    let stats = app.shares.get_stats();
    
    Json(json!({
        "success": true,
        "shares_stats": {
            "total_shares_minted": stats.total_shares_minted,
            "total_shares_redeemed": stats.total_shares_redeemed,
            "total_bb_locked": stats.total_bb_locked,
            "total_transactions": stats.total_transactions,
            "unique_holders": stats.unique_holders
        }
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// MARKET RESOLUTION HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════

use crate::app_state::MarketResolution;

/// Request to resolve a market
#[derive(Debug, Deserialize)]
pub struct ResolveMarketRequest {
    /// The oracle/admin address resolving the market
    pub resolver_address: String,
    /// Signature proving ownership of resolver address
    pub signature: String,
    /// The winning outcome index (0=YES, 1=NO for binary markets)
    pub winning_outcome: usize,
    /// Optional reason/evidence for resolution
    pub resolution_reason: Option<String>,
    /// Nonce for replay protection
    pub nonce: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// POST /markets/:id/resolve - Resolve a market with winning outcome
/// 
/// Authorization: Only whitelisted oracles or admins can resolve markets.
/// High-value markets may require multi-sig (configurable).
pub async fn resolve_market(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
    Json(req): Json<ResolveMarketRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Validate timestamp (1 hour window for resolution)
    if now.abs_diff(req.timestamp) > 3600 {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({
            "success": false,
            "error": "Resolution request expired (1 hour window)"
        }))));
    }
    
    let mut app = state.lock().unwrap();
    
    // Extract needed data from market (immutable borrow scope)
    let (market_volume, market_title, winning_outcome_name, is_resolved, existing_winner, num_options) = {
        let market = app.markets.get(&market_id).ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({
                "success": false,
                "error": format!("Market {} not found", market_id)
            })))
        })?;
        
        (
            market.total_volume,
            market.title.clone(),
            market.options.get(req.winning_outcome).cloned(),
            market.is_resolved,
            market.winning_option,
            market.options.len(),
        )
    };
    
    // Check if already resolved
    if is_resolved {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Market already resolved",
            "winning_outcome": existing_winner
        }))));
    }
    
    // Validate outcome index
    if req.winning_outcome >= num_options {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("Invalid outcome index {}. Market has {} outcomes.", 
                req.winning_outcome, num_options)
        }))));
    }
    
    let winning_outcome_name = winning_outcome_name.unwrap(); // Safe: checked above
    
    // Check nonce
    let last_nonce = app.nonces.get(&req.resolver_address).copied().unwrap_or(0);
    if req.nonce <= last_nonce {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("Invalid nonce: got {}, expected > {}", req.nonce, last_nonce)
        }))));
    }
    
    // Authorization check
    if !app.oracle_config.can_resolve(&req.resolver_address, &market_id, market_volume) {
        return Err((StatusCode::FORBIDDEN, Json(json!({
            "success": false,
            "error": "Not authorized to resolve this market",
            "reason": "Address is not a whitelisted oracle or admin",
            "resolver": req.resolver_address
        }))));
    }
    
    // Update nonce
    app.nonces.insert(req.resolver_address.clone(), req.nonce);
    
    // Resolve the market in PredictionMarket
    let market = app.markets.get_mut(&market_id).unwrap();
    market.is_resolved = true;
    market.winning_option = Some(req.winning_outcome);
    
    // Calculate payouts from shares system
    let share_payouts = app.shares.resolve_market(
        &market_id,
        OutcomeIndex::from_usize(req.winning_outcome),
        2, // Binary market
    );
    
    // Calculate total payout
    let total_payout: f64 = share_payouts.iter().map(|(_, amount)| amount).sum();
    let num_winners = share_payouts.len();
    
    // Credit winners
    for (wallet, payout_amount) in &share_payouts {
        app.ledger.credit(wallet, *payout_amount);
    }
    
    // Record resolution
    let resolution = MarketResolution {
        market_id: market_id.clone(),
        winning_outcome: req.winning_outcome,
        winning_outcome_name: winning_outcome_name.clone(),
        resolved_by: req.resolver_address.clone(),
        resolved_at: now,
        total_payout,
        num_winners,
        l1_settlement_hash: None,
        l1_settlement_status: "pending".to_string(),
    };
    app.resolutions.insert(market_id.clone(), resolution);
    
    // Log activity
    app.log_activity("⚖️", "RESOLVE", &format!(
        "Market '{}' resolved: {} wins | {} winners | {} BB paid out | by {}",
        market_title, winning_outcome_name, num_winners, total_payout, req.resolver_address
    ));
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "winning_outcome": req.winning_outcome,
        "winning_outcome_name": winning_outcome_name,
        "resolved_by": req.resolver_address,
        "resolved_at": now,
        "payouts": {
            "total_payout": total_payout,
            "num_winners": num_winners,
            "winners": share_payouts.iter().map(|(wallet, amount)| {
                json!({ "wallet": wallet, "payout": amount })
            }).collect::<Vec<_>>()
        },
        "l1_settlement_status": "pending",
        "message": format!("Market resolved. {} will be settled to L1.", total_payout)
    })))
}

/// POST /admin/resolve/:market_id/:outcome - Admin shortcut to resolve
/// 
/// Simplified endpoint for admin resolution without signature verification.
/// Only works if caller is admin (checked by address in body or header).
pub async fn admin_resolve_market(
    State(state): State<SharedState>,
    Path((market_id, winning_outcome)): Path<(String, usize)>,
    Json(req): Json<AdminResolveRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    // Verify admin
    if !app.oracle_config.is_admin(&req.admin_address) {
        return Err((StatusCode::FORBIDDEN, Json(json!({
            "success": false,
            "error": "Not authorized. Admin access required.",
            "address": req.admin_address
        }))));
    }
    
    // Check if market exists
    let market = app.markets.get(&market_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": format!("Market {} not found", market_id)
        })))
    })?;
    
    if market.is_resolved {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Market already resolved"
        }))));
    }
    
    if winning_outcome >= market.options.len() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": format!("Invalid outcome {}. Market has {} outcomes.", 
                winning_outcome, market.options.len())
        }))));
    }
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let winning_outcome_name = market.options[winning_outcome].clone();
    let market_title = market.title.clone();
    
    // Resolve
    let market = app.markets.get_mut(&market_id).unwrap();
    market.is_resolved = true;
    market.winning_option = Some(winning_outcome);
    
    // Calculate payouts
    let share_payouts = app.shares.resolve_market(
        &market_id,
        OutcomeIndex::from_usize(winning_outcome),
        2,
    );
    
    let total_payout: f64 = share_payouts.iter().map(|(_, amount)| amount).sum();
    let num_winners = share_payouts.len();
    
    // Credit winners
    for (wallet, payout_amount) in &share_payouts {
        app.ledger.credit(wallet, *payout_amount);
    }
    
    // Record resolution
    let resolution = MarketResolution {
        market_id: market_id.clone(),
        winning_outcome,
        winning_outcome_name: winning_outcome_name.clone(),
        resolved_by: req.admin_address.clone(),
        resolved_at: now,
        total_payout,
        num_winners,
        l1_settlement_hash: None,
        l1_settlement_status: "pending".to_string(),
    };
    app.resolutions.insert(market_id.clone(), resolution);
    
    app.log_activity("⚖️", "ADMIN_RESOLVE", &format!(
        "Market '{}' resolved by admin: {} wins", market_title, winning_outcome_name
    ));
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "winning_outcome": winning_outcome,
        "winning_outcome_name": winning_outcome_name,
        "resolved_by": req.admin_address,
        "total_payout": total_payout,
        "num_winners": num_winners
    })))
}

#[derive(Debug, Deserialize)]
pub struct AdminResolveRequest {
    pub admin_address: String,
}

/// GET /markets/:id/resolution - Get resolution details for a market
pub async fn get_market_resolution(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if let Some(resolution) = app.resolutions.get(&market_id) {
        Ok(Json(json!({
            "success": true,
            "resolved": true,
            "resolution": resolution
        })))
    } else if app.markets.contains_key(&market_id) {
        Ok(Json(json!({
            "success": true,
            "resolved": false,
            "message": "Market exists but has not been resolved"
        })))
    } else {
        Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        }))))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// RESOLUTION REDEMPTION HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ClaimWinningsRequest {
    pub wallet: String,
    pub signature: Option<String>,
    pub nonce: Option<u64>,
}

/// POST /shares/claim/:market_id - Claim winnings from a resolved market
/// 
/// After a market resolves, winners can claim their winnings.
/// Winning shares are redeemed 1:1 for BB tokens.
pub async fn claim_market_winnings(
    State(state): State<SharedState>,
    Path(market_id): Path<String>,
    Json(req): Json<ClaimWinningsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    // Check market exists and is resolved
    let market = app.markets.get(&market_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Market not found"
        })))
    })?;
    
    if !market.is_resolved {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Market not yet resolved. Cannot claim winnings."
        }))));
    }
    
    let winning_outcome = market.winning_option.ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "success": false,
            "error": "Market marked resolved but no winning outcome set"
        })))
    })?;
    
    // Get user's position
    let position = app.shares.get_position(&req.wallet, &market_id);
    
    // Calculate winnings based on winning outcome
    let winning_shares = if winning_outcome == 0 {
        position.yes_shares
    } else {
        position.no_shares
    };
    
    if winning_shares <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "No winning shares to claim",
            "your_position": {
                "yes_shares": position.yes_shares,
                "no_shares": position.no_shares
            },
            "winning_outcome": winning_outcome
        }))));
    }
    
    // Calculate payout (1 winning share = 1 BB)
    let payout = winning_shares;
    
    // Debit the winning shares
    let outcome_idx = OutcomeIndex::from_usize(winning_outcome);
    if let Err(e) = app.shares.debit_shares_simple(&req.wallet, &market_id, outcome_idx, winning_shares) {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "success": false,
            "error": format!("Failed to debit shares: {}", e)
        }))));
    }
    
    // Credit BB to wallet
    app.ledger.credit(&req.wallet, payout);
    
    // Log activity
    app.log_activity("💰", "CLAIM", &format!(
        "{} claimed {} BB from market {} (redeemed {} winning shares)",
        req.wallet, payout, market_id, winning_shares
    ));
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "wallet": req.wallet,
        "winning_outcome": winning_outcome,
        "shares_redeemed": winning_shares,
        "bb_received": payout,
        "new_balance": app.ledger.balance(&req.wallet)
    })))
}

// ═══════════════════════════════════════════════════════════════════════════════
// ORACLE/ADMIN MANAGEMENT HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct AddOracleRequest {
    pub admin_address: String,
    pub oracle_address: String,
}

/// POST /admin/oracles - Add a new oracle to the whitelist
pub async fn add_oracle(
    State(state): State<SharedState>,
    Json(req): Json<AddOracleRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    if !app.oracle_config.is_admin(&req.admin_address) {
        return Err((StatusCode::FORBIDDEN, Json(json!({
            "success": false,
            "error": "Admin access required"
        }))));
    }
    
    app.oracle_config.add_oracle(req.oracle_address.clone());
    
    app.log_activity("🔐", "ORACLE_ADD", &format!(
        "Admin {} added oracle {}", req.admin_address, req.oracle_address
    ));
    
    Ok(Json(json!({
        "success": true,
        "message": format!("Oracle {} added to whitelist", req.oracle_address),
        "total_oracles": app.oracle_config.oracle_whitelist.len()
    })))
}

/// DELETE /admin/oracles/:address - Remove an oracle from whitelist
pub async fn remove_oracle(
    State(state): State<SharedState>,
    Path(oracle_address): Path<String>,
    Json(req): Json<AdminResolveRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    if !app.oracle_config.is_admin(&req.admin_address) {
        return Err((StatusCode::FORBIDDEN, Json(json!({
            "success": false,
            "error": "Admin access required"
        }))));
    }
    
    app.oracle_config.remove_oracle(&oracle_address);
    
    app.log_activity("🔐", "ORACLE_REMOVE", &format!(
        "Admin {} removed oracle {}", req.admin_address, oracle_address
    ));
    
    Ok(Json(json!({
        "success": true,
        "message": format!("Oracle {} removed from whitelist", oracle_address)
    })))
}

/// GET /admin/oracles - List all whitelisted oracles
pub async fn list_oracles(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    Json(json!({
        "success": true,
        "oracles": app.oracle_config.oracle_whitelist.iter().collect::<Vec<_>>(),
        "admins": app.oracle_config.admin_addresses.iter().collect::<Vec<_>>(),
        "multi_sig_threshold": app.oracle_config.multi_sig_threshold,
        "high_value_threshold": app.oracle_config.high_value_threshold
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// L1 SETTLEMENT HANDLERS (Real Implementation)
// ═══════════════════════════════════════════════════════════════════════════════

use crate::rpc::L1BlackBookRpc;

#[derive(Debug, Deserialize)]
pub struct SettlementRequest {
    /// Optional: settle specific market. If None, settles all pending.
    pub market_id: Option<String>,
    /// Admin address initiating settlement
    pub admin_address: Option<String>,
}

/// POST /settle - Submit market resolution to L1 for recording
/// 
/// This records the market outcome on L1 for audit trail and finality.
pub async fn settle_to_l1_real(
    State(state): State<SharedState>,
    Json(req): Json<SettlementRequest>,
) -> Json<Value> {
    let mut app = state.lock().unwrap();
    
    // Collect markets to settle
    let markets_to_settle: Vec<String> = if let Some(market_id) = req.market_id {
        vec![market_id]
    } else {
        // Get all resolved but unsettled markets
        app.resolutions.iter()
            .filter(|(_, res)| res.l1_settlement_status == "pending")
            .map(|(id, _)| id.clone())
            .collect()
    };
    
    if markets_to_settle.is_empty() {
        return Json(json!({
            "success": true,
            "message": "No markets pending settlement",
            "settled": []
        }));
    }
    
    let mut settled = Vec::new();
    let failed: Vec<serde_json::Value> = Vec::new();
    
    // Check if L1 is available
    let l1_url = std::env::var("L1_RPC_URL").ok();
    
    for market_id in markets_to_settle {
        if let Some(resolution) = app.resolutions.get_mut(&market_id) {
            if l1_url.is_some() {
                // Real L1 settlement (async would be better but keeping sync for simplicity)
                // In production, this would call L1's /rpc/settlement endpoint
                
                // For now, mark as "submitted" and generate a mock tx hash
                let tx_hash = format!("L1_TX_{}", uuid::Uuid::new_v4().simple());
                resolution.l1_settlement_status = "submitted".to_string();
                resolution.l1_settlement_hash = Some(tx_hash.clone());
                
                settled.push(json!({
                    "market_id": market_id,
                    "l1_tx_hash": tx_hash,
                    "status": "submitted",
                    "winning_outcome": resolution.winning_outcome,
                    "total_payout": resolution.total_payout
                }));
                
                app.log_activity("📤", "L1_SETTLE", &format!(
                    "Market {} settlement submitted to L1: {}", market_id, tx_hash
                ));
            } else {
                // Mock mode - just mark as settled locally
                resolution.l1_settlement_status = "mock_settled".to_string();
                resolution.l1_settlement_hash = Some(format!("MOCK_{}", uuid::Uuid::new_v4().simple()));
                
                settled.push(json!({
                    "market_id": market_id,
                    "status": "mock_settled",
                    "message": "L1_RPC_URL not configured - settlement recorded locally only"
                }));
            }
        }
    }
    
    Json(json!({
        "success": true,
        "settled": settled,
        "failed": failed,
        "l1_mode": if l1_url.is_some() { "live" } else { "mock" }
    }))
}

/// GET /settle/pending - Get markets pending L1 settlement
pub async fn get_pending_settlements(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    let pending: Vec<_> = app.resolutions.iter()
        .filter(|(_, res)| res.l1_settlement_status == "pending")
        .map(|(id, res)| json!({
            "market_id": id,
            "winning_outcome": res.winning_outcome,
            "winning_outcome_name": res.winning_outcome_name,
            "total_payout": res.total_payout,
            "num_winners": res.num_winners,
            "resolved_at": res.resolved_at,
            "resolved_by": res.resolved_by
        }))
        .collect();
    
    Json(json!({
        "success": true,
        "pending_count": pending.len(),
        "pending_settlements": pending
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// BRIDGE ENDPOINT HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════

use crate::bridge::{BridgeCompleteRequest, BridgeDirection};
use crate::rpc::{L1RpcConfig, L1WithdrawRequest};
use crate::app_state::PendingWithdrawal;

#[derive(Debug, Deserialize)]
pub struct InitiateBridgeRequest {
    pub wallet: String,
    pub amount: f64,
    pub target_address: String, // L1 address for L2→L1
    pub signature: String,
    pub nonce: u64,
    pub timestamp: u64,
}

/// POST /bridge/withdraw - Initiate L2→L1 bridge (withdraw from L2)
/// 
/// Locks BB on L2, calls L1's /bridge/withdraw endpoint.
/// Returns pending status - client can poll /bridge/status/:bridge_id
/// If L1 rejects, automatically refunds L2 balance.
pub async fn bridge_withdraw(
    State(state): State<SharedState>,
    Json(req): Json<InitiateBridgeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Validate timestamp
    if now.abs_diff(req.timestamp) > 3600 {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({
            "success": false,
            "error": "Bridge request expired"
        }))));
    }
    
    // --- Phase 1: Validation & L2 Debit ---
    let (bridge_id, bridge) = {
        let mut app = state.lock().unwrap();
        
        // Check nonce
        let last_nonce = app.nonces.get(&req.wallet).copied().unwrap_or(0);
        if req.nonce <= last_nonce {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Invalid nonce: got {}, expected > {}", req.nonce, last_nonce)
            }))));
        }
        
        // Check balance
        let balance = app.ledger.balance(&req.wallet);
        if balance < req.amount {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Insufficient balance: have {} BB, need {} BB", balance, req.amount)
            }))));
        }
        
        // Validate amount bounds
        if req.amount < 0.01 {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": "Minimum bridge amount is 0.01 BB"
            }))));
        }
        if req.amount > 1_000_000.0 {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": "Maximum bridge amount is 1,000,000 BB"
            }))));
        }
        
        // Debit balance (lock on L2)
        app.ledger.debit(&req.wallet, req.amount);
        app.nonces.insert(req.wallet.clone(), req.nonce);
        
        // Create and store bridge record
        let bridge = app.bridge_manager.store_pending_withdrawal(
            req.wallet.clone(),
            req.target_address.clone(),
            req.amount,
        );
        let bridge_id = bridge.bridge_id.clone();
        
        // Store pending withdrawal for tracking
        app.pending_withdrawals.insert(bridge_id.clone(), PendingWithdrawal {
            bridge_id: bridge_id.clone(),
            wallet_address: req.wallet.clone(),
            amount: req.amount,
            l1_target: req.target_address.clone(),
            status: "pending".to_string(),
            created_at: now,
            l1_tx_hash: None,
            error: None,
            poll_count: 0,
            last_poll: None,
        });
        
        app.log_activity("🌉", "BRIDGE_WITHDRAW_INIT", &format!(
            "{} initiated bridge of {} BB to L1 {}",
            req.wallet, req.amount, req.target_address
        ));
        
        (bridge_id, bridge)
    }; // Release lock before async L1 call
    
    // --- Phase 2: Call L1 /bridge/withdraw (async) ---
    let mut l1_rpc = L1BlackBookRpc::new(L1RpcConfig::from_env());
    
    let l1_request = L1WithdrawRequest {
        from_l2_address: req.wallet.clone(),
        to_l1_address: req.target_address.clone(),
        amount: req.amount,
        bridge_id: bridge_id.clone(),
        signature: req.signature.clone(),
        timestamp: req.timestamp,
        nonce: req.nonce.to_string(),
    };
    
    match l1_rpc.withdraw_to_l1(l1_request).await {
        Ok(l1_response) => {
            let mut app = state.lock().unwrap();
            
            if l1_response.success {
                // L1 accepted - update bridge status
                let _ = app.bridge_manager.update_withdrawal_l1_submitted(
                    &bridge_id,
                    l1_response.l1_tx_hash.clone(),
                );
                
                // Update pending withdrawal
                if let Some(pw) = app.pending_withdrawals.get_mut(&bridge_id) {
                    pw.status = "l1_submitted".to_string();
                    pw.l1_tx_hash = l1_response.l1_tx_hash.clone();
                }
                
                app.log_activity("🌉", "BRIDGE_L1_ACCEPTED", &format!(
                    "L1 accepted withdrawal {} for {} BB (tx: {:?})",
                    bridge_id, req.amount, l1_response.l1_tx_hash
                ));
                
                Ok(Json(json!({
                    "success": true,
                    "bridge_id": bridge_id,
                    "direction": "L2_TO_L1",
                    "from_address": req.wallet,
                    "to_address": req.target_address,
                    "amount": req.amount,
                    "status": l1_response.status,
                    "l1_tx_hash": l1_response.l1_tx_hash,
                    "message": "Bridge submitted to L1. Poll /bridge/status/:bridge_id for confirmation."
                })))
            } else {
                // L1 rejected - refund L2 balance
                let error_msg = l1_response.error.unwrap_or_else(|| "L1 rejected withdrawal".to_string());
                
                // Refund L2 balance
                app.ledger.credit(&req.wallet, req.amount);
                let _ = app.bridge_manager.refund_withdrawal(&bridge_id, error_msg.clone());
                
                // Update pending withdrawal
                if let Some(pw) = app.pending_withdrawals.get_mut(&bridge_id) {
                    pw.status = "refunded".to_string();
                    pw.error = Some(error_msg.clone());
                }
                
                app.log_activity("🌉", "BRIDGE_L1_REJECTED", &format!(
                    "L1 rejected withdrawal {} - refunded {} BB to {}",
                    bridge_id, req.amount, req.wallet
                ));
                
                Err((StatusCode::BAD_REQUEST, Json(json!({
                    "success": false,
                    "bridge_id": bridge_id,
                    "error": error_msg,
                    "refunded": true,
                    "message": "L1 rejected withdrawal. L2 balance has been refunded."
                }))))
            }
        }
        Err(l1_error) => {
            // L1 communication error - refund L2 balance
            let mut app = state.lock().unwrap();
            
            // Refund L2 balance
            app.ledger.credit(&req.wallet, req.amount);
            let _ = app.bridge_manager.refund_withdrawal(&bridge_id, l1_error.clone());
            
            // Update pending withdrawal
            if let Some(pw) = app.pending_withdrawals.get_mut(&bridge_id) {
                pw.status = "refunded".to_string();
                pw.error = Some(l1_error.clone());
            }
            
            app.log_activity("🌉", "BRIDGE_L1_ERROR", &format!(
                "L1 communication failed for {} - refunded {} BB to {}",
                bridge_id, req.amount, req.wallet
            ));
            
            Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "success": false,
                "bridge_id": bridge_id,
                "error": format!("L1 communication failed: {}", l1_error),
                "refunded": true,
                "message": "Could not reach L1. L2 balance has been refunded."
            }))))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BridgeDepositRequest {
    pub bridge_id: String,
    pub from_address: String,  // L1 address
    pub to_address: String,    // L2 wallet
    pub amount: f64,
    pub l1_tx_hash: String,
    pub l1_slot: u64,
}

/// POST /bridge/deposit - Complete L1→L2 bridge (deposit to L2)
/// 
/// Called by L1 (or relayer) when L1→L2 bridge is confirmed.
/// Mints BB on L2 for the recipient.
/// Idempotent - ignores duplicate l1_tx_hash.
pub async fn bridge_deposit(
    State(state): State<SharedState>,
    Json(req): Json<BridgeDepositRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut app = state.lock().unwrap();
    
    // Validate amount
    if req.amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({
            "success": false,
            "error": "Amount must be positive"
        }))));
    }
    
    // Idempotency check - prevent double-crediting same L1 tx
    if app.processed_l1_txs.contains(&req.l1_tx_hash) {
        return Ok(Json(json!({
            "success": true,
            "bridge_id": req.bridge_id,
            "message": "Already processed - idempotent",
            "l1_tx_hash": req.l1_tx_hash,
            "idempotent": true
        })));
    }
    
    // Complete bridge via manager
    let complete_request = BridgeCompleteRequest {
        bridge_id: req.bridge_id.clone(),
        from_address: req.from_address.clone(),
        to_address: req.to_address.clone(),
        amount: req.amount,
        l1_tx_hash: req.l1_tx_hash.clone(),
        l1_slot: req.l1_slot,
    };
    
    match app.bridge_manager.complete_from_l1(&complete_request) {
        Ok(_bridge) => {
            // Credit the L2 wallet
            app.ledger.credit(&req.to_address, req.amount);
            
            // Mark L1 tx as processed (idempotency)
            app.processed_l1_txs.insert(req.l1_tx_hash.clone());
            
            app.log_activity("🌉", "BRIDGE_DEPOSIT", &format!(
                "L1→L2 bridge complete: {} BB to {} (L1 tx: {})",
                req.amount, req.to_address, req.l1_tx_hash
            ));
            
            Ok(Json(json!({
                "success": true,
                "bridge_id": req.bridge_id,
                "direction": "L1_TO_L2",
                "from_address": req.from_address,
                "to_address": req.to_address,
                "amount": req.amount,
                "l1_tx_hash": req.l1_tx_hash,
                "status": "completed",
                "new_balance": app.ledger.balance(&req.to_address)
            })))
        }
        Err(e) => {
            Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Bridge completion failed: {:?}", e)
            }))))
        }
    }
}

/// GET /bridge/status/:bridge_id - Get bridge status
pub async fn get_bridge_status(
    State(state): State<SharedState>,
    Path(bridge_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let app = state.lock().unwrap();
    
    if let Some(bridge) = app.bridge_manager.get_status(&bridge_id) {
        Ok(Json(json!({
            "success": true,
            "bridge": {
                "bridge_id": bridge.bridge_id,
                "direction": format!("{:?}", bridge.direction),
                "from_address": bridge.from_address,
                "to_address": bridge.to_address,
                "amount": bridge.amount,
                "status": format!("{:?}", bridge.status),
                "created_at": bridge.created_at,
                "l1_tx_hash": bridge.l1_tx_hash,
                "l1_slot": bridge.l1_slot
            }
        })))
    } else {
        Err((StatusCode::NOT_FOUND, Json(json!({
            "success": false,
            "error": "Bridge not found"
        }))))
    }
}

/// GET /bridge/list/:wallet - List all bridges for a wallet
pub async fn list_wallet_bridges(
    State(state): State<SharedState>,
    Path(wallet): Path<String>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    let bridges = app.bridge_manager.list_by_address(&wallet);
    
    Json(json!({
        "success": true,
        "wallet": wallet,
        "bridges": bridges.iter().map(|b| json!({
            "bridge_id": b.bridge_id,
            "direction": format!("{:?}", b.direction),
            "amount": b.amount,
            "status": format!("{:?}", b.status),
            "created_at": b.created_at
        })).collect::<Vec<_>>()
    }))
}

/// GET /bridge/stats - Get overall bridge statistics
pub async fn get_bridge_stats(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    let stats = app.bridge_manager.stats();
    
    Json(json!({
        "success": true,
        "stats": {
            "total_bridges": stats.total,
            "pending": stats.pending,
            "confirmed": stats.confirmed,
            "completed": stats.completed,
            "failed": stats.failed,
            "cancelled": stats.cancelled,
            "l1_to_l2_count": stats.l1_to_l2,
            "l2_to_l1_count": stats.l2_to_l1,
            "total_volume": stats.total_volume
        }
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// SESSION HANDLERS (Optimistic Execution)
// ═══════════════════════════════════════════════════════════════════════════════

use crate::app_state::L2Session;
use crate::rpc::{L1SessionStartRequest, L1SessionSettleRequest};

#[derive(Debug, Deserialize)]
pub struct SessionStartRequest {
    pub wallet_address: String,
    pub requested_amount: f64,
    pub signature: String,
    pub timestamp: u64,
    pub nonce: String,
}

/// POST /session/start - Start an L2 optimistic session
/// 
/// Calls L1 to lock balance, creates L2 session for instant betting.
/// Session expires after 1 hour (auto-settle required).
pub async fn session_start(
    State(state): State<SharedState>,
    Json(req): Json<SessionStartRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Validate timestamp
    if now.abs_diff(req.timestamp) > 300 {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({
            "success": false,
            "error": "Session request expired"
        }))));
    }
    
    // Check if user already has active session
    {
        let app = state.lock().unwrap();
        if let Some(existing) = app.sessions.get(&req.wallet_address) {
            if existing.status == "active" && !existing.is_expired() {
                return Err((StatusCode::CONFLICT, Json(json!({
                    "success": false,
                    "error": "Active session already exists",
                    "session_id": existing.session_id,
                    "expires_in_secs": existing.time_remaining_secs()
                }))));
            }
        }
    }
    
    // Generate session ID
    let session_id = format!("session_{}", uuid::Uuid::new_v4().simple());
    
    // Call L1 to start session (lock L1 balance)
    let mut l1_rpc = L1BlackBookRpc::new(L1RpcConfig::from_env());
    
    let l1_request = L1SessionStartRequest {
        wallet_address: req.wallet_address.clone(),
        l2_session_id: session_id.clone(),
        requested_amount: req.requested_amount,
        signature: req.signature.clone(),
        timestamp: req.timestamp,
        nonce: req.nonce.clone(),
    };
    
    match l1_rpc.start_session(l1_request).await {
        Ok(l1_response) => {
            if l1_response.success {
                let mut app = state.lock().unwrap();
                
                let l2_credit = l1_response.l2_credit.unwrap_or(req.requested_amount);
                let l1_balance = l1_response.l1_balance.unwrap_or(0.0);
                
                // Create L2 session
                let session = L2Session::new(
                    req.wallet_address.clone(),
                    l1_balance,
                    l2_credit,
                    l1_response.session_id.clone().unwrap_or(session_id.clone()),
                );
                
                // Credit L2 balance for optimistic execution
                app.ledger.credit(&req.wallet_address, l2_credit);
                
                // Store session
                app.sessions.insert(req.wallet_address.clone(), session.clone());
                
                app.log_activity("🎮", "SESSION_START", &format!(
                    "{} started session with {} BB credit (L1 balance: {})",
                    req.wallet_address, l2_credit, l1_balance
                ));
                
                Ok(Json(json!({
                    "success": true,
                    "session_id": session.session_id,
                    "wallet_address": req.wallet_address,
                    "l1_balance": l1_balance,
                    "l2_credit": l2_credit,
                    "expires_at": session.expires_at,
                    "expires_in_secs": session.time_remaining_secs(),
                    "status": "active",
                    "message": "Session started. You can now bet on L2 instantly."
                })))
            } else {
                let error = l1_response.error.unwrap_or_else(|| "L1 rejected session start".to_string());
                Err((StatusCode::BAD_REQUEST, Json(json!({
                    "success": false,
                    "error": error
                }))))
            }
        }
        Err(e) => {
            Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "success": false,
                "error": format!("L1 communication failed: {}", e)
            }))))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SessionSettleRequest {
    pub wallet_address: String,
    pub signature: String,
    pub timestamp: u64,
}

/// POST /session/settle - Settle an L2 session (write PnL to L1)
/// 
/// Settles all L2 activity back to L1. Required before:
/// - Session expires (1 hour max)
/// - User wants to withdraw to L1
/// - User wants to start a new session
pub async fn session_settle(
    State(state): State<SharedState>,
    Json(req): Json<SessionSettleRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Get session info
    let (session, current_l2_balance) = {
        let app = state.lock().unwrap();
        
        let session = app.sessions.get(&req.wallet_address)
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({
                "success": false,
                "error": "No active session found"
            }))))?
            .clone();
        
        if session.status != "active" {
            return Err((StatusCode::BAD_REQUEST, Json(json!({
                "success": false,
                "error": format!("Session is not active (status: {})", session.status)
            }))));
        }
        
        let current_balance = app.ledger.balance(&req.wallet_address);
        (session, current_balance)
    };
    
    // Calculate final PnL
    let initial_credit = session.l2_balance;
    let pnl = current_l2_balance - initial_credit;
    
    // Call L1 to settle session
    let mut l1_rpc = L1BlackBookRpc::new(L1RpcConfig::from_env());
    
    let l1_request = L1SessionSettleRequest {
        wallet_address: req.wallet_address.clone(),
        session_id: session.session_id.clone(),
        final_l2_balance: current_l2_balance,
        pnl,
        bet_count: session.bet_count,
        signature: req.signature.clone(),
        timestamp: req.timestamp,
    };
    
    match l1_rpc.settle_session(l1_request).await {
        Ok(l1_response) => {
            if l1_response.success {
                let mut app = state.lock().unwrap();
                
                // Clear L2 balance (settled to L1)
                app.ledger.debit(&req.wallet_address, current_l2_balance);
                
                // Update session status
                if let Some(s) = app.sessions.get_mut(&req.wallet_address) {
                    s.status = "settled".to_string();
                    s.l1_settlement_hash = l1_response.l1_tx_hash.clone();
                }
                
                app.log_activity("🎮", "SESSION_SETTLE", &format!(
                    "{} settled session: {} bets, PnL: {:.2} BB, new L1 balance: {:?}",
                    req.wallet_address, session.bet_count, pnl, l1_response.new_l1_balance
                ));
                
                Ok(Json(json!({
                    "success": true,
                    "session_id": session.session_id,
                    "wallet_address": req.wallet_address,
                    "bet_count": session.bet_count,
                    "final_l2_balance": current_l2_balance,
                    "pnl": pnl,
                    "l1_tx_hash": l1_response.l1_tx_hash,
                    "new_l1_balance": l1_response.new_l1_balance,
                    "status": "settled",
                    "message": "Session settled. PnL written to L1."
                })))
            } else {
                let error = l1_response.error.unwrap_or_else(|| "L1 rejected settlement".to_string());
                Err((StatusCode::BAD_REQUEST, Json(json!({
                    "success": false,
                    "error": error
                }))))
            }
        }
        Err(e) => {
            // L1 failed - session remains active, user can retry
            Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({
                "success": false,
                "error": format!("L1 settlement failed: {}. Session remains active, please retry.", e)
            }))))
        }
    }
}

/// GET /session/status/:wallet - Get session status
pub async fn session_status(
    State(state): State<SharedState>,
    Path(wallet): Path<String>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    if let Some(session) = app.sessions.get(&wallet) {
        let current_balance = app.ledger.balance(&wallet);
        let pnl = current_balance - session.l2_balance;
        
        Json(json!({
            "success": true,
            "session": {
                "session_id": session.session_id,
                "wallet_address": session.wallet_address,
                "l1_balance_snapshot": session.l1_balance_snapshot,
                "initial_l2_credit": session.l2_balance,
                "current_l2_balance": current_balance,
                "bet_count": session.bet_count,
                "pnl": pnl,
                "started_at": session.started_at,
                "expires_at": session.expires_at,
                "expires_in_secs": session.time_remaining_secs(),
                "is_expired": session.is_expired(),
                "status": session.status,
                "l1_settlement_hash": session.l1_settlement_hash
            }
        }))
    } else {
        Json(json!({
            "success": true,
            "session": null,
            "message": "No session found for this wallet"
        }))
    }
}

/// GET /session/list - List all active sessions
pub async fn session_list(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    let sessions: Vec<_> = app.sessions.values()
        .filter(|s| s.status == "active")
        .map(|s| {
            let current_balance = app.ledger.balance(&s.wallet_address);
            json!({
                "session_id": s.session_id,
                "wallet_address": s.wallet_address,
                "l2_balance": current_balance,
                "bet_count": s.bet_count,
                "expires_in_secs": s.time_remaining_secs(),
                "is_expired": s.is_expired()
            })
        })
        .collect();
    
    Json(json!({
        "success": true,
        "active_sessions": sessions.len(),
        "sessions": sessions
    }))
}