/// BlackBook L2 - L1 RPC Client
/// 
/// HTTP client for communicating with the L1 blockchain.
/// Supports mock mode for local development without a live L1 connection.

use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default timeout for L1 RPC calls
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default L1 RPC URL (for reference only - use environment variable)
pub const DEFAULT_L1_RPC_URL: &str = "http://localhost:8080";

// ============================================================================
// ERROR TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum L1RpcError {
    /// L1 is not connected (mock mode or connection failed)
    NotConnected,
    /// HTTP request failed
    RequestFailed(String),
    /// Invalid response from L1
    InvalidResponse(String),
    /// Signature verification failed
    VerificationFailed(String),
    /// Account not found
    AccountNotFound(String),
    /// Timeout waiting for L1
    Timeout,
    /// Generic error
    Other(String),
}

impl std::fmt::Display for L1RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            L1RpcError::NotConnected => write!(f, "L1 RPC not connected"),
            L1RpcError::RequestFailed(msg) => write!(f, "L1 request failed: {}", msg),
            L1RpcError::InvalidResponse(msg) => write!(f, "Invalid L1 response: {}", msg),
            L1RpcError::VerificationFailed(msg) => write!(f, "Verification failed: {}", msg),
            L1RpcError::AccountNotFound(addr) => write!(f, "Account not found: {}", addr),
            L1RpcError::Timeout => write!(f, "L1 RPC timeout"),
            L1RpcError::Other(msg) => write!(f, "L1 RPC error: {}", msg),
        }
    }
}

impl std::error::Error for L1RpcError {}

// ============================================================================
// REQUEST/RESPONSE TYPES
// ============================================================================

/// Request to verify a signature via L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifySignatureRequest {
    pub pubkey: String,      // 64 hex chars (32 bytes)
    pub message: String,     // Base64 or hex encoded message
    pub signature: String,   // 128 hex chars (64 bytes)
}

/// Response from L1 signature verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifySignatureResponse {
    pub valid: bool,
    pub error: Option<String>,
    pub verified_at: Option<u64>,
}

/// Account info from L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1AccountInfo {
    pub address: String,
    pub balance: f64,
    pub nonce: u64,
    pub exists: bool,
}

/// Nonce response from L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonceResponse {
    pub address: String,
    pub nonce: u64,
    pub last_activity_slot: Option<u64>,
}

/// Settlement request to record on L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementRequest {
    pub market_id: String,
    pub outcome: usize,
    pub winners: Vec<(String, f64)>,  // (address, payout)
    pub l2_block_height: u64,
    pub l2_signature: String,
}

/// Settlement response from L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResponse {
    pub recorded: bool,
    pub l1_tx_hash: Option<String>,
    pub l1_slot: Option<u64>,
    pub error: Option<String>,
}

// ============================================================================
// L1 RPC CLIENT
// ============================================================================

/// Client for communicating with the L1 blockchain
pub struct L1RpcClient {
    /// L1 RPC endpoint URL
    endpoint_url: Option<String>,
    
    /// HTTP client
    client: Client,
    
    /// Request timeout
    timeout: Duration,
    
    /// Whether we're in mock mode (no real L1 connection)
    mock_mode: bool,
    
