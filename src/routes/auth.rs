// Authentication routes for BlackBook Layer 2
// POST /auth/login - Login with Supabase JWT and get L1 wallet info

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::app_state::SharedState;
use crate::auth::User;

// ===== REQUEST/RESPONSE TYPES =====

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Supabase JWT token from frontend
    pub token: String,
    /// Optional username for new users
    pub username: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub wallet_address: Option<String>,
    pub balance: Option<f64>,
    pub is_new_user: Option<bool>,
    pub error: Option<String>,
}

// ===== ROUTE HANDLERS =====

/// POST /auth/login
/// 
/// Login with a Supabase JWT token. If the user doesn't exist on L2,
/// a new wallet is created and funded with initial tokens.
///
/// Request:
/// ```json
/// {
///   "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
///   "username": "optional_display_name"
/// }
/// ```
///
/// Response:
/// ```json
/// {
///   "success": true,
///   "user_id": "supabase-user-id",
///   "username": "display_name",
///   "wallet_address": "L1_ABC123...",
///   "balance": 100.0,
///   "is_new_user": false
/// }
/// ```
pub async fn login(
    State(state): State<SharedState>,
    Json(payload): Json<LoginRequest>,
) -> Json<LoginResponse> {
    // Get Supabase config from state
    let supabase_config = {
        let app_state = state.lock().unwrap();
        app_state.supabase_config.clone()
    };

    // Verify the JWT token with Supabase
    let user_id = match supabase_config.verify_token(&payload.token).await {
        Ok(id) => id,
        Err(e) => {
            return Json(LoginResponse {
                success: false,
                user_id: None,
                username: None,
                wallet_address: None,
                balance: None,
                is_new_user: None,
                error: Some(format!("Authentication failed: {}", e)),
            });
        }
    };

    let mut app_state = state.lock().unwrap();

    // Check if user already exists on L2
    if let Some(existing_user) = app_state.supabase_users.get(&user_id).cloned() {
        let balance = app_state.ledger.get_balance(&existing_user.wallet_address);
        
        app_state.log_blockchain_activity(
            "🔐",
            "LOGIN",
            &format!("User {} logged in | Wallet: {} | Balance: {} BB", 
                existing_user.username, existing_user.wallet_address, balance)
        );

        return Json(LoginResponse {
            success: true,
            user_id: Some(user_id),
            username: Some(existing_user.username),
            wallet_address: Some(existing_user.wallet_address),
            balance: Some(balance),
            is_new_user: Some(false),
            error: None,
        });
    }

    // New user - create wallet and fund with initial tokens
    let username = payload.username.unwrap_or_else(|| format!("user_{}", &user_id[..8]));
    let new_user = User::new(user_id.clone(), username.clone());
    let wallet_address = new_user.wallet_address.clone();

    // Register wallet in ledger
    app_state.ledger.accounts.insert(username.clone(), wallet_address.clone());
    
    // Fund new user with initial tokens (100 BB welcome bonus)
    let initial_balance = 100.0;
    if let Err(e) = app_state.ledger.admin_mint_tokens(&wallet_address, initial_balance) {
        return Json(LoginResponse {
            success: false,
            user_id: Some(user_id),
            username: Some(username),
            wallet_address: Some(wallet_address),
            balance: None,
            is_new_user: Some(true),
            error: Some(format!("Failed to fund wallet: {}", e)),
        });
    }

    // Store user in supabase_users map
    app_state.supabase_users.insert(user_id.clone(), new_user);

    app_state.log_blockchain_activity(
        "🆕",
        "NEW_USER",
        &format!("New user {} registered | Wallet: {} | Initial Balance: {} BB", 
            username, wallet_address, initial_balance)
    );

    Json(LoginResponse {
        success: true,
        user_id: Some(user_id),
        username: Some(username),
        wallet_address: Some(wallet_address),
        balance: Some(initial_balance),
        is_new_user: Some(true),
        error: None,
    })
}

/// GET /auth/user
/// 
/// Get the authenticated user's info from JWT in Authorization header
pub async fn get_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    // Extract token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) => t,
        None => {
            return Ok(Json(json!({
                "success": false,
                "error": "Missing Authorization header"
            })));
        }
    };

    // Get Supabase config and verify token
    let supabase_config = {
        let app_state = state.lock().unwrap();
        app_state.supabase_config.clone()
    };

    let user_id = match supabase_config.verify_token(token).await {
        Ok(id) => id,
        Err(e) => {
            return Ok(Json(json!({
                "success": false,
                "error": format!("Authentication failed: {}", e)
            })));
        }
    };

    let app_state = state.lock().unwrap();

    // Look up user
    match app_state.supabase_users.get(&user_id) {
        Some(user) => {
            let balance = app_state.ledger.get_balance(&user.wallet_address);
            Ok(Json(json!({
                "success": true,
                "user": {
                    "id": user.id,
                    "username": user.username,
                    "wallet_address": user.wallet_address,
                    "created_at": user.created_at,
                    "is_test_account": user.is_test_account
                },
                "balance": balance
            })))
        }
        None => {
            Ok(Json(json!({
                "success": false,
                "error": "User not found. Please call POST /auth/login first."
            })))
        }
    }
}
