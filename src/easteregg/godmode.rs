/// BlackBook L2 Prediction Market - God Mode Module
/// 
/// Provides deterministic test accounts with Ed25519 keypairs,
/// admin operations, and debug utilities for development/testing.
/// 
/// All test accounts derive from a master seed, ensuring consistent
/// addresses across server restarts.

use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Master seed for deterministic key generation (NEVER use in production!)
pub const GODMODE_SEED: &[u8; 32] = b"BLACKBOOK_GODMODE_MASTER_SEED_01";

/// Test account names - Only Alice and Bob (real L1 accounts)
/// ORACLE is the L2-only admin for market resolution
pub const TEST_ACCOUNT_NAMES: [&str; 3] = [
    "ALICE", "BOB", "ORACLE"
];

/// Default initial balance for user test accounts (in BB tokens)
pub const DEFAULT_USER_BALANCE: f64 = 1000.0;

/// Default initial balance for ORACLE account (in BB tokens)
pub const DEFAULT_ORACLE_BALANCE: f64 = 10000.0;

/// BB token value in USD
pub const BB_TOKEN_VALUE_USD: f64 = 0.01;

// ============================================================================
// TEST ACCOUNT STRUCTURE
// ============================================================================

/// A test account with deterministic Ed25519 keypair
#[derive(Clone)]
pub struct TestAccount {
    pub name: String,
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub address: String,
    pub initial_balance: f64,
}

impl TestAccount {
    /// Create a new test account from a seed and name
    pub fn from_seed(seed: &[u8; 32], name: &str, initial_balance: f64) -> Self {
        // Derive a unique 32-byte key for this account: SHA256(seed || name)
        let mut hasher = Sha256::new();
        hasher.update(seed);
        hasher.update(name.as_bytes());
        let derived_key: [u8; 32] = hasher.finalize().into();
        
        // Create Ed25519 keypair from derived key
        let signing_key = SigningKey::from_bytes(&derived_key);
        let verifying_key = signing_key.verifying_key();
        
        // Address is the hex-encoded public key with L1_ prefix
        let address = format!("L1_{}", hex::encode(verifying_key.as_bytes()).to_uppercase());
        
        TestAccount {
            name: name.to_string(),
            signing_key,
            verifying_key,
            address,
            initial_balance,
        }
    }
    
    /// Sign a message with this account's private key
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }
    
    /// Sign a message and return hex-encoded signature
    pub fn sign_hex(&self, message: &[u8]) -> String {
        hex::encode(self.sign(message).to_bytes())
    }
    
    /// Verify a signature against this account's public key
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        self.verifying_key.verify(message, signature).is_ok()
    }
    
    /// Verify a hex-encoded signature
    pub fn verify_hex(&self, message: &[u8], signature_hex: &str) -> bool {
        match hex::decode(signature_hex) {
            Ok(sig_bytes) => {
                if sig_bytes.len() != 64 {
                    return false;
                }
                let mut sig_array = [0u8; 64];
                sig_array.copy_from_slice(&sig_bytes);
                match Signature::from_bytes(&sig_array) {
                    sig => self.verify(message, &sig),
                }
            }
            Err(_) => false,
        }
    }
}

impl std::fmt::Debug for TestAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestAccount")
            .field("name", &self.name)
            .field("address", &self.address)
            .field("initial_balance", &self.initial_balance)
            .finish()
    }
}

// ============================================================================
// SERIALIZABLE ACCOUNT INFO (for API responses)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub name: String,
    pub address: String,
    pub public_key_hex: String,
    pub initial_balance: f64,
}

impl From<&TestAccount> for AccountInfo {
    fn from(account: &TestAccount) -> Self {
        AccountInfo {
            name: account.name.clone(),
            address: account.address.clone(),
            public_key_hex: hex::encode(account.verifying_key.as_bytes()),
            initial_balance: account.initial_balance,
        }
    }
}

// ============================================================================
// GOD MODE CONTROLLER
// ============================================================================

/// God Mode controller for admin operations
pub struct GodMode {
    /// Whether god mode is enabled
    pub enabled: bool,
    
    /// The admin account (has special privileges)
    pub admin: TestAccount,
    
    /// All test accounts including admin
    pub test_accounts: HashMap<String, TestAccount>,
    
    /// Map of address -> account name for reverse lookup
    pub address_to_name: HashMap<String, String>,
}

