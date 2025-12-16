/// Integration tests using Alice & Bob test accounts
/// 
/// These tests use deterministic test keys from alice-bob.txt
/// ⚠️ These keys are for testing only - NEVER use in production!

use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// TEST ACCOUNT CONSTANTS (from alice-bob.txt)
// ============================================================================

const ALICE_ADDRESS: &str = "L1ALICE000000001";
const ALICE_PUBLIC_KEY: &str = "4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57";
#[allow(dead_code)]
const ALICE_PRIVATE_KEY: &str = "616c6963655f746573745f6163636f756e745f76310000000000000000000001";
const ALICE_USERNAME: &str = "alice_test";

const BOB_ADDRESS: &str = "L1BOB00000000001";
const BOB_PUBLIC_KEY: &str = "b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f";
#[allow(dead_code)]
const BOB_PRIVATE_KEY: &str = "626f625f746573745f6163636f756e745f763100000000000000000000000002";
const BOB_USERNAME: &str = "bob_test";

const BASE_URL: &str = "http://localhost:1234";
#[allow(dead_code)]
const L1_URL: &str = "http://localhost:8080";

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Create a test signature (mock for testing - in production use Ed25519)
fn mock_signature(address: &str, timestamp: u64) -> String {
    format!("sig_{}_{}", address, timestamp)
}

// ============================================================================
// WALLET CONNECTION TESTS
// ============================================================================

#[tokio::test]
async fn test_alice_wallet_connect() {
    let client = reqwest::Client::new();
    
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME,
        "public_key": ALICE_PUBLIC_KEY
    });

    let response = client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect Alice's wallet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("🔐 Alice wallet connect response: {}", serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(status, 200);
    assert_eq!(body["success"], true);
    assert_eq!(body["wallet_address"], ALICE_ADDRESS);
}

#[tokio::test]
async fn test_bob_wallet_connect() {
    let client = reqwest::Client::new();
    
    let connect_payload = json!({
        "wallet_address": BOB_ADDRESS,
        "username": BOB_USERNAME,
        "public_key": BOB_PUBLIC_KEY
    });

    let response = client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect Bob's wallet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("🔐 Bob wallet connect response: {}", serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(status, 200);
    assert_eq!(body["success"], true);
    assert_eq!(body["wallet_address"], BOB_ADDRESS);
}

// ============================================================================
// BALANCE TESTS
// ============================================================================