    /// Local nonce tracking (for mock mode)
    local_nonces: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl L1RpcClient {
    /// Create a new L1RpcClient with explicit endpoint URL
    pub fn new(endpoint_url: Option<String>) -> Self {
        let mock_mode = endpoint_url.is_none();
        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
        
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| Client::new());
        
        L1RpcClient {
            endpoint_url,
            client,
            timeout,
            mock_mode,
            local_nonces: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
    
    /// Create L1RpcClient from environment variable L1_RPC_URL
    pub fn from_env() -> Self {
        let endpoint_url = std::env::var("L1_RPC_URL").ok();
        Self::new(endpoint_url)
    }
    
    /// Check if connected to L1 (not in mock mode)
    pub fn is_connected(&self) -> bool {
        !self.mock_mode
    }
    
    /// Check if in mock mode
    pub fn is_mock_mode(&self) -> bool {
        self.mock_mode
    }
    
    /// Get the endpoint URL (if connected)
    pub fn endpoint_url(&self) -> Option<&str> {
        self.endpoint_url.as_deref()
    }
    
    /// Log connection status (call on startup)
    pub fn log_status(&self) {
        if self.mock_mode {
            println!("⚠️  L1 RPC: Mock mode (L1_RPC_URL not set)");
            println!("   Signatures will be verified locally using GodMode");
        } else {
            println!("🔗 L1 RPC: Connected to {}", self.endpoint_url.as_ref().unwrap());
        }
    }
    
    // ========================================================================
    // SIGNATURE VERIFICATION
    // ========================================================================
    
    /// Verify an Ed25519 signature
    /// 
    /// In mock mode: Verifies locally using ed25519-dalek
    /// In live mode: Calls L1's /rpc/verify-l1-signature endpoint
    pub async fn verify_signature(
        &self,
        pubkey_hex: &str,
        message: &[u8],
        signature_hex: &str,
    ) -> Result<bool, L1RpcError> {
        if self.mock_mode {
            self.verify_signature_local(pubkey_hex, message, signature_hex)
        } else {
            self.verify_signature_remote(pubkey_hex, message, signature_hex).await
        }
    }
    
    /// Verify signature locally using ed25519-dalek
    fn verify_signature_local(
        &self,
        pubkey_hex: &str,
        message: &[u8],
        signature_hex: &str,
    ) -> Result<bool, L1RpcError> {
        // Decode public key from hex
        let pubkey_bytes = hex::decode(pubkey_hex)
            .map_err(|e| L1RpcError::VerificationFailed(format!("Invalid pubkey hex: {}", e)))?;
        
        if pubkey_bytes.len() != 32 {
            return Err(L1RpcError::VerificationFailed(
                format!("Invalid pubkey length: expected 32, got {}", pubkey_bytes.len())
            ));
        }
        
        let mut pubkey_array = [0u8; 32];
        pubkey_array.copy_from_slice(&pubkey_bytes);
        
        let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|e| L1RpcError::VerificationFailed(format!("Invalid pubkey: {}", e)))?;
        
        // Decode signature from hex
        let sig_bytes = hex::decode(signature_hex)
            .map_err(|e| L1RpcError::VerificationFailed(format!("Invalid signature hex: {}", e)))?;
        
        if sig_bytes.len() != 64 {
            return Err(L1RpcError::VerificationFailed(
                format!("Invalid signature length: expected 64, got {}", sig_bytes.len())
            ));
        }
        
        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        
        let signature = Signature::from_bytes(&sig_array);
        
        // Verify
        match verifying_key.verify(message, &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// Verify signature synchronously (for use in non-async contexts)
    /// In mock mode: Verifies locally
    /// In live mode: Would need to block on async, so just verify locally
    pub fn verify_signature_sync(
        &self,
        pubkey_hex: &str,
        message: &[u8],
        signature_hex: &str,
    ) -> Result<bool, L1RpcError> {
        // Always use local verification for sync calls
        self.verify_signature_local(pubkey_hex, message, signature_hex)
    }
    
    /// Verify signature via L1 RPC endpoint
    async fn verify_signature_remote(
        &self,
        pubkey_hex: &str,
        message: &[u8],
        signature_hex: &str,
    ) -> Result<bool, L1RpcError> {
        let url = format!("{}/rpc/verify-l1-signature", self.endpoint_url.as_ref().unwrap());
        
        let request = VerifySignatureRequest {
            pubkey: pubkey_hex.to_string(),
            message: hex::encode(message),
            signature: signature_hex.to_string(),
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| L1RpcError::RequestFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(L1RpcError::RequestFailed(
                format!("L1 returned status {}", response.status())
            ));
        }
        
        let result: VerifySignatureResponse = response
            .json()
            .await
            .map_err(|e| L1RpcError::InvalidResponse(e.to_string()))?;
        
        Ok(result.valid)
    }
    
    // ========================================================================
    // ACCOUNT QUERIES
    // ========================================================================
    
    /// Get account balance from L1
    /// 
    /// In mock mode: Returns 0.0 (or could integrate with local ledger)
    /// In live mode: Calls L1's /rpc/account/{address} endpoint
    pub async fn get_balance(&self, address: &str) -> Result<f64, L1RpcError> {
        if self.mock_mode {
            // In mock mode, return 0.0 - actual balance is tracked locally
            Ok(0.0)
        } else {
            self.get_balance_remote(address).await
        }
    }
    
    /// Get account balance from L1 RPC
    async fn get_balance_remote(&self, address: &str) -> Result<f64, L1RpcError> {
        let url = format!("{}/rpc/account/{}", self.endpoint_url.as_ref().unwrap(), address);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| L1RpcError::RequestFailed(e.to_string()))?;
        
        if response.status().as_u16() == 404 {
            return Err(L1RpcError::AccountNotFound(address.to_string()));
        }
        
        if !response.status().is_success() {
            return Err(L1RpcError::RequestFailed(
                format!("L1 returned status {}", response.status())
            ));
        }
        
        let account: L1AccountInfo = response
            .json()
            .await
            .map_err(|e| L1RpcError::InvalidResponse(e.to_string()))?;
        
        Ok(account.balance)
    }
    
    /// Get full account info from L1
    pub async fn get_account(&self, address: &str) -> Result<L1AccountInfo, L1RpcError> {
        if self.mock_mode {
            // Return mock account info
            Ok(L1AccountInfo {
                address: address.to_string(),
                balance: 0.0,
                nonce: self.get_local_nonce(address),
                exists: true,
            })
        } else {
            self.get_account_remote(address).await
        }
    }
    
    /// Get account info from L1 RPC
    async fn get_account_remote(&self, address: &str) -> Result<L1AccountInfo, L1RpcError> {
        let url = format!("{}/rpc/account/{}", self.endpoint_url.as_ref().unwrap(), address);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| L1RpcError::RequestFailed(e.to_string()))?;
        
        if response.status().as_u16() == 404 {
            return Err(L1RpcError::AccountNotFound(address.to_string()));
        }
        
        if !response.status().is_success() {
            return Err(L1RpcError::RequestFailed(
                format!("L1 returned status {}", response.status())
            ));
        }
        
        let account: L1AccountInfo = response
            .json()
            .await
            .map_err(|e| L1RpcError::InvalidResponse(e.to_string()))?;
        
        Ok(account)
    }
    
    // ========================================================================
    // NONCE MANAGEMENT
    // ========================================================================
    
    /// Get current nonce for an address
    /// 
    /// In mock mode: Returns locally tracked nonce
    /// In live mode: Calls L1's /rpc/nonce/{address} endpoint
    pub async fn get_nonce(&self, address: &str) -> Result<u64, L1RpcError> {
        if self.mock_mode {
            Ok(self.get_local_nonce(address))
        } else {
            self.get_nonce_remote(address).await
        }
    }
    
    /// Get locally tracked nonce
    fn get_local_nonce(&self, address: &str) -> u64 {
        let nonces = self.local_nonces.lock().unwrap();
        *nonces.get(address).unwrap_or(&0)
    }
    
    /// Increment local nonce after successful transaction
    pub fn increment_nonce(&self, address: &str) {
        let mut nonces = self.local_nonces.lock().unwrap();
        let current = *nonces.get(address).unwrap_or(&0);
        nonces.insert(address.to_string(), current + 1);
    }
    
    /// Get nonce from L1 RPC
    async fn get_nonce_remote(&self, address: &str) -> Result<u64, L1RpcError> {
        let url = format!("{}/rpc/nonce/{}", self.endpoint_url.as_ref().unwrap(), address);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| L1RpcError::RequestFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(L1RpcError::RequestFailed(
                format!("L1 returned status {}", response.status())
            ));
        }
        
        let result: NonceResponse = response
            .json()
            .await
            .map_err(|e| L1RpcError::InvalidResponse(e.to_string()))?;
        
        Ok(result.nonce)
    }
    
