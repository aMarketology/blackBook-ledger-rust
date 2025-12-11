//! Signed Transaction Support for L1 ↔ L2 Integration
//! 
//! This module provides cryptographically signed transaction envelopes
//! for secure cross-layer communication and verified bet placement.

use serde::{Deserialize, Serialize};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::easteregg::GodMode;
use crate::l1_rpc_client::L1RpcClient;

/// Transaction type identifiers matching L1 protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SignedTxType {
    Transfer = 0,
    Bridge = 4,
    BetPlacement = 7,
    BetResolution = 8,
    MarketLaunch = 9,
    AddLiquidity = 10,
    RemoveLiquidity = 11,
}

impl SignedTxType {
    /// Get the numeric value of the transaction type
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Create from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SignedTxType::Transfer),
            4 => Some(SignedTxType::Bridge),
            7 => Some(SignedTxType::BetPlacement),
            8 => Some(SignedTxType::BetResolution),
            9 => Some(SignedTxType::MarketLaunch),
            10 => Some(SignedTxType::AddLiquidity),
            11 => Some(SignedTxType::RemoveLiquidity),
            _ => None,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            SignedTxType::Transfer => "Transfer",
            SignedTxType::Bridge => "Bridge",
            SignedTxType::BetPlacement => "BetPlacement",
            SignedTxType::BetResolution => "BetResolution",
            SignedTxType::MarketLaunch => "MarketLaunch",
            SignedTxType::AddLiquidity => "AddLiquidity",
            SignedTxType::RemoveLiquidity => "RemoveLiquidity",
        }
    }
}

/// Transaction payload variants with typed fields
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransactionPayload {
    Transfer {
        to: String,
        amount: f64,
    },
    Bridge {
        target_layer: String,       // "L1" or "L2"
        target_address: String,     // Address on target layer
        amount: f64,
    },
    BetPlacement {
        market_id: String,
        outcome: usize,
        amount: f64,
    },
    BetResolution {
        market_id: String,
        winning_outcome: usize,
    },
    MarketLaunch {
        event_id: String,
        liquidity: f64,
    },
    AddLiquidity {
        market_id: String,
        amount: f64,
    },
    RemoveLiquidity {
        market_id: String,
        shares: f64,
    },
}

impl TransactionPayload {
    /// Get the corresponding SignedTxType for this payload
    pub fn tx_type(&self) -> SignedTxType {
        match self {
            TransactionPayload::Transfer { .. } => SignedTxType::Transfer,
            TransactionPayload::Bridge { .. } => SignedTxType::Bridge,
            TransactionPayload::BetPlacement { .. } => SignedTxType::BetPlacement,
            TransactionPayload::BetResolution { .. } => SignedTxType::BetResolution,
            TransactionPayload::MarketLaunch { .. } => SignedTxType::MarketLaunch,
            TransactionPayload::AddLiquidity { .. } => SignedTxType::AddLiquidity,
            TransactionPayload::RemoveLiquidity { .. } => SignedTxType::RemoveLiquidity,
        }
    }

    /// Serialize payload to canonical bytes for signing
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

/// Default transaction expiry window (5 minutes)
pub const TX_EXPIRY_SECS: u64 = 300;

/// A cryptographically signed transaction envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    /// Sender's Ed25519 public key (64 hex chars)
    pub sender_pubkey: String,
    /// Sender's address (L1_<pubkey> format)
    pub sender_address: String,
    /// Transaction nonce (must be > last nonce for this sender)
    pub nonce: u64,
    /// Unix timestamp when transaction was created
    pub timestamp: u64,
    /// Transaction type identifier
    pub tx_type: SignedTxType,
    /// Transaction-specific payload
    pub payload: TransactionPayload,
    /// Ed25519 signature (128 hex chars)
    pub signature: String,
}

#[derive(Debug, Clone)]
pub enum SignedTxError {
    InvalidPubkey(String),
    InvalidSignature(String),
    SignatureMismatch,
    Expired,
    TypeMismatch,
    SerializationError(String),
    L1VerificationFailed(String),
}

impl std::fmt::Display for SignedTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignedTxError::InvalidPubkey(msg) => write!(f, "Invalid pubkey: {}", msg),
            SignedTxError::InvalidSignature(msg) => write!(f, "Invalid signature: {}", msg),
            SignedTxError::SignatureMismatch => write!(f, "Signature does not match"),
            SignedTxError::Expired => write!(f, "Transaction expired"),
            SignedTxError::TypeMismatch => write!(f, "tx_type does not match payload type"),
            SignedTxError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            SignedTxError::L1VerificationFailed(msg) => write!(f, "L1 verification failed: {}", msg),
        }
    }
}

impl std::error::Error for SignedTxError {}