impl GodMode {
    /// Create a new GodMode instance with default test accounts
    pub fn new() -> Self {
        let mut test_accounts = HashMap::new();
        let mut address_to_name = HashMap::new();
        
        // Create all test accounts from the master seed
        for name in TEST_ACCOUNT_NAMES.iter() {
            let balance = if *name == "ORACLE" {
                DEFAULT_ORACLE_BALANCE
            } else {
                DEFAULT_USER_BALANCE
            };
            
            let account = TestAccount::from_seed(GODMODE_SEED, name, balance);
            address_to_name.insert(account.address.clone(), name.to_string());
            test_accounts.insert(name.to_string(), account);
        }
        
        // Create admin account (separate from test accounts)
        let admin = TestAccount::from_seed(GODMODE_SEED, "GODMODE_ADMIN", 0.0);
        
        // Check environment for enabled flag
        let enabled = std::env::var("GODMODE_ENABLED")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true); // Default to enabled for development
        
        GodMode {
            enabled,
            admin,
            test_accounts,
            address_to_name,
        }
    }
    
    /// Create from environment (checks GODMODE_SEED env var)
    pub fn from_env() -> Self {
        // Could customize seed from env, but for now use default
        Self::new()
    }
    
    /// Get a test account by name (case-insensitive)
    pub fn get_account(&self, name: &str) -> Option<&TestAccount> {
        self.test_accounts.get(&name.to_uppercase())
    }
    
    /// Get a test account by address
    pub fn get_account_by_address(&self, address: &str) -> Option<&TestAccount> {
        self.address_to_name.get(address)
            .and_then(|name| self.test_accounts.get(name))
    }
    
    /// Get all test accounts as AccountInfo (for API)
    pub fn list_accounts(&self) -> Vec<AccountInfo> {
        TEST_ACCOUNT_NAMES.iter()
            .filter_map(|name| self.test_accounts.get(*name))
            .map(AccountInfo::from)
            .collect()
    }
    
    /// Resolve an identifier (name or address) to an address
    pub fn resolve_address(&self, identifier: &str) -> Option<String> {
        // Check if it's already an address
        if identifier.starts_with("L1_") {
            if self.address_to_name.contains_key(identifier) {
                return Some(identifier.to_string());
            }
        }
        
        // Try to find by name
        self.get_account(identifier)
            .map(|acc| acc.address.clone())
    }
    
    /// Verify that a signature was made by the admin
    pub fn verify_admin_signature(&self, message: &[u8], signature_hex: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.admin.verify_hex(message, signature_hex)
    }
    
    /// Verify that a signature was made by a specific address
    pub fn verify_signature(&self, address: &str, message: &[u8], signature_hex: &str) -> bool {
        // First try test accounts
        if let Some(account) = self.get_account_by_address(address) {
            return account.verify_hex(message, signature_hex);
        }
        
        // Check if it's the admin
        if address == self.admin.address {
            return self.admin.verify_hex(message, signature_hex);
        }
        
        false
    }
    
    /// Get initial balances for all test accounts (for ledger initialization)
    pub fn get_initial_balances(&self) -> HashMap<String, f64> {
        self.test_accounts.iter()
            .map(|(_, acc)| (acc.address.clone(), acc.initial_balance))
            .collect()
    }
    
    /// Get account name -> address mapping (for ledger initialization)
    pub fn get_account_mapping(&self) -> HashMap<String, String> {
        self.test_accounts.iter()
            .map(|(name, acc)| (name.clone(), acc.address.clone()))
            .collect()
    }
    
    // ========================================================================
    // ADMIN OPERATIONS (require admin signature in production)
    // ========================================================================
    
    /// Mint tokens to an address (god mode operation)
    pub fn mint(&self, address: &str, amount: f64) -> Result<MintOperation, GodModeError> {
        if !self.enabled {
            return Err(GodModeError::Disabled);
        }
        if amount <= 0.0 {
            return Err(GodModeError::InvalidAmount(amount));
        }
        
        Ok(MintOperation {
            to_address: address.to_string(),
            amount,
            operation: "mint".to_string(),
        })
    }
    
    /// Burn tokens from an address (god mode operation)
    pub fn burn(&self, address: &str, amount: f64) -> Result<BurnOperation, GodModeError> {
        if !self.enabled {
            return Err(GodModeError::Disabled);
        }
        if amount <= 0.0 {
            return Err(GodModeError::InvalidAmount(amount));
        }
        
        Ok(BurnOperation {
            from_address: address.to_string(),
            amount,
            operation: "burn".to_string(),
        })
    }
    
    /// Set exact balance for an address (god mode operation)
    pub fn set_balance(&self, address: &str, balance: f64) -> Result<SetBalanceOperation, GodModeError> {
        if !self.enabled {
            return Err(GodModeError::Disabled);
        }
        if balance < 0.0 {
            return Err(GodModeError::InvalidAmount(balance));
        }
        
        Ok(SetBalanceOperation {
            address: address.to_string(),
            new_balance: balance,
            operation: "set_balance".to_string(),
        })
    }
    
    /// Airdrop tokens to multiple addresses
    pub fn airdrop(&self, addresses: &[&str], amount: f64) -> Result<AirdropOperation, GodModeError> {
        if !self.enabled {
            return Err(GodModeError::Disabled);
        }
        if amount <= 0.0 {
            return Err(GodModeError::InvalidAmount(amount));
        }
        
        Ok(AirdropOperation {
            addresses: addresses.iter().map(|s| s.to_string()).collect(),
            amount_each: amount,
            total_amount: amount * addresses.len() as f64,
            operation: "airdrop".to_string(),
        })
    }
    
    /// Sign a message as admin (for testing/development)
    pub fn admin_sign(&self, message: &[u8]) -> String {
        self.admin.sign_hex(message)
    }
    
    /// Get admin address
    pub fn admin_address(&self) -> String {
        self.admin.address.clone()
    }
    
    /// Get admin public key hex
    pub fn admin_public_key(&self) -> String {
        hex::encode(self.admin.verifying_key.as_bytes())
    }
}

