// Authentication routes for BlackBook Layer 2
// Simplified: No JWT - authentication via cryptographic signatures on transactions

use axum::{
    extract::{State, Path},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::app_state::SharedState;

// ===== REQUEST/RESPONSE TYPES =====

#[derive(Debug, Deserialize)]
pub struct ConnectWalletRequest {
    /// Wallet address (L1_ABC123...)
    pub wallet_address: String,
    /// Optional username for display
    pub username: Option<String>,
}

// ===== ROUTE HANDLERS =====

/// POST /auth/connect
/// Simple wallet connection - no JWT, just wallet address
/// Creates account if new, returns balance if existing
pub async fn connect_wallet(
    State(state): State<SharedState>,
    Json(payload): Json<ConnectWalletRequest>,
) -> Json<Value> {
    println!("💳 Wallet connect: {}", payload.wallet_address);

    let mut app_state = state.lock().unwrap();
    
    // Check if wallet exists in ledger
    let balance_exists = app_state.ledger.get_balance(&payload.wallet_address) > 0.0 
        || app_state.ledger.accounts.values().any(|addr| addr == &payload.wallet_address);
    
    if !balance_exists {
        println!("🆕 New wallet detected: {}", payload.wallet_address);
        
        // Add wallet to ledger accounts (UPPERCASE for consistency with resolve_address)
        let username = payload.username.clone()
            .unwrap_or_else(|| format!("user_{}", &payload.wallet_address[3..11]))
            .to_uppercase();
        
        app_state.ledger.accounts.insert(username.clone(), payload.wallet_address.clone());
        
        // Fund new account with initial balance (30,000 BB - matches L1 for development)
        let initial_balance = 30_000.0;
        match app_state.ledger.admin_mint_tokens(&username, initial_balance) {
            Ok(_) => {
                println!("✅ Funded {} with {} BB", payload.wallet_address, initial_balance);
                
                app_state.log_blockchain_activity(
                    "🆕",
                    "NEW_WALLET",
                    &format!("New wallet {} connected | Funded with {} BB", 
                        payload.wallet_address, initial_balance)
                );
                
                Json(json!({
                    "success": true,
                    "wallet_address": payload.wallet_address,
                    "username": username,
                    "balance": initial_balance,
                    "is_new_account": true,
                    "message": format!("Account created and funded with {} BB", initial_balance)
                }))
            }
            Err(e) => {
                Json(json!({
                    "success": false,
                    "error": format!("Failed to fund wallet: {}", e)
                }))
            }
        }
    } else {
        // Existing account - return balance
        let balance = app_state.ledger.get_balance(&payload.wallet_address);
        
        println!("✅ Existing wallet: {} (balance: {} BB)", payload.wallet_address, balance);
        
        app_state.log_blockchain_activity(
            "🔐",
            "WALLET_CONNECT",
            &format!("Wallet {} reconnected | Balance: {} BB", 
                payload.wallet_address, balance)
        );
        
        Json(json!({
            "success": true,
            "wallet_address": payload.wallet_address,
            "username": payload.username,
            "balance": balance,
            "is_new_account": false
        }))
    }
}

/// GET /balance/:wallet
/// Get balance for any wallet address
pub async fn get_wallet_balance(
    State(state): State<SharedState>,
    Path(wallet): Path<String>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let balance = app_state.ledger.get_balance(&wallet);
    
    Json(json!({
        "wallet_address": wallet,
        "balance": balance
    }))
}