    // ========================================================================
    // SETTLEMENT RECORDING
    // ========================================================================
    
    /// Record a market settlement on L1 for audit trail
    /// 
    /// In mock mode: Just logs and returns success
    /// In live mode: Calls L1's /rpc/settlement endpoint
    pub async fn record_settlement(
        &self,
        market_id: &str,
        outcome: usize,
        winners: Vec<(String, f64)>,
    ) -> Result<SettlementResponse, L1RpcError> {
        if self.mock_mode {
            println!("📝 [Mock] Settlement recorded for market {}: outcome {}", market_id, outcome);
            Ok(SettlementResponse {
                recorded: true,
                l1_tx_hash: Some(format!("mock_tx_{}", uuid::Uuid::new_v4().simple())),
                l1_slot: Some(0),
                error: None,
            })
        } else {
            self.record_settlement_remote(market_id, outcome, winners).await
        }
    }
    
    /// Record settlement via L1 RPC
    async fn record_settlement_remote(
        &self,
        market_id: &str,
        outcome: usize,
        winners: Vec<(String, f64)>,
    ) -> Result<SettlementResponse, L1RpcError> {
        let url = format!("{}/rpc/settlement", self.endpoint_url.as_ref().unwrap());
        
        let request = SettlementRequest {
            market_id: market_id.to_string(),
            outcome,
            winners,
            l2_block_height: 0, // TODO: Track L2 block height
            l2_signature: String::new(), // TODO: Sign with L2 validator key
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| L1RpcError::RequestFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(L1RpcError::RequestFailed(
                format!("L1 returned status {}", response.status())
            ));
        }
        
        let result: SettlementResponse = response
            .json()
            .await
            .map_err(|e| L1RpcError::InvalidResponse(e.to_string()))?;
        
        Ok(result)
    }
}

