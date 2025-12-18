// Authentication routes for BlackBook Layer 2
// Simplified: No JWT - authentication via cryptographic signatures on transactions

use axum::{
    extract::{State, Path},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::app_state::SharedState;

// L1 endpoint for balance queries
const L1_ENDPOINT: &str = "http://localhost:8080";

// ===== REQUEST/RESPONSE TYPES =====

#[derive(Debug, Deserialize)]
pub struct ConnectWalletRequest {
    /// Wallet address (L1_ABC123...) - supports both 'wallet_address' and 'address' fields
    #[serde(alias = "address")]
    pub wallet_address: Option<String>,
    /// Public key for Ed25519 verification
    pub public_key: Option<String>,
    /// Connection timestamp
    pub timestamp: Option<u64>,
    /// Optional username for display
    pub username: Option<String>,
}

/// Response from L1 balance endpoint
#[derive(Debug, Deserialize)]
struct L1BalanceResponse {
    address: String,
    balance: f64,
    success: bool,
}

/// Fetch balance from L1 for a wallet address
async fn fetch_l1_balance(address: &str) -> Option<f64> {
    let url = format!("{}/balance/{}", L1_ENDPOINT, address);
    
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            if let Ok(data) = response.json::<L1BalanceResponse>().await {
                if data.success && data.balance > 0.0 {
                    println!("📡 L1 balance for {}: {} BB", address, data.balance);
                    return Some(data.balance);
                }
            }
            None
        }
        Err(e) => {
            println!("⚠️ Failed to fetch L1 balance: {}", e);
            None
        }
    }
}

// ===== ROUTE HANDLERS =====

/// POST /auth/connect
/// Simple wallet connection - no JWT, just wallet address
/// Creates account if new, returns balance if existing
/// Now fetches real L1 balance for new accounts!
/// Accepts: { address, public_key, timestamp } or { wallet_address, username }
pub async fn connect_wallet(
    State(state): State<SharedState>,
    Json(payload): Json<ConnectWalletRequest>,
) -> Json<Value> {
    // Support both 'wallet_address' and 'address' fields, fallback to public_key
    let wallet_address = payload.wallet_address
        .clone()
        .or_else(|| payload.public_key.clone())
        .unwrap_or_default();
    
    if wallet_address.is_empty() {
        return Json(json!({
            "success": false,
            "error": "No wallet address provided. Use 'address', 'wallet_address', or 'public_key' field."
        }));
    }
    
    println!("💳 Wallet connect: {}", wallet_address);
    
    // Check L1 balance BEFORE acquiring lock (async operation)
    let l1_balance = fetch_l1_balance(&wallet_address).await;

    let mut app_state = state.lock().unwrap();
    
    // Check if wallet exists in ledger
    let balance_exists = app_state.ledger.balance(&wallet_address) > 0.0 
        || app_state.ledger.accounts.values().any(|addr| addr == &wallet_address);
    
    if !balance_exists {
        println!("🆕 New wallet detected: {}", wallet_address);
        
        // Add wallet to ledger accounts (UPPERCASE for consistency with resolve_address)
        let username = payload.username.clone()
            .unwrap_or_else(|| {
                if wallet_address.len() > 11 {
                    format!("user_{}", &wallet_address[3..11])
                } else {
                    format!("user_{}", &wallet_address)
                }
            })
            .to_uppercase();
        
        // Use L1 balance if available, otherwise default to 30,000 BB for development
        let initial_balance = l1_balance.unwrap_or(30_000.0);
        let balance_source = if l1_balance.is_some() { "L1" } else { "default" };
        
        app_state.ledger.register(&username, &wallet_address, initial_balance);
        
        println!("✅ Funded {} with {} BB (source: {})", wallet_address, initial_balance, balance_source);
        
        app_state.log_activity(
            "🆕",
            "NEW_WALLET",
            &format!("New wallet {} connected | Funded with {} BB ({})", 
                wallet_address, initial_balance, balance_source)
        );
        
        Json(json!({
            "success": true,
            "wallet_address": wallet_address,
            "username": username,
            "balance": initial_balance,
            "l1_balance": l1_balance,
            "balance_source": balance_source,
            "is_new_account": true,
            "message": format!("Account created and funded with {} BB from {}", initial_balance, balance_source)
        }))
    } else {
        // Existing account - return balance
        let balance = app_state.ledger.balance(&wallet_address);
        
        println!("✅ Existing wallet: {} (balance: {} BB)", wallet_address, balance);
        
        app_state.log_activity(
            "🔐",
            "WALLET_CONNECT",
            &format!("Wallet {} reconnected | Balance: {} BB", 
                wallet_address, balance)
        );
        
        Json(json!({
            "success": true,
            "wallet_address": wallet_address,
            "username": payload.username,
            "balance": balance,
            "l1_balance": l1_balance,
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
    let balance = app_state.ledger.balance(&wallet);
    
    Json(json!({
        "wallet_address": wallet,
        "balance": balance
    }))
}
