// HTTP request handlers - Simplified

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::app_state::SharedState;
use crate::models::*;

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
    
    // Check nonce
    let last_nonce = app.nonces.get(&req.from_address).copied().unwrap_or(0);
    if req.nonce <= last_nonce {
        return Err((StatusCode::BAD_REQUEST, Json(SignedBetResponse::error("Invalid nonce"))));
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
        "options": m.options,
        "is_resolved": m.is_resolved,
        "total_volume": m.total_volume,
        "odds": m.calculate_odds(),
        "option_stats": m.option_stats,
    })))
}

pub async fn create_market(
    State(state): State<SharedState>,
    Json(payload): Json<CreateMarketRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app = state.lock().unwrap();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let market = PredictionMarket::new(id.clone(), payload.title.clone(), payload.description, payload.category, payload.options);
    app.markets.insert(id.clone(), market);
    app.log_activity("📊", "MARKET", &format!("Created: {}", payload.title));
    Ok(Json(json!({ "success": true, "market_id": id })))
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

// ===== RPC ENDPOINTS =====

pub async fn get_nonce(State(state): State<SharedState>, Path(address): Path<String>) -> Json<Value> {
    let app = state.lock().unwrap();
    let nonce = app.nonces.get(&address).copied().unwrap_or(0);
    Json(json!({ "address": address, "nonce": nonce }))
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
