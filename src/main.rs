// BlackBook Layer 2 Prediction Market - Main Entry Point
// Clean, modular architecture with cryptographic signature verification

use axum::{
    routing::{get, post, delete},
    Router,
};
use std::{net::SocketAddr, sync::{Arc, Mutex}};
use tower_http::cors::{Any, CorsLayer};

// Module declarations
mod market_resolve;
mod auth;
mod easteregg;
mod bridge;
mod models;
mod ledger;
mod app_state;
mod handlers;
mod routes;
mod orderbook;
mod shares;

#[path = "../rss/mod.rs"]
mod rss;

#[path = "../rpc/mod.rs"]
mod rpc;

use app_state::{AppState, SharedState};
use handlers::*;
use routes::auth::connect_wallet;

#[tokio::main]
async fn main() {
    println!("\n═══════════════════════════════════════════════");
    println!("     🎲 BlackBook Layer 2 Prediction Market");
    println!("═══════════════════════════════════════════════\n");

    // Initialize application state
    let state: SharedState = Arc::new(Mutex::new(AppState::new()));
    
    // Clone state for shutdown handler before moving into router
    let shutdown_state = state.clone();

    // Build router with all endpoints
    let app = Router::new()
        // ===== CORE MARKET ENDPOINTS =====
        .route("/markets", get(get_markets))
        .route("/markets", post(create_market))
        .route("/markets/initial-liquidity", post(initialize_all_market_liquidity))
        .route("/markets/initial-liquidity/:market_id", post(initialize_market_liquidity))
        .route("/markets/:id", get(get_market))
        .route("/markets/:id/odds", get(get_market_odds))
        .route("/markets/:id/resolution", get(get_market_resolution))
        
        // ===== MARKET RESOLUTION ENDPOINTS =====
        .route("/markets/:id/resolve", post(resolve_market))
        .route("/resolve/:market_id/:outcome", post(admin_resolve_market))  // SDK compatibility
        .route("/admin/resolve/:market_id/:outcome", post(admin_resolve_market))
        
        // ===== AUTHENTICATION ENDPOINTS (NO JWT - CRYPTOGRAPHIC SIGNATURES ONLY) =====
        .route("/auth/connect", post(connect_wallet))
        
        // ===== CLOB ORDER BOOK ENDPOINTS =====
        .route("/orders", post(submit_order))
        .route("/orders/:order_id", delete(cancel_order))
        .route("/orders/user/:wallet", get(get_user_orders))
        .route("/orderbook/:market_id", get(get_orderbook))
        .route("/orderbook/:market_id/:outcome", get(get_orderbook_outcome))
        .route("/trades/:market_id", get(get_recent_trades))
        
        // ===== SHARES ENDPOINTS =====
        .route("/shares/mint", post(mint_shares))
        .route("/shares/redeem", post(redeem_shares))
        .route("/shares/claim/:market_id", post(claim_market_winnings))  // Claim winnings after resolution
        .route("/positions/:wallet", get(get_positions))
        .route("/positions/:wallet/:market_id", get(get_market_positions))
        
        // ===== BETTING ENDPOINTS (Legacy - routes to CLOB/CPMM hybrid) =====
        .route("/bet/signed", post(place_signed_bet))
        .route("/rpc/submit", post(place_signed_bet))  // SDK compatibility alias
        .route("/bets/:account", get(get_user_bets))
        
        // ===== LEDGER ENDPOINTS =====
        .route("/balance/:account", get(get_balance))
        .route("/balance/details/:account", get(get_balance_details))  // Hybrid balance details
        .route("/transfer", post(transfer))
        .route("/ledger", get(get_ledger_activity))
        .route("/ledger/transactions", get(get_ledger_transactions))  // Public ledger with filtering
        
        // ===== L1 SETTLEMENT ENDPOINTS =====
        .route("/settle", post(settle_to_l1_real))           // Submit resolutions to L1
        .route("/settle/pending", get(get_pending_settlements)) // View pending settlements
        .route("/settle/status", get(get_settlement_status)) // Get settlement status
        .route("/sync", post(sync_from_l1))                 // Sync balances from L1
        
        // ===== BRIDGE ENDPOINTS (L1↔L2 Token Movement) =====
        .route("/bridge/deposit", post(bridge_deposit))     // L1→L2 (receive from L1)
        .route("/bridge/withdraw", post(bridge_withdraw))   // L2→L1 (send to L1)
        .route("/bridge/status/:bridge_id", get(get_bridge_status))
        .route("/bridge/list/:wallet", get(list_wallet_bridges))
        .route("/bridge/stats", get(get_bridge_stats))
        
        // ===== ORACLE/ADMIN MANAGEMENT =====
        .route("/admin/oracles", post(add_oracle))
        .route("/admin/oracles", get(list_oracles))
        .route("/admin/oracles/:address", delete(remove_oracle))
        
        // ===== RPC ENDPOINTS =====
        .route("/rpc/nonce/:address", get(get_nonce))
        
        // ===== STATISTICS ENDPOINTS =====
        .route("/stats/orderbook", get(get_orderbook_stats))
        .route("/stats/shares", get(get_shares_stats))
        
        // ===== HEALTH CHECK =====
        .route("/", get(health_check))
        .route("/health", get(health_check))
        
        // Apply CORS and state
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 1234));
    
    println!("\n╔════════════════════════════════════════════╗");
    println!("║   🚀 SERVER RUNNING                        ║");
    println!("║   📡 http://0.0.0.0:1234                   ║");
    println!("╚════════════════════════════════════════════╝\n");
    
    println!("📋 Available Endpoints:");
    println!("   POST /auth/connect      - Connect wallet (creates & funds if new)");
    println!("   GET  /markets           - List all prediction markets");
    println!("   POST /markets           - Create new market");
    println!("   POST /markets/initial-liquidity - Init CPMM + L1 mint for all markets");
    println!("   GET  /markets/:id       - Get market details");
    println!("   GET  /markets/:id/odds  - Get dynamic odds (CLOB/CPMM hybrid)");
    println!("");
    println!("   ═══ MARKET RESOLUTION ═══");
    println!("   POST /markets/:id/resolve - Resolve market (oracle/admin only)");
    println!("   POST /resolve/:id/:outcome - Admin shortcut to resolve");
    println!("   GET  /markets/:id/resolution - Get resolution details");
    println!("   POST /shares/claim/:id  - Claim winnings after resolution");
    println!("");
    println!("   ═══ CLOB ORDER BOOK ═══");
    println!("   POST /orders            - Submit limit order");
    println!("   DELETE /orders/:id      - Cancel order");
    println!("   GET  /orders/user/:wallet - Get user's open orders");
    println!("   GET  /orderbook/:market_id - Get order book depth");
    println!("   GET  /trades/:market_id - Get recent trades");
    println!("");
    println!("   ═══ OUTCOME SHARES ═══");
    println!("   POST /shares/mint       - Mint YES+NO shares (1 BB → 1 YES + 1 NO)");
    println!("   POST /shares/redeem     - Redeem shares (1 YES + 1 NO → 1 BB)");
    println!("   GET  /positions/:wallet - Get all user positions");
    println!("");
    println!("   ═══ L1↔L2 BRIDGE ═══");
    println!("   POST /bridge/deposit    - L1→L2 deposit (receive from L1)");
    println!("   POST /bridge/withdraw   - L2→L1 withdraw (send to L1)");
    println!("   GET  /bridge/status/:id - Get bridge status");
    println!("   GET  /bridge/list/:wallet - List wallet bridges");
    println!("   GET  /bridge/stats      - Bridge statistics");
    println!("");
    println!("   ═══ L1 SETTLEMENT ═══");
    println!("   POST /settle            - Submit resolutions to L1");
    println!("   GET  /settle/pending    - View pending settlements");
    println!("   GET  /settle/status     - Get settlement status");
    println!("");
    println!("   ═══ ADMIN/ORACLE ═══");
    println!("   POST /admin/oracles     - Add oracle to whitelist");
    println!("   GET  /admin/oracles     - List whitelisted oracles");
    println!("   DELETE /admin/oracles/:addr - Remove oracle");
    println!("");
    println!("   ═══ LEGACY ENDPOINTS ═══");
    println!("   POST /bet/signed        - Place bet (cryptographic signature)");
    println!("   GET  /balance/:account  - Get account balance");
    println!("   POST /transfer          - Transfer tokens");
    println!("   GET  /ledger            - View blockchain activity");
    println!("   GET  /rpc/nonce/:addr   - Get nonce for signing");


    // Setup graceful shutdown
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    
    // Spawn shutdown handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        
        println!("\n\n🛑 Shutdown signal received...");
        println!("💾 Saving state to disk...");
        
        if let Ok(app_state) = shutdown_state.lock() {
            if let Err(e) = app_state.save_to_disk() {
                eprintln!("❌ Failed to save state: {}", e);
            } else {
                println!("✅ State saved successfully");
            }
        }
        
        println!("👋 Goodbye!\n");
        std::process::exit(0);
    });

    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "BlackBook Layer 2 Prediction Market - Online ✅"
}
