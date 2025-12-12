// BlackBook Layer 2 Prediction Market - Main Entry Point
// Clean, modular architecture with cryptographic signature verification

use axum::{
    routing::{get, post},
    Router,
};
use std::{net::SocketAddr, sync::{Arc, Mutex}};
use tower_http::cors::{Any, CorsLayer};

// Module declarations
mod market_resolve;
mod hot_upgrades;
mod auth;
mod easteregg;
mod l1_rpc_client;
mod bridge;
mod models;
mod app_state;
mod handlers;
mod routes;

#[path = "../rss/mod.rs"]
mod rss;

#[path = "../rpc/mod.rs"]
mod rpc;

use app_state::{AppState, SharedState};
use handlers::*;
use routes::auth::{connect_wallet, get_wallet_balance};

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
        .route("/markets/:id", get(get_market))
        
        // ===== AUTHENTICATION ENDPOINTS (NO JWT - CRYPTOGRAPHIC SIGNATURES ONLY) =====
        .route("/auth/connect", post(connect_wallet))
        
        // ===== BETTING ENDPOINTS =====
        .route("/bet/signed", post(place_signed_bet))
        .route("/rpc/submit", post(place_signed_bet))  // SDK compatibility alias
        .route("/bets/:account", get(get_user_bets))
        
        // ===== LEDGER ENDPOINTS =====
        .route("/balance/:account", get(get_balance))
        .route("/transfer", post(transfer))
        .route("/ledger", get(get_ledger_activity))
        
        // ===== RPC ENDPOINTS =====
        .route("/rpc/nonce/:address", get(get_nonce))
        
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
    println!("   GET  /markets/:id       - Get market details");
    println!("   POST /bet/signed        - Place bet (cryptographic signature)");
    println!("   POST /rpc/submit        - Place bet (SDK alias)");
    println!("   GET  /bets/:account     - Get user bet history");
    println!("   GET  /balance/:account  - Get account balance");
    println!("   POST /transfer          - Transfer tokens");
    println!("   GET  /ledger            - View blockchain activity");
    println!("   GET  /rpc/nonce/:addr   - Get nonce for signing");
    println!("\n📡 Cryptographic signatures only - No JWT required\n");

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
