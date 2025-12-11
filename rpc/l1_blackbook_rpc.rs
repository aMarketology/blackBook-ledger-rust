// ============================================================================
// L1 BlackBook RPC - Layer 1 Blockchain Communication
// ============================================================================
//
// Wrapper for L1 blockchain RPC calls specific to BlackBook.
// Provides high-level methods for wallet operations, settlements, and bridging.
//
// L1 Endpoints:
//   GET  /health              - L1 health check
//   GET  /balance/:address    - Get wallet balance
//   GET  /rpc/nonce/:address  - Get account nonce
//   POST /rpc/verify          - Verify signature
//   POST /rpc/settlement      - Record market settlement
//   GET  /auth/wallet/:userId - Get wallet by Supabase user ID
//   GET  /poh/status          - Proof of History status
//
// ============================================================================

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default L1 RPC endpoint
pub const L1_DEFAULT_ENDPOINT: &str = "http://localhost:8080";

/// Default timeout for L1 calls (seconds)
pub const L1_TIMEOUT_SECS: u64 = 30;

/// Retry attempts for failed L1 calls
pub const L1_RETRY_ATTEMPTS: u32 = 3;

/// Retry delay between attempts (milliseconds)
pub const L1_RETRY_DELAY_MS: u64 = 500;

// ============================================================================
// L1 RESPONSE TYPES
// ============================================================================

/// L1 health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1HealthResponse {
    pub status: String,
    pub version: Option<String>,
    pub slot: Option<u64>,
    pub epoch: Option<u64>,
    pub uptime_secs: Option<u64>,
}

/// L1 wallet lookup response (by Supabase user ID)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1WalletLookupResponse {
    pub found: bool,
    pub user_id: String,
    pub wallet_address: Option<String>,
    pub registered_at: Option<u64>,
}

/// L1 balance response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1BalanceResponse {
    pub address: String,
    pub balance: f64,
    pub exists: bool,
}

/// L1 Proof of History status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1PoHStatus {
    pub enabled: bool,
    pub current_slot: u64,
    pub current_hash: String,
    pub tick_rate_ms: u64,
    pub entries_since_genesis: u64,
}

/// Bridge request to L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1BridgeRequest {
    pub from_l2_address: String,
    pub to_l1_address: String,
    pub amount: f64,
    pub l2_tx_hash: String,
    pub signature: String,
}

/// Bridge response from L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1BridgeResponse {
    pub success: bool,
    pub bridge_id: Option<String>,
    pub l1_tx_hash: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

// ============================================================================
// L1 RPC CONFIG
// ============================================================================

/// Configuration for L1 RPC connection
#[derive(Debug, Clone)]
pub struct L1RpcConfig {
    /// L1 endpoint URL
    pub endpoint: String,
    
    /// Request timeout
    pub timeout: Duration,
    
    /// Number of retry attempts
    pub retry_attempts: u32,
    
    /// Delay between retries
    pub retry_delay: Duration,
    
    /// Whether to use mock mode (no real L1)
    pub mock_mode: bool,
}

impl Default for L1RpcConfig {
    fn default() -> Self {
        Self {
            endpoint: L1_DEFAULT_ENDPOINT.to_string(),
            timeout: Duration::from_secs(L1_TIMEOUT_SECS),
            retry_attempts: L1_RETRY_ATTEMPTS,
            retry_delay: Duration::from_millis(L1_RETRY_DELAY_MS),
            mock_mode: false,
        }
    }
}

impl L1RpcConfig {
    /// Create config from environment variable
    pub fn from_env() -> Self {
        let endpoint = std::env::var("L1_RPC_URL")
            .unwrap_or_else(|_| L1_DEFAULT_ENDPOINT.to_string());
        
        let mock_mode = std::env::var("L1_MOCK_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        
        Self {
            endpoint,
            mock_mode,
            ..Default::default()
        }
    }
    
    /// Create mock mode config (for testing)
    pub fn mock() -> Self {
        Self {
            mock_mode: true,
            ..Default::default()
        }
    }
}

// ============================================================================
// L1 BLACKBOOK RPC CLIENT
// ============================================================================

/// High-level L1 RPC client for BlackBook operations
#[derive(Debug, Clone)]
pub struct L1BlackBookRpc {
    /// Configuration
    pub config: L1RpcConfig,
    
    /// Whether connected to L1
    pub connected: bool,
    
    /// Last successful call timestamp
    pub last_call: Option<u64>,
}

impl L1BlackBookRpc {
    /// Create a new L1 BlackBook RPC client
    pub fn new(config: L1RpcConfig) -> Self {
        Self {
            config,
            connected: false,
            last_call: None,
        }
    }
    
    /// Create from environment
    pub fn from_env() -> Self {
        Self::new(L1RpcConfig::from_env())
    }
    
    /// Create in mock mode
    pub fn mock() -> Self {
        Self::new(L1RpcConfig::mock())
    }
    
    /// Get the L1 endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
    
    /// Check if in mock mode
    pub fn is_mock(&self) -> bool {
        self.config.mock_mode
    }
    
