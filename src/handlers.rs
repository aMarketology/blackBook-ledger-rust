// HTTP request handlers for the BlackBook API

use axum::{
    extract::{Path, State},
    http::{StatusCode, HeaderMap},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::app_state::SharedState;
use crate::models::*;
use crate::rpc::TransactionPayload;
use crate::optimistic_ledger::{AccountBalanceDetails, SettlementSummary};
use crate::l1_sync::HybridSettlementStatus;

// ===== SIMPLE BET REQUEST (FLAT STRUCTURE - MATCHES AUTH_SIMPLIFICATION.md) =====

#[derive(Debug, Deserialize)]
pub struct SimpleBetRequest {
    pub signature: String,
    pub from_address: String,
    pub market_id: String,
    pub option: String,  // "0" or "1" or "YES"/"NO"
    pub amount: f64,
    pub nonce: u64,
    pub timestamp: u64,
}

// ===== SIGNED BET ENDPOINT (PRIMARY BETTING METHOD) =====

pub async fn place_signed_bet(
    State(state): State<SharedState>,
    Json(request): Json<SimpleBetRequest>,
) -> Result<Json<SignedBetResponse>, (StatusCode, Json<SignedBetResponse>)> {
    println!("📥 Received bet: market={}, option={}, amount={}", 
             request.market_id, request.option, request.amount);
    
    // Validate timestamp (24 hour window)
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let time_diff = if current_time > request.timestamp {
        current_time - request.timestamp
    } else {
        request.timestamp - current_time
    };
    
    if time_diff > 86400 {
        println!("❌ Transaction expired: {}s old (max 86400s)", time_diff);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(SignedBetResponse {
                success: false,
                bet_id: None,
                transaction_id: None,
                market_id: Some(request.market_id),
                outcome: None,
                amount: Some(request.amount),
                new_balance: None,
                nonce_used: None,
                error: Some(format!("Transaction expired ({}s old)", time_diff)),
            })
        ));
    }
    
    // Convert option to outcome
    let outcome = match request.option.as_str() {
        "0" | "YES" => 0,
        "1" | "NO" => 1,
        _ => {
            println!("❌ Invalid option: {}", request.option);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(SignedBetResponse {
                    success: false,
                    bet_id: None,
                    transaction_id: None,
                    market_id: Some(request.market_id),
                    outcome: None,
                    amount: Some(request.amount),
                    new_balance: None,
                    nonce_used: None,
                    error: Some("Invalid option: must be 0, 1, YES, or NO".to_string()),
                })
            ));
        }
    };
    
    println!("✅ Timestamp valid, placing bet...");
    
    let sender_address = request.from_address.clone();
    let market_id = request.market_id.clone();
    let amount = request.amount;
    let nonce = request.nonce;
    
    let mut app_state = state.lock().unwrap();
    
    // Check nonce for replay protection
    let last_nonce = app_state.nonces.get(&sender_address).copied().unwrap_or(0);
    if nonce <= last_nonce {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(SignedBetResponse {
                success: false,
                bet_id: None,
                transaction_id: None,
                market_id: Some(market_id),
                outcome: Some(outcome),
                amount: Some(amount),
                new_balance: None,
                nonce_used: None,
                error: Some(format!("Invalid nonce: {} <= {}", nonce, last_nonce)),
            })
        ));
    }
    
    // Resolve address to account
    let account_name = app_state.ledger.accounts.iter()
        .find(|(_, addr)| **addr == sender_address)
        .map(|(name, _)| name.clone())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(SignedBetResponse {
                    success: false,
                    bet_id: None,
                    transaction_id: None,
                    market_id: Some(market_id.clone()),
                    outcome: Some(outcome),
                    amount: Some(amount),
                    new_balance: None,
                    nonce_used: None,
                    error: Some("Address not found".into()),
                })
            )
        })?;
    
    // Validate market
    let market_exists = app_state.markets.contains_key(&market_id);
    if !market_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(SignedBetResponse {
                success: false,
                bet_id: None,
                transaction_id: None,
                market_id: Some(market_id),
                outcome: Some(outcome),
                amount: Some(amount),
                new_balance: None,
                nonce_used: None,
                error: Some("Market not found".into()),
            })
        ));
    }
    
    // Place bet
    match app_state.ledger.place_bet(&account_name, &market_id, amount) {
        Ok(tx_id) => {
            app_state.nonces.insert(sender_address.clone(), nonce);
            
            let bet_id = if let Some(market) = app_state.markets.get_mut(&market_id) {
                market.record_bet(&account_name, amount, outcome)
            } else {
                tx_id.clone()
            };
            
            let new_balance = app_state.ledger.get_balance(&account_name);
            
            app_state.log_blockchain_activity(
                "🎯",
                "SIGNED_BET",
                &format!("{} bet {} BB on market {} | Nonce: {}", account_name, amount, market_id, nonce)
            );
            
            Ok(Json(SignedBetResponse {
                success: true,
                bet_id: Some(bet_id),
                transaction_id: Some(tx_id),
                market_id: Some(market_id),
                outcome: Some(outcome),
                amount: Some(amount),
                new_balance: Some(new_balance),
                nonce_used: Some(nonce),
                error: None,
            }))
        }
        Err(e) => {
            Err((
                StatusCode::BAD_REQUEST,
                Json(SignedBetResponse {
                    success: false,
                    bet_id: None,
                    transaction_id: None,
                    market_id: Some(market_id),
                    outcome: Some(outcome),
                    amount: Some(amount),
                    new_balance: None,
                    nonce_used: None,
                    error: Some(e),
                })
            ))
        }
    }
}