impl Default for L1RpcClient {
    fn default() -> Self {
        Self::from_env()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godmode::GodMode;
    
    #[test]
    fn test_rpc_client_creation() {
        // Without URL - should be in mock mode
        let client = L1RpcClient::new(None);
        assert!(client.is_mock_mode());
        assert!(!client.is_connected());
        assert!(client.endpoint_url().is_none());
        
        // With URL - should be connected
        let client = L1RpcClient::new(Some("http://localhost:8080".to_string()));
        assert!(!client.is_mock_mode());
        assert!(client.is_connected());
        assert_eq!(client.endpoint_url(), Some("http://localhost:8080"));
    }
    
    #[test]
    fn test_mock_mode_verification() {
        let client = L1RpcClient::new(None);
        let gm = GodMode::new();
        
        // Get Alice's account
        let alice = gm.get_account("ALICE").unwrap();
        
        // Sign a message
        let message = b"Hello, L1!";
        let signature = alice.sign_hex(message);
        let pubkey_hex = hex::encode(alice.verifying_key.as_bytes());
        
        // Verify locally (mock mode)
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.verify_signature(&pubkey_hex, message, &signature));
        
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
    
    #[test]
    fn test_mock_mode_invalid_signature() {
        let client = L1RpcClient::new(None);
        let gm = GodMode::new();
        
        // Get Alice and Bob
        let alice = gm.get_account("ALICE").unwrap();
        let bob = gm.get_account("BOB").unwrap();
        
        // Alice signs a message
        let message = b"Hello, L1!";
        let signature = alice.sign_hex(message);
        
        // Try to verify with Bob's pubkey - should fail
        let bob_pubkey = hex::encode(bob.verifying_key.as_bytes());
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.verify_signature(&bob_pubkey, message, &signature));
        
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Signature is invalid for Bob's key
    }
    
    #[test]
    fn test_mock_mode_nonce() {
        let client = L1RpcClient::new(None);
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        // Initial nonce should be 0
        let nonce = rt.block_on(client.get_nonce("test_address")).unwrap();
        assert_eq!(nonce, 0);
        
        // Increment nonce
        client.increment_nonce("test_address");
        
        // Nonce should now be 1
        let nonce = rt.block_on(client.get_nonce("test_address")).unwrap();
        assert_eq!(nonce, 1);
    }
    
    #[tokio::test]
    async fn test_mock_mode_settlement() {
        let client = L1RpcClient::new(None);
        
        let winners = vec![
            ("L1_ALICE123".to_string(), 150.0),
            ("L1_BOB456".to_string(), 50.0),
        ];
        
        let result = client.record_settlement("market_123", 0, winners).await;
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.recorded);
        assert!(response.l1_tx_hash.is_some());
    }
    
    #[tokio::test]
    async fn test_mock_mode_balance() {
        let client = L1RpcClient::new(None);
        
        // In mock mode, balance is always 0.0 (local ledger tracks real balance)
        let balance = client.get_balance("any_address").await.unwrap();
        assert_eq!(balance, 0.0);
    }
    
    #[tokio::test]
    async fn test_mock_mode_account() {
        let client = L1RpcClient::new(None);
        
        let account = client.get_account("test_address").await.unwrap();
        assert_eq!(account.address, "test_address");
        assert!(account.exists);
    }
}
