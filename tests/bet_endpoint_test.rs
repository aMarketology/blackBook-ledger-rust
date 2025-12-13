use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_wallet_connect() {
    // Start test server
    let client = reqwest::Client::new();
    let base_url = "http://localhost:1234";

    // Test wallet connection
    let connect_payload = json!({
        "wallet_address": "L1TEST123456",
        "username": "test_user_integration"
    });

    let response = client
        .post(format!("{}/auth/connect", base_url))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    
    assert_eq!(body["success"], true);
    assert_eq!(body["wallet_address"], "L1TEST123456");
    assert_eq!(body["balance"], 100.0);
    assert_eq!(body["is_new_account"], true);
}

#[tokio::test]
async fn test_place_bet_simple_format() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:1234";
    
    // First, connect wallet
    let wallet_address = "L1BETTEST999";
    let connect_payload = json!({
        "wallet_address": wallet_address,
        "username": "bet_tester"
    });

    client
        .post(format!("{}/auth/connect", base_url))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Place bet with simple flat format
    let bet_payload = json!({
        "signature": format!("sig_test_{}", timestamp),
        "from_address": wallet_address,
        "market_id": "pegatron_georgetown_2026",
        "option": "0",
        "amount": 10.0,
        "nonce": 1,
        "timestamp": timestamp
    });

    println!("📤 Sending bet request: {}", serde_json::to_string_pretty(&bet_payload).unwrap());

    let response = client
        .post(format!("{}/bet/signed", base_url))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to place bet");

    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");

    println!("📥 Response ({}): {}", status, serde_json::to_string_pretty(&body).unwrap());

    assert_eq!(status, 200, "Expected 200 OK, got: {} - {}", status, body);
    assert_eq!(body["success"], true, "Bet should succeed");
    assert_eq!(body["market_id"], "pegatron_georgetown_2026");
    assert_eq!(body["outcome"], 0);
    assert_eq!(body["amount"], 10.0);
    assert_eq!(body["new_balance"], 90.0);
}

#[tokio::test]
async fn test_place_bet_invalid_option() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:1234";
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Try to place bet with invalid option
    let bet_payload = json!({
        "signature": "sig_test",
        "from_address": "L1TEST123456",
        "market_id": "pegatron_georgetown_2026",
        "option": "99",  // Invalid option
        "amount": 10.0,
        "nonce": 1,
        "timestamp": timestamp
    });

    let response = client
        .post(format!("{}/bet/signed", base_url))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert_eq!(status, 400, "Should return 400 for invalid option");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("Invalid option"));
}

#[tokio::test]
async fn test_place_bet_expired_timestamp() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:1234";
    
    // Use timestamp from 2 days ago (beyond 24 hour window)
    let old_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() - (48 * 3600);

    let bet_payload = json!({
        "signature": "sig_test",
        "from_address": "L1TEST123456",
        "market_id": "pegatron_georgetown_2026",
        "option": "0",
        "amount": 10.0,
        "nonce": 1,
        "timestamp": old_timestamp
    });

    let response = client
        .post(format!("{}/bet/signed", base_url))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert_eq!(status, 401, "Should return 401 for expired timestamp");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("expired"));
}

#[tokio::test]
async fn test_check_balance_after_bet() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:1234";
    
    let wallet_address = "L1BALANCETEST";
    
    // Connect wallet (gets 100 BB)
    let connect_payload = json!({
        "wallet_address": wallet_address,
        "username": "balance_tester"
    });

    client
        .post(format!("{}/auth/connect", base_url))
        .json(&connect_payload)
        .send()
        .await
        .expect("Failed to connect wallet");

    // Check initial balance
    let response = client
        .get(format!("{}/balance/{}", base_url, wallet_address))
        .send()
        .await
        .expect("Failed to get balance");

    let body: serde_json::Value = response.json().await.expect("Failed to parse balance");
    assert_eq!(body["balance"], 100.0);

    // Place a bet
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let bet_payload = json!({
        "signature": format!("sig_{}", timestamp),
        "from_address": wallet_address,
        "market_id": "pegatron_georgetown_2026",
        "option": "0",
        "amount": 25.0,
        "nonce": 1,
        "timestamp": timestamp
    });

    client
        .post(format!("{}/bet/signed", base_url))
        .json(&bet_payload)
        .send()
        .await
        .expect("Failed to place bet");

    // Check balance after bet
    let response = client
        .get(format!("{}/balance/{}", base_url, wallet_address))
        .send()
        .await
        .expect("Failed to get balance");

    let body: serde_json::Value = response.json().await.expect("Failed to parse balance");
    assert_eq!(body["balance"], 75.0, "Balance should be 75 BB after 25 BB bet");
}