#[tokio::test]
async fn test_alice_initial_balance() {
    let client = reqwest::Client::new();
    
    // First connect Alice
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Check balance
    let response = client
        .get(format!("{}/balance/{}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get balance");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("💰 Alice balance: {}", serde_json::to_string_pretty(&body).unwrap());

    assert!(body["balance"].as_f64().is_some(), "Balance should be a number");
}

#[tokio::test]
async fn test_alice_balance_details() {
    let client = reqwest::Client::new();
    
    // Connect Alice first
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Get detailed balance
    let response = client
        .get(format!("{}/balance/details/{}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get balance details");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("💰 Alice balance details: {}", serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(body["success"], true);
    assert!(body["confirmed_balance"].as_f64().is_some());
    assert!(body["available_balance"].as_f64().is_some());
}

// ============================================================================
// BETTING TESTS - ALICE
// ============================================================================

#[tokio::test]
async fn test_alice_places_yes_bet() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    
    // Connect Alice's wallet first
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Place a YES bet
    let bet_payload = json!({
        "signature": mock_signature(ALICE_ADDRESS, timestamp),
        "from_address": ALICE_ADDRESS,
        "market_id": "pegatron_georgetown_2026",
        "option": "YES",
        "amount": 50.0,
        "nonce": timestamp,  // Use timestamp as unique nonce
        "timestamp": timestamp
    });

    println!("📤 Alice placing YES bet: {}", serde_json::to_string_pretty(&bet_payload).unwrap());

    let response = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to place bet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse");

    println!("📥 Bet response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());

    if status == 200 {
        assert_eq!(body["success"], true);
        assert_eq!(body["outcome"], 0);  // YES = 0
        assert_eq!(body["amount"], 50.0);
    }
}

#[tokio::test]
async fn test_alice_places_no_bet() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    
    // Connect wallet
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Place a NO bet
    let bet_payload = json!({
        "signature": mock_signature(ALICE_ADDRESS, timestamp),
        "from_address": ALICE_ADDRESS,
        "market_id": "tesla_robotaxi_safety",
        "option": "NO",
        "amount": 25.0,
        "nonce": timestamp + 1,
        "timestamp": timestamp
    });

    println!("📤 Alice placing NO bet: {}", serde_json::to_string_pretty(&bet_payload).unwrap());

    let response = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to place bet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse");

    println!("📥 Bet response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());

    if status == 200 {
        assert_eq!(body["success"], true);
        assert_eq!(body["outcome"], 1);  // NO = 1
    }
}

// ============================================================================
// BETTING TESTS - BOB
// ============================================================================

#[tokio::test]
async fn test_bob_places_bet() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    
    // Connect Bob's wallet
    let connect_payload = json!({
        "wallet_address": BOB_ADDRESS,
        "username": BOB_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Bob places a bet
    let bet_payload = json!({
        "signature": mock_signature(BOB_ADDRESS, timestamp),
        "from_address": BOB_ADDRESS,
        "market_id": "pegatron_georgetown_2026",
        "option": "0",  // YES
        "amount": 30.0,
        "nonce": timestamp,
        "timestamp": timestamp
    });

    println!("📤 Bob placing bet: {}", serde_json::to_string_pretty(&bet_payload).unwrap());

    let response = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to place bet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse");

    println!("📥 Bet response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());
}

// ============================================================================
// TRANSFER TESTS - ALICE TO BOB
// ============================================================================

#[tokio::test]
async fn test_alice_transfer_to_bob() {
    let client = reqwest::Client::new();
    
    // Connect both wallets
    for (address, username) in [(ALICE_ADDRESS, ALICE_USERNAME), (BOB_ADDRESS, BOB_USERNAME)] {
        let connect_payload = json!({
            "wallet_address": address,
            "username": username
        });
        client
            .post(format!("{}/auth/connect", BASE_URL))
            .json(&connect_payload)
            .send()
            .await
            .expect("Failed to connect wallet");
    }

    // Get Alice's initial balance
    let alice_balance_before: serde_json::Value = client
        .get(format!("{}/balance/{}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get balance")
        .json()
        .await
        .expect("Failed to parse");

    println!("💰 Alice balance before transfer: {:?}", alice_balance_before["balance"]);

    // Alice transfers 10 BB to Bob
    let transfer_payload = json!({
        "from": ALICE_ADDRESS,
        "to": BOB_ADDRESS,
        "amount": 10.0
    });

    let response = client
        .post(format!("{}/transfer", BASE_URL))
        .json(&transfer_payload)
        .send()
        .await
        .expect("Failed to transfer");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("💸 Transfer result: {}", serde_json::to_string_pretty(&body).unwrap());

    if body["success"] == true {
        // Check balances after
        let alice_balance_after: serde_json::Value = client
            .get(format!("{}/balance/{}", BASE_URL, ALICE_ADDRESS))
            .send()
            .await
            .expect("Failed to get balance")
            .json()
            .await
            .expect("Failed to parse");

        let bob_balance_after: serde_json::Value = client
            .get(format!("{}/balance/{}", BASE_URL, BOB_ADDRESS))
            .send()
            .await
            .expect("Failed to get balance")
            .json()
            .await
            .expect("Failed to parse");

        println!("💰 Alice balance after: {:?}", alice_balance_after["balance"]);
        println!("💰 Bob balance after: {:?}", bob_balance_after["balance"]);
    }
}

// ============================================================================
// MARKET TESTS
// ============================================================================

#[tokio::test]
async fn test_get_markets() {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/markets", BASE_URL))
        .send()
        .await
        .expect("Failed to get markets");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("📊 Markets: {}", serde_json::to_string_pretty(&body).unwrap());

    assert!(body["markets"].is_array());
}

#[tokio::test]
async fn test_get_specific_market() {
    let client = reqwest::Client::new();
    let market_id = "pegatron_georgetown_2026";

    let response = client
        .get(format!("{}/markets/{}", BASE_URL, market_id))
        .send()
        .await
        .expect("Failed to get market");

    let status = response.status();
    if status == 200 {
        let body: serde_json::Value = response.json().await.expect("Failed to parse");
        println!("📊 Market {}: {}", market_id, serde_json::to_string_pretty(&body).unwrap());
        
        assert_eq!(body["id"], market_id);
    } else {
        println!("⚠️ Market {} not found ({})", market_id, status);
    }
}

// ============================================================================
// USER BETS HISTORY TESTS
// ============================================================================

#[tokio::test]
async fn test_alice_bet_history() {
    let client = reqwest::Client::new();
    
    // Connect Alice
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Get Alice's bet history
    let response = client
        .get(format!("{}/bets/{}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get bet history");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("📋 Alice's bets: {}", serde_json::to_string_pretty(&body).unwrap());
}

// ============================================================================
// LEDGER TRANSACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_get_ledger_activity() {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/ledger", BASE_URL))
        .send()
        .await
        .expect("Failed to get ledger");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("📜 Ledger activity: {}", serde_json::to_string_pretty(&body).unwrap());
}

#[tokio::test]
async fn test_get_ledger_transactions_filtered() {
    let client = reqwest::Client::new();

    // Get only bet transactions
    let response = client
        .get(format!("{}/ledger/transactions?type=bet&limit=10", BASE_URL))
        .send()
        .await
        .expect("Failed to get transactions");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("📜 Bet transactions: {}", serde_json::to_string_pretty(&body).unwrap());
}

#[tokio::test]
async fn test_get_alice_transactions() {
    let client = reqwest::Client::new();

    // Get Alice's transactions
    let response = client
        .get(format!("{}/ledger/transactions?account={}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get transactions");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("📜 Alice's transactions: {}", serde_json::to_string_pretty(&body).unwrap());
}

// ============================================================================
// NONCE TESTS
// ============================================================================

#[tokio::test]
async fn test_get_nonce() {
    let client = reqwest::Client::new();
    
    // Connect Alice first
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Get nonce
    let response = client
        .get(format!("{}/rpc/nonce/{}", BASE_URL, ALICE_ADDRESS))
        .send()
        .await
        .expect("Failed to get nonce");

    let body: serde_json::Value = response.json().await.expect("Failed to parse");
    println!("🔢 Alice's nonce: {}", serde_json::to_string_pretty(&body).unwrap());
}

// ============================================================================
// ERROR CASE TESTS
// ============================================================================

#[tokio::test]
async fn test_bet_insufficient_balance() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    
    // Connect wallet
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Try to bet more than balance
    let bet_payload = json!({
        "signature": mock_signature(ALICE_ADDRESS, timestamp),
        "from_address": ALICE_ADDRESS,
        "market_id": "pegatron_georgetown_2026",
        "option": "YES",
        "amount": 999999.0,  // Huge amount
        "nonce": timestamp + 100,
        "timestamp": timestamp
    });

    let response = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse");

    println!("❌ Insufficient balance response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());

    // Should fail
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_bet_invalid_market() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    
    // Connect wallet
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Try to bet on non-existent market
    let bet_payload = json!({
        "signature": mock_signature(ALICE_ADDRESS, timestamp),
        "from_address": ALICE_ADDRESS,
        "market_id": "totally_fake_market_12345",
        "option": "YES",
        "amount": 10.0,
        "nonce": timestamp + 200,
        "timestamp": timestamp
    });

    let response = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse");

    println!("❌ Invalid market response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(status, 404);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_bet_replay_attack_protection() {
    let client = reqwest::Client::new();
    let timestamp = current_timestamp();
    let nonce = timestamp + 300;
    
    // Connect wallet
    let connect_payload = json!({
        "wallet_address": ALICE_ADDRESS,
        "username": ALICE_USERNAME
    });

    client
        .post(format!("{}/auth/connect", BASE_URL))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // First bet should succeed
    let bet_payload = json!({
        "signature": mock_signature(ALICE_ADDRESS, timestamp),
        "from_address": ALICE_ADDRESS,
        "market_id": "pegatron_georgetown_2026",
        "option": "YES",
        "amount": 5.0,
        "nonce": nonce,
        "timestamp": timestamp
    });

    let response1 = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let body1: serde_json::Value = response1.json().await.expect("Failed to parse");
    println!("📤 First bet: {}", serde_json::to_string_pretty(&body1).unwrap());

    // Second bet with same nonce should fail (replay attack)
    let response2 = client
        .post(format!("{}/bet/signed", BASE_URL))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let status2 = response2.status();
    let body2: serde_json::Value = response2.json().await.expect("Failed to parse");

    println!("❌ Replay attack response ({}): {}", status2, serde_json::to_string_pretty(&body2).unwrap());

    // If first succeeded, second should fail due to nonce
    if body1["success"] == true {
        assert_eq!(body2["success"], false);
        assert!(body2["error"].as_str().unwrap_or("").contains("nonce"));
    }
}

// ============================================================================
// HEALTH CHECK TESTS
// ============================================================================

#[tokio::test]
async fn test_health_check() {
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", BASE_URL))
        .send()
        .await
        .expect("Failed health check");

    assert_eq!(response.status(), 200);
    println!("✅ Health check passed");
}

#[tokio::test]
async fn test_root_endpoint() {
    let client = reqwest::Client::new();

    let response = client
        .get(BASE_URL)
        .send()
        .await
        .expect("Failed to reach root");

    assert_eq!(response.status(), 200);
    println!("✅ Root endpoint accessible");
}