impl Default for GodMode {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// OPERATION RESULTS (for ledger to apply)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintOperation {
    pub to_address: String,
    pub amount: f64,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnOperation {
    pub from_address: String,
    pub amount: f64,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBalanceOperation {
    pub address: String,
    pub new_balance: f64,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirdropOperation {
    pub addresses: Vec<String>,
    pub amount_each: f64,
    pub total_amount: f64,
    pub operation: String,
}

// ============================================================================
// ERROR TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GodModeError {
    Disabled,
    InvalidAmount(f64),
    AccountNotFound(String),
    InvalidSignature,
    Unauthorized,
}

impl std::fmt::Display for GodModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GodModeError::Disabled => write!(f, "God mode is disabled"),
            GodModeError::InvalidAmount(amt) => write!(f, "Invalid amount: {}", amt),
            GodModeError::AccountNotFound(name) => write!(f, "Account not found: {}", name),
            GodModeError::InvalidSignature => write!(f, "Invalid signature"),
            GodModeError::Unauthorized => write!(f, "Unauthorized operation"),
        }
    }
}

impl std::error::Error for GodModeError {}

// ============================================================================
// SIGNED TRANSACTION STRUCTURE (for future L1 bridge)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub payload: String,  // JSON-encoded payload
    pub sender_address: String,
    pub signature: String,  // Hex-encoded Ed25519 signature
    pub timestamp: u64,
    pub nonce: u64,
}

impl SignedMessage {
    /// Create a new signed message
    pub fn new(payload: serde_json::Value, account: &TestAccount, nonce: u64) -> Self {
        let payload_str = serde_json::to_string(&payload).unwrap_or_default();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Message to sign: payload || timestamp || nonce
        let sign_data = format!("{}:{}:{}", payload_str, timestamp, nonce);
        let signature = account.sign_hex(sign_data.as_bytes());
        
        SignedMessage {
            payload: payload_str,
            sender_address: account.address.clone(),
            signature,
            timestamp,
            nonce,
        }
    }
    
    /// Verify this signed message against a GodMode instance
    pub fn verify(&self, godmode: &GodMode) -> bool {
        let sign_data = format!("{}:{}:{}", self.payload, self.timestamp, self.nonce);
        godmode.verify_signature(&self.sender_address, sign_data.as_bytes(), &self.signature)
    }
    
    /// Check if the message has expired (default: 5 minutes)
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.timestamp + 300 // 5 minute window
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_deterministic_account_generation() {
        // Create two GodMode instances - they should have identical accounts
        let gm1 = GodMode::new();
        let gm2 = GodMode::new();
        
        for name in TEST_ACCOUNT_NAMES.iter() {
            let acc1 = gm1.get_account(name).unwrap();
            let acc2 = gm2.get_account(name).unwrap();
            
            assert_eq!(acc1.address, acc2.address, "Addresses should be deterministic for {}", name);
            assert_eq!(
                acc1.verifying_key.as_bytes(),
                acc2.verifying_key.as_bytes(),
                "Public keys should be deterministic for {}", name
            );
        }
    }
    