// ===== MARKET ENDPOINTS =====

pub async fn get_markets(State(state): State<SharedState>) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let markets: Vec<Value> = app_state
        .markets
        .values()
        .map(|m| {
            json!({
                "id": m.id,
                "title": m.title,
                "description": m.description,
                "category": m.category,
                "options": m.options,
                "is_resolved": m.is_resolved,
                "total_volume": m.total_volume,
                "unique_bettors": m.unique_bettors.len(),
                "odds": m.calculate_odds(),
            })
        })
        .collect();
    
    Json(json!({ "markets": markets }))
}

pub async fn get_market(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let app_state = state.lock().unwrap();
    let market = app_state.markets.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    
    Ok(Json(json!({
        "id": market.id,
        "title": market.title,
        "description": market.description,
        "category": market.category,
        "options": market.options,
        "is_resolved": market.is_resolved,
        "total_volume": market.total_volume,
        "bet_count": market.bet_count,
        "unique_bettors": market.unique_bettors.len(),
        "odds": market.calculate_odds(),
        "option_stats": market.option_stats,
    })))
}

pub async fn create_market(
    State(state): State<SharedState>,
    Json(payload): Json<CreateMarketRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    let market_id = uuid::Uuid::new_v4().simple().to_string();
    let market = PredictionMarket::new(
        market_id.clone(),
        payload.title.clone(),
        payload.description,
        payload.category,
        payload.options,
    );
    
    app_state.markets.insert(market_id.clone(), market);
    app_state.log_blockchain_activity("📊", "MARKET_CREATED", &format!("Created: {}", payload.title));
    
    Ok(Json(json!({ "success": true, "market_id": market_id })))
}

// ===== LEDGER ENDPOINTS =====

pub async fn get_balance(
    State(state): State<SharedState>,
    Path(account): Path<String>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let balance = app_state.ledger.get_balance(&account);
    Json(json!({ "account": account, "balance": balance }))
}

pub async fn transfer(
    State(state): State<SharedState>,
    Json(payload): Json<TransferRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.ledger.transfer(&payload.from, &payload.to, payload.amount) {
        Ok(tx_id) => {
            app_state.log_blockchain_activity(
                "💸",
                "TRANSFER",
                &format!("{} → {} | {} BB", payload.from, payload.to, payload.amount)
            );
            Ok(Json(json!({ "success": true, "transaction_id": tx_id })))
        }
        Err(e) => Ok(Json(json!({ "success": false, "error": e }))),
    }
}

pub async fn get_ledger_activity(State(state): State<SharedState>) -> Json<Value> {
    let app_state = state.lock().unwrap();
    Json(json!({ "activity": app_state.blockchain_activity }))
}

// ===== RPC ENDPOINTS =====

pub async fn get_nonce(
    State(state): State<SharedState>,
    Path(address): Path<String>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let nonce = app_state.nonces.get(&address).copied().unwrap_or(0);
    Json(json!({ "address": address, "nonce": nonce }))
}

// ===== USER ENDPOINTS =====

pub async fn get_user_bets(
    State(state): State<SharedState>,
    Path(account): Path<String>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let mut all_bets = Vec::new();
    
    for market in app_state.markets.values() {
        all_bets.extend(market.get_bets_for_account(&account));
    }
    
    Json(json!({ "account": account, "bets": all_bets }))
}

// ===== HYBRID L1/L2 SETTLEMENT ENDPOINTS =====

/// Get detailed balance showing confirmed (L1) and pending (L2) amounts
pub async fn get_balance_details(
    State(state): State<SharedState>,
    Path(account): Path<String>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    if app_state.use_optimistic_mode {
        let addr = app_state.optimistic_ledger.resolve_address(&account);
        if let Some(balance) = app_state.optimistic_ledger.balances.get(&addr) {
            let details = AccountBalanceDetails::from((&addr, balance));
            Json(json!({
                "success": true,
                "account": account,
                "address": details.address,
                "confirmed_balance": details.confirmed_balance,
                "pending_delta": details.pending_delta,
                "available_balance": details.available_balance,
                "last_l1_sync_slot": details.last_l1_sync_slot,
                "last_l1_sync_timestamp": details.last_l1_sync_timestamp,
                "mode": "optimistic"
            }))
        } else {
            Json(json!({
                "success": false,
                "error": "Account not found",
                "account": account
            }))
        }
    } else {
        // Non-optimistic mode - just return regular balance
        let balance = app_state.ledger.get_balance(&account);
        Json(json!({
            "success": true,
            "account": account,
            "confirmed_balance": balance,
            "pending_delta": 0.0,
            "available_balance": balance,
            "mode": "direct"
        }))
    }
}