impl SignedTransaction {
    /// Create a new signed transaction using a GodMode test account
    /// 
    /// This is primarily for testing. In production, the frontend would
    /// create and sign transactions using the user's private key.
    pub fn new(
        godmode: &GodMode,
        account_name: &str,
        nonce: u64,
        payload: TransactionPayload,
    ) -> Result<Self, SignedTxError> {
        let account = godmode.get_account(account_name)
            .ok_or_else(|| SignedTxError::InvalidPubkey(format!("Account '{}' not found", account_name)))?;

        let tx_type = payload.tx_type();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get pubkey as hex from verifying_key
        let sender_pubkey = hex::encode(account.verifying_key.as_bytes());

        // Create unsigned transaction for signing
        let mut tx = SignedTransaction {
            sender_pubkey,
            sender_address: account.address.clone(),
            nonce,
            timestamp,
            tx_type,
            payload,
            signature: String::new(),
        };

        // Sign the transaction
        let signing_bytes = tx.to_signing_bytes();
        let signature = account.sign_hex(&signing_bytes);

        tx.signature = signature;
        Ok(tx)
    }

    /// Create a signed transaction with an explicit timestamp (for testing expiry)
    pub fn new_with_timestamp(
        godmode: &GodMode,
        account_name: &str,
        nonce: u64,
        timestamp: u64,
        payload: TransactionPayload,
    ) -> Result<Self, SignedTxError> {
        let account = godmode.get_account(account_name)
            .ok_or_else(|| SignedTxError::InvalidPubkey(format!("Account '{}' not found", account_name)))?;

        let tx_type = payload.tx_type();

        // Get pubkey as hex from verifying_key
        let sender_pubkey = hex::encode(account.verifying_key.as_bytes());

        let mut tx = SignedTransaction {
            sender_pubkey,
            sender_address: account.address.clone(),
            nonce,
            timestamp,
            tx_type,
            payload,
            signature: String::new(),
        };

        let signing_bytes = tx.to_signing_bytes();
        let signature = account.sign_hex(&signing_bytes);

        tx.signature = signature;
        Ok(tx)
    }

    /// Generate the canonical bytes to sign
    /// 
    /// Format: SHA256(tx_type || nonce || timestamp || sender_pubkey || payload_json)
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        
        // Transaction type (1 byte)
        hasher.update([self.tx_type.as_u8()]);
        
        // Nonce (8 bytes, big-endian)
        hasher.update(self.nonce.to_be_bytes());
        
        // Timestamp (8 bytes, big-endian)
        hasher.update(self.timestamp.to_be_bytes());
        
        // Sender pubkey (raw bytes from hex)
        if let Ok(pubkey_bytes) = hex::decode(&self.sender_pubkey) {
            hasher.update(&pubkey_bytes);
        }
        
        // Payload JSON
        hasher.update(self.payload.to_bytes());
        