    /// Update last call timestamp
    fn update_last_call(&mut self) {
        self.last_call = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    // ========================================================================
    // HEALTH & STATUS
    // ========================================================================
    
    /// Check L1 health
    /// GET /health
    pub async fn health(&mut self) -> Result<L1HealthResponse, String> {
        if self.config.mock_mode {
            return Ok(L1HealthResponse {
                status: "healthy".to_string(),
                version: Some("mock-1.0.0".to_string()),
                slot: Some(12345),
                epoch: Some(100),
                uptime_secs: Some(3600),
            });
        }
        
        let url = format!("{}/health", self.config.endpoint);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("L1 health check failed: {}", e))?;
        
        if response.status().is_success() {
            self.connected = true;
            self.update_last_call();
            
            response
                .json::<L1HealthResponse>()
                .await
                .map_err(|e| format!("Failed to parse L1 health response: {}", e))
        } else {
            self.connected = false;
            Err(format!("L1 health check returned status: {}", response.status()))
        }
    }
    
    /// Get PoH status
    /// GET /poh/status
    pub async fn poh_status(&mut self) -> Result<L1PoHStatus, String> {
        if self.config.mock_mode {
            return Ok(L1PoHStatus {
                enabled: true,
                current_slot: 12345,
                current_hash: "0".repeat(64),
                tick_rate_ms: 400,
                entries_since_genesis: 1_000_000,
            });
        }
        
        let url = format!("{}/poh/status", self.config.endpoint);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("L1 PoH status failed: {}", e))?;
        
        self.update_last_call();
        
        response
            .json::<L1PoHStatus>()
            .await
            .map_err(|e| format!("Failed to parse L1 PoH status: {}", e))
    }
    
    // ========================================================================
    // WALLET OPERATIONS
    // ========================================================================
    
    /// Get wallet by Supabase user ID
    /// GET /auth/wallet/:userId
    pub async fn get_wallet_by_user_id(&mut self, user_id: &str) -> Result<L1WalletLookupResponse, String> {
        if self.config.mock_mode {
            // In mock mode, return a deterministic wallet address
            let mock_address = format!("{:0>64}", hex::encode(user_id.as_bytes()));
            return Ok(L1WalletLookupResponse {
                found: true,
                user_id: user_id.to_string(),
                wallet_address: Some(mock_address),
                registered_at: Some(1700000000),
            });
        }
        
        let url = format!("{}/auth/wallet/{}", self.config.endpoint, user_id);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("L1 wallet lookup failed: {}", e))?;
        
        self.update_last_call();
        
        if response.status().is_success() {
            response
                .json::<L1WalletLookupResponse>()
                .await
                .map_err(|e| format!("Failed to parse wallet lookup response: {}", e))
        } else if response.status().as_u16() == 404 {
            Ok(L1WalletLookupResponse {
                found: false,
                user_id: user_id.to_string(),
                wallet_address: None,
                registered_at: None,
            })
        } else {
            Err(format!("L1 wallet lookup returned status: {}", response.status()))
        }
    }
    
    /// Get balance for an address
    /// GET /balance/:address
    pub async fn get_balance(&mut self, address: &str) -> Result<L1BalanceResponse, String> {
        if self.config.mock_mode {
            return Ok(L1BalanceResponse {
                address: address.to_string(),
                balance: 10000.0, // Mock balance
                exists: true,
            });
        }
        
        let url = format!("{}/balance/{}", self.config.endpoint, address);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("L1 balance lookup failed: {}", e))?;
        
        self.update_last_call();
        
        response
            .json::<L1BalanceResponse>()
            .await
            .map_err(|e| format!("Failed to parse balance response: {}", e))
    }
    
    /// Get nonce for an address
    /// GET /rpc/nonce/:address
    pub async fn get_nonce(&mut self, address: &str) -> Result<u64, String> {
        if self.config.mock_mode {
            return Ok(0); // Mock nonce starts at 0
        }
        
        let url = format!("{}/rpc/nonce/{}", self.config.endpoint, address);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("L1 nonce lookup failed: {}", e))?;
        
        self.update_last_call();
        
        #[derive(Deserialize)]
        struct NonceResp {
            nonce: u64,
        }
        
        let nonce_resp = response
            .json::<NonceResp>()
            .await
            .map_err(|e| format!("Failed to parse nonce response: {}", e))?;
        
        Ok(nonce_resp.nonce)
    }
    
    // ========================================================================
    // BRIDGE OPERATIONS
    // ========================================================================
    
    /// Request bridge from L2 to L1
    /// POST /rpc/bridge
    pub async fn bridge_to_l1(&mut self, request: L1BridgeRequest) -> Result<L1BridgeResponse, String> {
        if self.config.mock_mode {
            return Ok(L1BridgeResponse {
                success: true,
                bridge_id: Some(format!("bridge_{}", uuid::Uuid::new_v4())),
                l1_tx_hash: Some("0".repeat(64)),
                status: "pending".to_string(),
                error: None,
            });
        }
        
        let url = format!("{}/rpc/bridge", self.config.endpoint);
        
        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;
        
        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("L1 bridge request failed: {}", e))?;
        
        self.update_last_call();
        
        response
            .json::<L1BridgeResponse>()
            .await
            .map_err(|e| format!("Failed to parse bridge response: {}", e))
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_default() {
        let config = L1RpcConfig::default();
        assert_eq!(config.endpoint, L1_DEFAULT_ENDPOINT);
        assert!(!config.mock_mode);
    }
    
    #[test]
    fn test_config_mock() {
        let config = L1RpcConfig::mock();
        assert!(config.mock_mode);
    }
    
    #[test]
    fn test_rpc_client_mock() {
        let rpc = L1BlackBookRpc::mock();
        assert!(rpc.is_mock());
        assert!(!rpc.connected);
    }
}