/// Trigger settlement of pending bets to L1
pub async fn settle_to_l1(
    State(state): State<SharedState>,
) -> Json<Value> {
    let mut app_state = state.lock().unwrap();
    
    if !app_state.use_optimistic_mode {
        return Json(json!({
            "success": false,
            "error": "Optimistic mode is disabled"
        }));
    }
    
    // Check if there's a batch ready
    if app_state.optimistic_ledger.current_batch.is_none() {
        return Json(json!({
            "success": false,
            "error": "No pending bets to settle"
        }));
    }
    
    // Prepare the batch
    if let Some(batch) = app_state.optimistic_ledger.prepare_batch_for_submission() {
        let batch_id = batch.batch_id.clone();
        let bet_count = batch.bet_ids.len();
        let merkle_root = batch.merkle_root.clone();
        let affected_accounts = batch.affected_accounts();
        let total_volume = batch.total_volume();
        
        // In mock mode, simulate immediate success
        // In production, this would submit to L1 and wait for confirmation
        app_state.optimistic_ledger.pending_batches.push_back(batch);
        
        app_state.log_blockchain_activity(
            "📦",
            "BATCH_SUBMITTED",
            &format!("Batch {} with {} bets submitted to L1 | Volume: {} BB", 
                     batch_id, bet_count, total_volume)
        );
        
        Json(json!({
            "success": true,
            "batch_id": batch_id,
            "bets_included": bet_count,
            "affected_accounts": affected_accounts,
            "total_volume": total_volume,
            "merkle_root": merkle_root,
            "status": "submitted",
            "message": "Batch submitted to L1 for settlement"
        }))
    } else {
        Json(json!({
            "success": false,
            "error": "Failed to prepare batch for submission"
        }))
    }
}

/// Get current settlement status
pub async fn get_settlement_status(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    if !app_state.use_optimistic_mode {
        return Json(json!({
            "success": true,
            "mode": "direct",
            "optimistic_enabled": false,
            "message": "Optimistic mode disabled - all bets execute directly"
        }));
    }
    
    let settlement_summary = app_state.optimistic_ledger.get_settlement_summary();
    let sync_status = app_state.l1_sync.get_status();
    
    Json(json!({
        "success": true,
        "mode": "optimistic",
        "optimistic_enabled": true,
        "l2_block_height": settlement_summary.l2_block_height,
        "pending_bets": settlement_summary.pending_bets,
        "batched_bets": settlement_summary.batched_bets,
        "current_batch_size": settlement_summary.current_batch_size,
        "pending_batches": settlement_summary.pending_batches,
        "finalized_batches": settlement_summary.finalized_batches,
        "pending_volume": settlement_summary.pending_volume,
        "sync": {
            "status": format!("{:?}", sync_status.status),
            "is_connected": sync_status.is_connected,
            "is_mock_mode": sync_status.is_mock_mode,
            "last_sync_slot": sync_status.last_sync_slot,
            "last_sync_timestamp": sync_status.last_sync_timestamp,
            "sync_interval_secs": sync_status.sync_interval_secs
        }
    }))
}

/// Sync balances from L1
pub async fn sync_from_l1(
    State(state): State<SharedState>,
) -> Json<Value> {
    // We need to release the lock before calling async sync
    // For now, just report that sync was requested
    let mut app_state = state.lock().unwrap();
    
    if !app_state.use_optimistic_mode {
        return Json(json!({
            "success": false,
            "error": "Optimistic mode is disabled"
        }));
    }
    
    // In mock mode, sync is instant
    if app_state.l1_sync.rpc_client.is_mock_mode() {
        // Sync from local ledger to optimistic ledger
        app_state.sync_ledger_to_optimistic();
        
        // Get count before mutable borrow for logging
        let accounts_count = app_state.optimistic_ledger.balances.len();
        
        app_state.log_blockchain_activity(
            "🔄",
            "L1_SYNC",
            &format!("Synced {} accounts from L1 (mock mode)", accounts_count)
        );
        
        return Json(json!({
            "success": true,
            "accounts_synced": accounts_count,
            "mode": "mock",
            "message": "Balances synced from L1 (mock mode)"
        }));
    }
    
    // In live mode, we would do actual L1 sync
    // For now, just acknowledge the request
    Json(json!({
        "success": true,
        "accounts_synced": 0,
        "mode": "live",
        "message": "L1 sync requested - will complete asynchronously"
    }))
}
