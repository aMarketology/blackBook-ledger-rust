// HTTP request handlers - Simplified

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::app_state::SharedState;
use crate::models::*;
use crate::market_resolve::cpmm::{CPMMPool, VIABILITY_THRESHOLD};
use crate::rss::{RssEvent, EventDates, write_rss_event_to_file, ResolutionRules as RssResolutionRules};
use crate::ledger::{TxType, Transaction};
use std::collections::HashMap;

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
                "HOUSE",
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
    /// Funder's L1 address (for user-funded) - if omitted, house mints
    pub funder: Option<String>,
    /// If true, admin mints tokens (house-funded)
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
///   { "amount": 10000, "house_funded": true }   // House-funded (admin mint)
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
        // House-funded: Admin mints tokens
        let result = mint_liquidity_on_l1(&escrow_address, amount).await;
        (result, "house_mint", "HOUSE".to_string())
    } else if let Some(ref funder) = payload.funder {
        // User-funded: Transfer from user's L1 balance to escrow
        let result = transfer_l1_to_escrow(funder, &escrow_address, amount).await;
        (result, "user_funded", funder.clone())
    } else {
        // Default to house-funded if no funder specified
        let result = mint_liquidity_on_l1(&escrow_address, amount).await;
        (result, "house_mint", "HOUSE".to_string())
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
}

/// Public ledger transactions endpoint with filtering, sorting, and pagination
pub async fn get_ledger_transactions(
    State(state): State<SharedState>,
    Query(params): Query<LedgerQuery>,
) -> Json<Value> {
    let app = state.lock().unwrap();
    
    // Get all transactions
    let mut txs: Vec<_> = app.ledger.transactions.iter().collect();
    
    // Filter by type
    if let Some(ref type_filter) = params.tx_type {
        let tx_type = match type_filter.to_lowercase().as_str() {
            "bet" => Some(TxType::Bet),
            "transfer" => Some(TxType::Transfer),
            "deposit" => Some(TxType::Deposit),
            "withdraw" => Some(TxType::Withdraw),
            "payout" => Some(TxType::Payout),
            "accountcreated" | "account_created" => Some(TxType::AccountCreated),
            _ => None,
        };
        if let Some(t) = tx_type {
            txs.retain(|tx| tx.tx_type == t);
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
    
    // Get ledger stats
    let stats = app.ledger.stats();
    
    // Format transactions for response
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
            "signature": if tx.signature.is_empty() { None } else { Some(&tx.signature[..16.min(tx.signature.len())]) }
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
            "total_accounts": stats.accounts,
            "total_transactions": stats.transactions,
            "current_block": stats.block,
            "total_bets": stats.total_bets,
            "bet_volume": stats.bet_volume
        }
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