        hasher.finalize().to_vec()
    }

    /// Verify the signature locally using Ed25519
    pub fn verify(&self) -> Result<bool, SignedTxError> {
        // Check type matches payload
        if self.tx_type != self.payload.tx_type() {
            return Err(SignedTxError::TypeMismatch);
        }

        // Decode public key
        let pubkey_bytes = hex::decode(&self.sender_pubkey)
            .map_err(|e| SignedTxError::InvalidPubkey(e.to_string()))?;
        
        if pubkey_bytes.len() != 32 {
            return Err(SignedTxError::InvalidPubkey(
                format!("Expected 32 bytes, got {}", pubkey_bytes.len())
            ));
        }

        let pubkey_array: [u8; 32] = pubkey_bytes.try_into()
            .map_err(|_| SignedTxError::InvalidPubkey("Failed to convert to array".into()))?;
        
        let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|e| SignedTxError::InvalidPubkey(e.to_string()))?;

        // Decode signature
        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| SignedTxError::InvalidSignature(e.to_string()))?;
        
        if sig_bytes.len() != 64 {
            return Err(SignedTxError::InvalidSignature(
                format!("Expected 64 bytes, got {}", sig_bytes.len())
            ));
        }

        let sig_array: [u8; 64] = sig_bytes.try_into()
            .map_err(|_| SignedTxError::InvalidSignature("Failed to convert to array".into()))?;
        
        let signature = Signature::from_bytes(&sig_array);

        // Verify
        let signing_bytes = self.to_signing_bytes();
        match verifying_key.verify(&signing_bytes, &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify the signature via L1 RPC (for cross-layer validation)
    pub async fn verify_with_l1(&self, client: &L1RpcClient) -> Result<bool, SignedTxError> {
        // Check type matches payload
        if self.tx_type != self.payload.tx_type() {
            return Err(SignedTxError::TypeMismatch);
        }

        let signing_bytes = self.to_signing_bytes();
        
        client.verify_signature(&self.sender_pubkey, &signing_bytes, &self.signature)
            .await
            .map_err(|e| SignedTxError::L1VerificationFailed(e.to_string()))
    }

    /// Check if the transaction has expired
    /// 
    /// Transactions are valid for TX_EXPIRY_SECS (5 minutes) by default
    pub fn is_expired(&self) -> bool {
        self.is_expired_with_window(TX_EXPIRY_SECS)
    }

    /// Check if expired with a custom window (in seconds)
    pub fn is_expired_with_window(&self, window_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Check if timestamp is in the future (clock skew tolerance: 60 seconds)
        if self.timestamp > now + 60 {
            return true;
        }
        
        // Check if too old
        now > self.timestamp + window_secs
    }

    /// Validate the transaction completely (signature + expiry)
    pub fn validate(&self) -> Result<(), SignedTxError> {
        if self.is_expired() {
            return Err(SignedTxError::Expired);
        }

        match self.verify()? {
            true => Ok(()),
            false => Err(SignedTxError::SignatureMismatch),
        }
    }

    /// Validate with L1 verification
    pub async fn validate_with_l1(&self, client: &L1RpcClient) -> Result<(), SignedTxError> {
        if self.is_expired() {
            return Err(SignedTxError::Expired);
        }

        match self.verify_with_l1(client).await? {
            true => Ok(()),
            false => Err(SignedTxError::SignatureMismatch),
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_type_values() {
        assert_eq!(SignedTxType::Transfer.as_u8(), 0);
        assert_eq!(SignedTxType::Bridge.as_u8(), 4);
        assert_eq!(SignedTxType::BetPlacement.as_u8(), 7);
        assert_eq!(SignedTxType::BetResolution.as_u8(), 8);
        assert_eq!(SignedTxType::MarketLaunch.as_u8(), 9);
        assert_eq!(SignedTxType::AddLiquidity.as_u8(), 10);
        assert_eq!(SignedTxType::RemoveLiquidity.as_u8(), 11);
    }

    #[test]
    fn test_tx_type_from_u8() {
        assert_eq!(SignedTxType::from_u8(0), Some(SignedTxType::Transfer));
        assert_eq!(SignedTxType::from_u8(4), Some(SignedTxType::Bridge));
        assert_eq!(SignedTxType::from_u8(7), Some(SignedTxType::BetPlacement));
        assert_eq!(SignedTxType::from_u8(99), None);
    }

    #[test]
    fn test_payload_tx_type() {
        let transfer = TransactionPayload::Transfer {
            to: "L1_abc123".into(),
            amount: 100.0,
        };
        assert_eq!(transfer.tx_type(), SignedTxType::Transfer);

        let bet = TransactionPayload::BetPlacement {
            market_id: "market_1".into(),
            outcome: 0,
            amount: 50.0,
        };
        assert_eq!(bet.tx_type(), SignedTxType::BetPlacement);
    }

    #[test]
    fn test_create_and_verify_transfer() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        let alice = godmode.get_account("ALICE").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let tx = SignedTransaction::new(&godmode, "ALICE", 1, payload)
            .expect("Should create transaction");

        assert_eq!(tx.sender_pubkey, hex::encode(alice.verifying_key.as_bytes()));
        assert_eq!(tx.sender_address, alice.address);
        assert_eq!(tx.nonce, 1);
        assert_eq!(tx.tx_type, SignedTxType::Transfer);
        assert!(!tx.signature.is_empty());
        assert_eq!(tx.signature.len(), 128); // 64 bytes = 128 hex chars

        // Verify signature
        let valid = tx.verify().expect("Should verify");
        assert!(valid, "Signature should be valid");
    }

    #[test]
    fn test_create_and_verify_bet_placement() {
        let godmode = GodMode::new();
        
        let payload = TransactionPayload::BetPlacement {
            market_id: "market_superbowl_2025".into(),
            outcome: 0,
            amount: 50.0,
        };

        let tx = SignedTransaction::new(&godmode, "BOB", 5, payload)
            .expect("Should create transaction");

        assert_eq!(tx.tx_type, SignedTxType::BetPlacement);
        assert!(tx.verify().expect("Should verify"));
    }

    #[test]
    fn test_create_and_verify_bridge() {
        let godmode = GodMode::new();
        
        let payload = TransactionPayload::Bridge {
            target_layer: "L1".into(),
            target_address: "bb1_target_address".into(),
            amount: 1000.0,
        };

        let tx = SignedTransaction::new(&godmode, "CHARLIE", 1, payload)
            .expect("Should create transaction");

        assert_eq!(tx.tx_type, SignedTxType::Bridge);
        assert!(tx.verify().expect("Should verify"));
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let mut tx = SignedTransaction::new(&godmode, "ALICE", 1, payload)
            .expect("Should create transaction");

        // Tamper with the signature
        let mut sig_bytes = hex::decode(&tx.signature).unwrap();
        sig_bytes[0] ^= 0xFF; // Flip bits
        tx.signature = hex::encode(sig_bytes);

        // Should verify but return false
        let valid = tx.verify().expect("Should complete verification");
        assert!(!valid, "Tampered signature should be invalid");
    }

    #[test]
    fn test_transaction_not_expired() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let tx = SignedTransaction::new(&godmode, "ALICE", 1, payload)
            .expect("Should create transaction");

        assert!(!tx.is_expired(), "Fresh transaction should not be expired");
    }

    #[test]
    fn test_transaction_expired() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        // Create transaction with old timestamp (10 minutes ago)
        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() - 600;

        let tx = SignedTransaction::new_with_timestamp(
            &godmode, "ALICE", 1, old_timestamp, payload
        ).expect("Should create transaction");

        assert!(tx.is_expired(), "Old transaction should be expired");
    }

    #[test]
    fn test_validate_complete() {
        let godmode = GodMode::new();
        
        let payload = TransactionPayload::MarketLaunch {
            event_id: "superbowl_2025".into(),
            liquidity: 10000.0,
        };

        let tx = SignedTransaction::new(&godmode, "HOUSE", 1, payload)
            .expect("Should create transaction");

        tx.validate().expect("Transaction should be valid");
    }

    #[test]
    fn test_validate_expired_fails() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let old_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() - 600;

        let tx = SignedTransaction::new_with_timestamp(
            &godmode, "ALICE", 1, old_timestamp, payload
        ).expect("Should create transaction");

        let result = tx.validate();
        assert!(matches!(result, Err(SignedTxError::Expired)));
    }

    #[test]
    fn test_different_accounts() {
        let godmode = GodMode::new();
        
        // Use actual account names from godmode::TEST_ACCOUNT_NAMES
        let accounts = ["ALICE", "BOB", "CHARLIE", "DIANA", "ETHAN", "FIONA", "GEORGE", "HANNAH", "HOUSE"];
        
        for (i, account) in accounts.iter().enumerate() {
            let payload = TransactionPayload::Transfer {
                to: "L1_recipient".into(),
                amount: 100.0 * (i + 1) as f64,
            };

            let tx = SignedTransaction::new(&godmode, account, i as u64, payload)
                .expect(&format!("Should create tx for {}", account));

            assert!(tx.verify().expect("Should verify"), "Tx from {} should verify", account);
        }
    }

    #[test]
    fn test_payload_serialization() {
        let payload = TransactionPayload::BetPlacement {
            market_id: "test_market".into(),
            outcome: 1,
            amount: 75.5,
        };

        let bytes = payload.to_bytes();
        assert!(!bytes.is_empty());

        // Should be valid JSON
        let json_str = String::from_utf8(bytes.clone()).expect("Should be valid UTF-8");
        assert!(json_str.contains("bet_placement"));
        assert!(json_str.contains("test_market"));
    }

    #[test]
    fn test_signing_bytes_deterministic() {
        let godmode = GodMode::new();
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let tx1 = SignedTransaction::new_with_timestamp(&godmode, "ALICE", 1, 1000000, payload.clone())
            .expect("Should create tx1");
        
        let payload2 = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };
        let tx2 = SignedTransaction::new_with_timestamp(&godmode, "ALICE", 1, 1000000, payload2)
            .expect("Should create tx2");

        // Same inputs should produce same signing bytes
        assert_eq!(tx1.to_signing_bytes(), tx2.to_signing_bytes());
        // And same signature
        assert_eq!(tx1.signature, tx2.signature);
    }

    #[tokio::test]
    async fn test_verify_with_l1_mock() {
        let godmode = GodMode::new();
        let client = L1RpcClient::new(None); // Mock mode when no URL
        let bob = godmode.get_account("BOB").unwrap();
        
        let payload = TransactionPayload::Transfer {
            to: bob.address.clone(),
            amount: 100.0,
        };

        let tx = SignedTransaction::new(&godmode, "ALICE", 1, payload)
            .expect("Should create transaction");

        let valid = tx.verify_with_l1(&client).await
            .expect("Should verify with L1");
        
        assert!(valid, "Should verify via L1 mock");
    }

    #[tokio::test]
    async fn test_validate_with_l1_mock() {
        let godmode = GodMode::new();
        let client = L1RpcClient::new(None); // Mock mode when no URL
        
        let payload = TransactionPayload::BetPlacement {
            market_id: "test_market".into(),
            outcome: 0,
            amount: 50.0,
        };

        let tx = SignedTransaction::new(&godmode, "DIANA", 1, payload)
            .expect("Should create transaction");

        tx.validate_with_l1(&client).await
            .expect("Should validate with L1");
    }
}