    #[test]
    fn test_account_addresses_are_unique() {
        let gm = GodMode::new();
        let mut addresses = std::collections::HashSet::new();
        
        for name in TEST_ACCOUNT_NAMES.iter() {
            let acc = gm.get_account(name).unwrap();
            assert!(
                addresses.insert(acc.address.clone()),
                "Address collision for {}", name
            );
        }
    }
    
    #[test]
    fn test_sign_and_verify() {
        let gm = GodMode::new();
        let alice = gm.get_account("ALICE").unwrap();
        
        let message = b"Hello, BlackBook!";
        let signature = alice.sign_hex(message);
        
        // Alice's signature should verify with Alice's key
        assert!(alice.verify_hex(message, &signature));
        
        // Alice's signature should NOT verify with Bob's key
        let bob = gm.get_account("BOB").unwrap();
        assert!(!bob.verify_hex(message, &signature));
        
        // Wrong message should not verify
        assert!(!alice.verify_hex(b"Wrong message", &signature));
    }
    
    #[test]
    fn test_admin_signature() {
        let gm = GodMode::new();
        let message = b"Admin operation: mint 1000 BB";
        
        let signature = gm.admin_sign(message);
        assert!(gm.verify_admin_signature(message, &signature));
        
        // Wrong message should fail
        assert!(!gm.verify_admin_signature(b"Different message", &signature));
    }
    
    #[test]
    fn test_resolve_address() {
        let gm = GodMode::new();
        
        // Resolve by name (case-insensitive)
        let alice_addr = gm.resolve_address("alice").unwrap();
        let alice_addr2 = gm.resolve_address("ALICE").unwrap();
        assert_eq!(alice_addr, alice_addr2);
        
        // Resolve by address
        let alice_addr3 = gm.resolve_address(&alice_addr).unwrap();
        assert_eq!(alice_addr, alice_addr3);
        
        // Unknown should return None
        assert!(gm.resolve_address("UNKNOWN").is_none());
    }
    
    #[test]
    fn test_signed_message() {
        let gm = GodMode::new();
        let alice = gm.get_account("ALICE").unwrap();
        
        let payload = serde_json::json!({
            "action": "place_bet",
            "market_id": "market_123",
            "amount": 100.0
        });
        
        let signed_msg = SignedMessage::new(payload, alice, 1);
        
        // Message should verify
        assert!(signed_msg.verify(&gm));
        
        // Message should not be expired (just created)
        assert!(!signed_msg.is_expired());
    }
    
    #[test]
    fn test_initial_balances() {
        let gm = GodMode::new();
        let balances = gm.get_initial_balances();
        
        // Should have all test accounts
        assert_eq!(balances.len(), TEST_ACCOUNT_NAMES.len());
        
        // ORACLE should have 10,000 BB
        let oracle = gm.get_account("ORACLE").unwrap();
        assert_eq!(balances[&oracle.address], DEFAULT_ORACLE_BALANCE);
        
        // ALICE should have 1,000 BB
        let alice = gm.get_account("ALICE").unwrap();
        assert_eq!(balances[&alice.address], DEFAULT_USER_BALANCE);
    }
    
    #[test]
    fn test_list_accounts() {
        let gm = GodMode::new();
        let accounts = gm.list_accounts();
        
        assert_eq!(accounts.len(), TEST_ACCOUNT_NAMES.len());
        
        // Verify order matches TEST_ACCOUNT_NAMES
        for (i, name) in TEST_ACCOUNT_NAMES.iter().enumerate() {
            assert_eq!(accounts[i].name, *name);
        }
    }
    
    #[test]
    fn test_mint_operation() {
        let gm = GodMode::new();
        let alice = gm.get_account("ALICE").unwrap();
        
        let op = gm.mint(&alice.address, 500.0).unwrap();
        assert_eq!(op.amount, 500.0);
        assert_eq!(op.to_address, alice.address);
        
        // Invalid amount should fail
        assert!(gm.mint(&alice.address, -100.0).is_err());
        assert!(gm.mint(&alice.address, 0.0).is_err());
    }
    
    #[test]
    fn test_airdrop_operation() {
        let gm = GodMode::new();
        
        let addresses: Vec<&str> = vec!["addr1", "addr2", "addr3"];
        let op = gm.airdrop(&addresses, 100.0).unwrap();
        
        assert_eq!(op.addresses.len(), 3);
        assert_eq!(op.amount_each, 100.0);
        assert_eq!(op.total_amount, 300.0);
    }
}
