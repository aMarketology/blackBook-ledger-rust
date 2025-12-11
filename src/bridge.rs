//! Bridge Module for L1 ↔ L2 Token Movement
//! 
//! This module handles cross-layer token bridging between L1 (consensus) and L2 (prediction market).
//! 
//! Bridge Flow:
//! 1. L2→L1: User initiates on L2, tokens locked, L1 confirms and releases
//! 2. L1→L2: User initiates on L1, L1 confirms, L2 receives callback and mints

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::rpc::{SignedTransaction, TransactionPayload, SignedTxError};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default bridge confirmation timeout (5 minutes)
pub const BRIDGE_TIMEOUT_SECS: u64 = 300;

/// Minimum bridge amount
pub const MIN_BRIDGE_AMOUNT: f64 = 0.01;

/// Maximum bridge amount (per transaction)
pub const MAX_BRIDGE_AMOUNT: f64 = 1_000_000.0;

// ============================================================================
// BRIDGE STATUS
// ============================================================================

/// Status of a bridge operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BridgeStatus {
    /// Bridge initiated, waiting for confirmation
    Pending,
    /// L1 has confirmed the transaction
    Confirmed,
    /// Bridge fully completed (tokens minted/released on target layer)
    Completed,
    /// Bridge failed (timeout, insufficient funds, etc.)
    Failed,
    /// Bridge was cancelled by user
    Cancelled,
}

impl BridgeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BridgeStatus::Pending => "pending",
            BridgeStatus::Confirmed => "confirmed",
            BridgeStatus::Completed => "completed",
            BridgeStatus::Failed => "failed",
            BridgeStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, BridgeStatus::Completed | BridgeStatus::Failed | BridgeStatus::Cancelled)
    }
}

// ============================================================================
// BRIDGE DIRECTION
// ============================================================================

/// Direction of the bridge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeDirection {
    /// L1 → L2 (deposit to L2)
    L1ToL2,
    /// L2 → L1 (withdraw from L2)
    L2ToL1,
}

impl BridgeDirection {
    pub fn from_layers(from: &str, to: &str) -> Option<Self> {
        match (from.to_uppercase().as_str(), to.to_uppercase().as_str()) {
            ("L1", "L2") => Some(BridgeDirection::L1ToL2),
            ("L2", "L1") => Some(BridgeDirection::L2ToL1),
            _ => None,
        }
    }

    pub fn from_layer(&self) -> &'static str {
        match self {
            BridgeDirection::L1ToL2 => "L1",
            BridgeDirection::L2ToL1 => "L2",
        }
    }

    pub fn to_layer(&self) -> &'static str {
        match self {
            BridgeDirection::L1ToL2 => "L2",
            BridgeDirection::L2ToL1 => "L1",
        }
    }
}

// ============================================================================
// PENDING BRIDGE
// ============================================================================

/// A pending bridge operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBridge {
    /// Unique bridge identifier
    pub bridge_id: String,
    /// Direction of the bridge
    pub direction: BridgeDirection,
    /// Source layer ("L1" or "L2")
    pub from_layer: String,
    /// Target layer ("L1" or "L2")
    pub to_layer: String,
    /// Source address (on from_layer)
    pub from_address: String,
    /// Target address (on to_layer)
    pub to_address: String,
    /// Amount being bridged
    pub amount: f64,
    /// Current status
    pub status: BridgeStatus,
    /// Unix timestamp when bridge was created
    pub created_at: u64,
    /// Unix timestamp when bridge was completed (if completed)
    pub completed_at: Option<u64>,
    /// L1 transaction hash (after L1 confirms)
    pub l1_tx_hash: Option<String>,
    /// L1 slot number (after L1 confirms)
    pub l1_slot: Option<u64>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Original signed transaction (for verification)
    pub signed_tx_signature: Option<String>,
}

impl PendingBridge {
    /// Create a new pending bridge
    pub fn new(
        direction: BridgeDirection,
        from_address: String,
        to_address: String,
        amount: f64,
    ) -> Self {
        let bridge_id = format!("bridge_{}_{}", 
            Uuid::new_v4().to_string().replace("-", "")[..12].to_string(),
            now_timestamp()
        );

        PendingBridge {
            bridge_id,
            direction,
            from_layer: direction.from_layer().to_string(),
            to_layer: direction.to_layer().to_string(),
            from_address,
            to_address,
            amount,
            status: BridgeStatus::Pending,
            created_at: now_timestamp(),
            completed_at: None,
            l1_tx_hash: None,
            l1_slot: None,
            error: None,
            signed_tx_signature: None,
        }
    }

    /// Check if the bridge has timed out
    pub fn is_expired(&self) -> bool {
        if self.status.is_terminal() {
            return false;
        }
        let now = now_timestamp();
        now > self.created_at + BRIDGE_TIMEOUT_SECS
    }

    /// Mark as confirmed (L1 has confirmed)
    pub fn confirm(&mut self, l1_tx_hash: String, l1_slot: u64) {
        self.status = BridgeStatus::Confirmed;
        self.l1_tx_hash = Some(l1_tx_hash);
        self.l1_slot = Some(l1_slot);
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.status = BridgeStatus::Completed;
        self.completed_at = Some(now_timestamp());
    }

    /// Mark as failed
    pub fn fail(&mut self, error: String) {
        self.status = BridgeStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(now_timestamp());
    }

    /// Get duration since creation (in seconds)
    pub fn age_secs(&self) -> u64 {
        now_timestamp().saturating_sub(self.created_at)
    }
}

// ============================================================================
// BRIDGE ERROR
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeError {
    InvalidAmount(String),
    InvalidAddress(String),
    InvalidDirection(String),
    InsufficientBalance { available: f64, requested: f64 },
    BridgeNotFound(String),
    BridgeAlreadyCompleted(String),
    BridgeExpired(String),
    SignatureVerificationFailed(String),
    L1CommunicationError(String),
    InternalError(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            BridgeError::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            BridgeError::InvalidDirection(msg) => write!(f, "Invalid direction: {}", msg),
            BridgeError::InsufficientBalance { available, requested } => {
                write!(f, "Insufficient balance: have {}, need {}", available, requested)
            }
            BridgeError::BridgeNotFound(id) => write!(f, "Bridge not found: {}", id),
            BridgeError::BridgeAlreadyCompleted(id) => write!(f, "Bridge already completed: {}", id),
            BridgeError::BridgeExpired(id) => write!(f, "Bridge expired: {}", id),
            BridgeError::SignatureVerificationFailed(msg) => write!(f, "Signature verification failed: {}", msg),
            BridgeError::L1CommunicationError(msg) => write!(f, "L1 communication error: {}", msg),
            BridgeError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<SignedTxError> for BridgeError {
    fn from(err: SignedTxError) -> Self {
        BridgeError::SignatureVerificationFailed(err.to_string())
    }
}

// ============================================================================
// BRIDGE REQUEST/RESPONSE TYPES
// ============================================================================

/// Request to initiate a bridge (L2→L1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    /// Signed transaction with Bridge payload
    pub signed_tx: SignedTransaction,
}

/// Response after initiating a bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub success: bool,
    pub bridge_id: String,
    pub amount: f64,
    pub from_layer: String,
    pub to_layer: String,
    pub from_address: String,
    pub to_address: String,
    pub status: BridgeStatus,
    pub estimated_completion_secs: u64,
    pub error: Option<String>,
}

/// Request from L1 to complete a bridge (L1→L2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCompleteRequest {
    pub bridge_id: String,
    pub from_address: String,
    pub to_address: String,
    pub amount: f64,
    pub l1_tx_hash: String,
    pub l1_slot: u64,
}

/// Response after completing a bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCompleteResponse {
    pub success: bool,
    pub bridge_id: String,
    pub l2_balance: Option<f64>,
    pub error: Option<String>,
}

/// Bridge status query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStatusResponse {
    pub found: bool,
    pub bridge: Option<PendingBridge>,
    pub error: Option<String>,
}

// ============================================================================
// BRIDGE MANAGER
// ============================================================================

/// Manages all pending and completed bridges
#[derive(Debug)]
pub struct BridgeManager {
    /// All bridges indexed by bridge_id
    bridges: Arc<Mutex<HashMap<String, PendingBridge>>>,
    /// Bridges indexed by from_address for quick lookup
    bridges_by_address: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl BridgeManager {
    /// Create a new BridgeManager
    pub fn new() -> Self {
        BridgeManager {
            bridges: Arc::new(Mutex::new(HashMap::new())),
            bridges_by_address: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initiate a new bridge from a signed transaction
    /// 
    /// This is called when a user wants to bridge L2→L1
    /// The caller is responsible for:
    /// 1. Verifying the signature
    /// 2. Locking tokens on L2
    pub fn initiate(
        &self,
        signed_tx: &SignedTransaction,
    ) -> Result<PendingBridge, BridgeError> {
        // Extract bridge payload
        let (target_layer, target_address, amount) = match &signed_tx.payload {
            TransactionPayload::Bridge { target_layer, target_address, amount } => {
                (target_layer.clone(), target_address.clone(), *amount)
            }
            _ => {
                return Err(BridgeError::InvalidDirection(
                    "Transaction payload is not a Bridge type".into()
                ));
            }
        };

        // Validate amount
        if amount < MIN_BRIDGE_AMOUNT {
            return Err(BridgeError::InvalidAmount(
                format!("Amount {} is below minimum {}", amount, MIN_BRIDGE_AMOUNT)
            ));
        }
        if amount > MAX_BRIDGE_AMOUNT {
            return Err(BridgeError::InvalidAmount(
                format!("Amount {} exceeds maximum {}", amount, MAX_BRIDGE_AMOUNT)
            ));
        }

        // Determine direction (we're on L2, so if target is L1, direction is L2→L1)
        let direction = if target_layer.to_uppercase() == "L1" {
            BridgeDirection::L2ToL1
        } else if target_layer.to_uppercase() == "L2" {
            // L2→L2 doesn't make sense from L2
            return Err(BridgeError::InvalidDirection(
                "Cannot bridge L2→L2 from L2".into()
            ));
        } else {
            return Err(BridgeError::InvalidDirection(
                format!("Unknown target layer: {}", target_layer)
            ));
        };

        // Create pending bridge
        let mut bridge = PendingBridge::new(
            direction,
            signed_tx.sender_address.clone(),
            target_address,
            amount,
        );
        bridge.signed_tx_signature = Some(signed_tx.signature.clone());

        // Store bridge
        let bridge_id = bridge.bridge_id.clone();
        {
            let mut bridges = self.bridges.lock().unwrap();
            bridges.insert(bridge_id.clone(), bridge.clone());
        }
        {
            let mut by_addr = self.bridges_by_address.lock().unwrap();
            by_addr
                .entry(signed_tx.sender_address.clone())
                .or_insert_with(Vec::new)
                .push(bridge_id);
        }

        Ok(bridge)
    }

    /// Complete a bridge from L1→L2
    /// 
    /// This is called when L1 notifies us that a bridge is confirmed
    /// The caller is responsible for minting tokens to the target address
    pub fn complete_from_l1(
        &self,
        request: &BridgeCompleteRequest,
    ) -> Result<PendingBridge, BridgeError> {
        // Check if bridge already exists (for retries)
        {
            let bridges = self.bridges.lock().unwrap();
            if let Some(existing) = bridges.get(&request.bridge_id) {
                if existing.status == BridgeStatus::Completed {
                    return Err(BridgeError::BridgeAlreadyCompleted(request.bridge_id.clone()));
                }
            }
        }

        // Create or update bridge record
        let mut bridge = PendingBridge::new(
            BridgeDirection::L1ToL2,
            request.from_address.clone(),
            request.to_address.clone(),
            request.amount,
        );
        bridge.bridge_id = request.bridge_id.clone();
        bridge.confirm(request.l1_tx_hash.clone(), request.l1_slot);
        bridge.complete();

        // Store
        {
            let mut bridges = self.bridges.lock().unwrap();
            bridges.insert(request.bridge_id.clone(), bridge.clone());
        }
        {
            let mut by_addr = self.bridges_by_address.lock().unwrap();
            by_addr
                .entry(request.to_address.clone())
                .or_insert_with(Vec::new)
                .push(request.bridge_id.clone());
        }

        Ok(bridge)
    }

    /// Get bridge status by ID
    pub fn get_status(&self, bridge_id: &str) -> Option<PendingBridge> {
        let bridges = self.bridges.lock().unwrap();
        bridges.get(bridge_id).cloned()
    }

    /// List all pending (non-terminal) bridges
    pub fn list_pending(&self) -> Vec<PendingBridge> {
        let bridges = self.bridges.lock().unwrap();
        bridges
            .values()
            .filter(|b| !b.status.is_terminal())
            .cloned()
            .collect()
    }

    /// List all bridges for an address
    pub fn list_by_address(&self, address: &str) -> Vec<PendingBridge> {
        let by_addr = self.bridges_by_address.lock().unwrap();
        let bridges = self.bridges.lock().unwrap();
        
        by_addr
            .get(address)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| bridges.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Mark a bridge as failed
    pub fn fail_bridge(&self, bridge_id: &str, error: String) -> Result<(), BridgeError> {
        let mut bridges = self.bridges.lock().unwrap();
        let bridge = bridges.get_mut(bridge_id)
            .ok_or_else(|| BridgeError::BridgeNotFound(bridge_id.to_string()))?;
        
        if bridge.status.is_terminal() {
            return Err(BridgeError::BridgeAlreadyCompleted(bridge_id.to_string()));
        }
        
        bridge.fail(error);
        Ok(())
    }

    /// Mark a pending bridge as confirmed (L1 confirmed, waiting for completion)
    pub fn confirm_bridge(&self, bridge_id: &str, l1_tx_hash: String, l1_slot: u64) -> Result<(), BridgeError> {
        let mut bridges = self.bridges.lock().unwrap();
        let bridge = bridges.get_mut(bridge_id)
            .ok_or_else(|| BridgeError::BridgeNotFound(bridge_id.to_string()))?;
        
        if bridge.status.is_terminal() {
            return Err(BridgeError::BridgeAlreadyCompleted(bridge_id.to_string()));
        }
        
        bridge.confirm(l1_tx_hash, l1_slot);
        Ok(())
    }

    /// Complete a pending bridge (after tokens are minted/released)
    pub fn complete_bridge(&self, bridge_id: &str) -> Result<PendingBridge, BridgeError> {
        let mut bridges = self.bridges.lock().unwrap();
        let bridge = bridges.get_mut(bridge_id)
            .ok_or_else(|| BridgeError::BridgeNotFound(bridge_id.to_string()))?;
        
        if bridge.status == BridgeStatus::Completed {
            return Err(BridgeError::BridgeAlreadyCompleted(bridge_id.to_string()));
        }
        
        bridge.complete();
        Ok(bridge.clone())
    }

    /// Clean up expired bridges (mark them as failed)
    pub fn cleanup_expired(&self) -> Vec<String> {
        let mut bridges = self.bridges.lock().unwrap();
        let mut expired = Vec::new();
        
        for (id, bridge) in bridges.iter_mut() {
            if bridge.is_expired() {
                bridge.fail("Bridge timed out".into());
                expired.push(id.clone());
            }
        }
        
        expired
    }

    /// Get statistics
    pub fn stats(&self) -> BridgeStats {
        let bridges = self.bridges.lock().unwrap();
        
        let mut stats = BridgeStats::default();
        for bridge in bridges.values() {
            stats.total += 1;
            match bridge.status {
                BridgeStatus::Pending => stats.pending += 1,
                BridgeStatus::Confirmed => stats.confirmed += 1,
                BridgeStatus::Completed => {
                    stats.completed += 1;
                    stats.total_volume += bridge.amount;
                }
                BridgeStatus::Failed => stats.failed += 1,
                BridgeStatus::Cancelled => stats.cancelled += 1,
            }
            match bridge.direction {
                BridgeDirection::L1ToL2 => stats.l1_to_l2 += 1,
                BridgeDirection::L2ToL1 => stats.l2_to_l1 += 1,
            }
        }
        
        stats
    }
}

impl Default for BridgeManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BRIDGE STATS
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BridgeStats {
    pub total: usize,
    pub pending: usize,
    pub confirmed: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub l1_to_l2: usize,
    pub l2_to_l1: usize,
    pub total_volume: f64,
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easteregg::GodMode;

    fn create_bridge_tx(godmode: &GodMode, account: &str, target: &str, amount: f64) -> SignedTransaction {
        let payload = TransactionPayload::Bridge {
            target_layer: "L1".into(),
            target_address: target.into(),
            amount,
        };
        SignedTransaction::new(godmode, account, 1, payload).unwrap()
    }

    #[test]
    fn test_bridge_status_is_terminal() {
        assert!(!BridgeStatus::Pending.is_terminal());
        assert!(!BridgeStatus::Confirmed.is_terminal());
        assert!(BridgeStatus::Completed.is_terminal());
        assert!(BridgeStatus::Failed.is_terminal());
        assert!(BridgeStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_bridge_direction() {
        assert_eq!(
            BridgeDirection::from_layers("L1", "L2"),
            Some(BridgeDirection::L1ToL2)
        );
        assert_eq!(
            BridgeDirection::from_layers("L2", "L1"),
            Some(BridgeDirection::L2ToL1)
        );
        assert_eq!(BridgeDirection::from_layers("L1", "L1"), None);
        assert_eq!(BridgeDirection::from_layers("invalid", "L2"), None);
    }

    #[test]
    fn test_pending_bridge_creation() {
        let bridge = PendingBridge::new(
            BridgeDirection::L2ToL1,
            "L1_sender".into(),
            "bb1_target".into(),
            100.0,
        );

        assert!(bridge.bridge_id.starts_with("bridge_"));
        assert_eq!(bridge.from_layer, "L2");
        assert_eq!(bridge.to_layer, "L1");
        assert_eq!(bridge.amount, 100.0);
        assert_eq!(bridge.status, BridgeStatus::Pending);
        assert!(!bridge.is_expired());
    }

    #[test]
    fn test_bridge_lifecycle() {
        let mut bridge = PendingBridge::new(
            BridgeDirection::L2ToL1,
            "L1_sender".into(),
            "bb1_target".into(),
            100.0,
        );

        // Initial state
        assert_eq!(bridge.status, BridgeStatus::Pending);
        assert!(bridge.l1_tx_hash.is_none());

        // Confirm
        bridge.confirm("0xabc123".into(), 12345);
        assert_eq!(bridge.status, BridgeStatus::Confirmed);
        assert_eq!(bridge.l1_tx_hash, Some("0xabc123".into()));
        assert_eq!(bridge.l1_slot, Some(12345));

        // Complete
        bridge.complete();
        assert_eq!(bridge.status, BridgeStatus::Completed);
        assert!(bridge.completed_at.is_some());
    }

    #[test]
    fn test_bridge_manager_initiate() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        let tx = create_bridge_tx(&godmode, "ALICE", "bb1_target", 100.0);
        let bridge = manager.initiate(&tx).expect("Should initiate bridge");

        assert_eq!(bridge.status, BridgeStatus::Pending);
        assert_eq!(bridge.amount, 100.0);
        assert_eq!(bridge.direction, BridgeDirection::L2ToL1);
        assert_eq!(bridge.from_address, tx.sender_address);
    }

    #[test]
    fn test_bridge_manager_get_status() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        let tx = create_bridge_tx(&godmode, "BOB", "bb1_target", 50.0);
        let bridge = manager.initiate(&tx).unwrap();
        let bridge_id = bridge.bridge_id.clone();

        let status = manager.get_status(&bridge_id);
        assert!(status.is_some());
        assert_eq!(status.unwrap().amount, 50.0);

        let not_found = manager.get_status("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_bridge_manager_complete_from_l1() {
        let manager = BridgeManager::new();
        
        let request = BridgeCompleteRequest {
            bridge_id: "bridge_l1_12345".into(),
            from_address: "bb1_sender".into(),
            to_address: "L1_receiver".into(),
            amount: 200.0,
            l1_tx_hash: "0xdef456".into(),
            l1_slot: 99999,
        };

        let bridge = manager.complete_from_l1(&request).expect("Should complete");
        assert_eq!(bridge.status, BridgeStatus::Completed);
        assert_eq!(bridge.direction, BridgeDirection::L1ToL2);
        assert_eq!(bridge.l1_tx_hash, Some("0xdef456".into()));

        // Should reject duplicate completion
        let result = manager.complete_from_l1(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_manager_list_pending() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        // Create multiple bridges
        let tx1 = create_bridge_tx(&godmode, "ALICE", "bb1_a", 100.0);
        let tx2 = create_bridge_tx(&godmode, "BOB", "bb1_b", 200.0);
        
        let b1 = manager.initiate(&tx1).unwrap();
        let _b2 = manager.initiate(&tx2).unwrap();

        // Both should be pending
        let pending = manager.list_pending();
        assert_eq!(pending.len(), 2);

        // Complete one
        manager.complete_bridge(&b1.bridge_id).unwrap();

        // Only one pending now
        let pending = manager.list_pending();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_bridge_manager_list_by_address() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        let alice = godmode.get_account("ALICE").unwrap();
        
        // Create two bridges from Alice
        let tx1 = create_bridge_tx(&godmode, "ALICE", "bb1_a", 100.0);
        let tx2 = create_bridge_tx(&godmode, "ALICE", "bb1_b", 200.0);
        
        manager.initiate(&tx1).unwrap();
        manager.initiate(&tx2).unwrap();

        let alice_bridges = manager.list_by_address(&alice.address);
        assert_eq!(alice_bridges.len(), 2);

        // Bob has none
        let bob = godmode.get_account("BOB").unwrap();
        let bob_bridges = manager.list_by_address(&bob.address);
        assert_eq!(bob_bridges.len(), 0);
    }

    #[test]
    fn test_bridge_manager_fail_bridge() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        let tx = create_bridge_tx(&godmode, "CHARLIE", "bb1_target", 75.0);
        let bridge = manager.initiate(&tx).unwrap();

        manager.fail_bridge(&bridge.bridge_id, "Insufficient L1 balance".into()).unwrap();

        let status = manager.get_status(&bridge.bridge_id).unwrap();
        assert_eq!(status.status, BridgeStatus::Failed);
        assert_eq!(status.error, Some("Insufficient L1 balance".into()));
    }

    #[test]
    fn test_bridge_amount_validation() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        // Too small
        let payload_small = TransactionPayload::Bridge {
            target_layer: "L1".into(),
            target_address: "bb1_target".into(),
            amount: 0.001,
        };
        let tx_small = SignedTransaction::new(&godmode, "ALICE", 1, payload_small).unwrap();
        let result = manager.initiate(&tx_small);
        assert!(matches!(result, Err(BridgeError::InvalidAmount(_))));

        // Too large
        let payload_large = TransactionPayload::Bridge {
            target_layer: "L1".into(),
            target_address: "bb1_target".into(),
            amount: 2_000_000.0,
        };
        let tx_large = SignedTransaction::new(&godmode, "ALICE", 2, payload_large).unwrap();
        let result = manager.initiate(&tx_large);
        assert!(matches!(result, Err(BridgeError::InvalidAmount(_))));
    }

    #[test]
    fn test_bridge_stats() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        // Create some bridges
        let tx1 = create_bridge_tx(&godmode, "ALICE", "bb1_a", 100.0);
        let tx2 = create_bridge_tx(&godmode, "BOB", "bb1_b", 200.0);
        
        let b1 = manager.initiate(&tx1).unwrap();
        manager.initiate(&tx2).unwrap();
        
        // Complete one
        manager.complete_bridge(&b1.bridge_id).unwrap();

        let stats = manager.stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.l2_to_l1, 2);
        assert_eq!(stats.total_volume, 100.0);
    }

    #[test]
    fn test_wrong_payload_type() {
        let godmode = GodMode::new();
        let manager = BridgeManager::new();
        
        // Use a Transfer payload instead of Bridge
        let payload = TransactionPayload::Transfer {
            to: "bb1_target".into(),
            amount: 100.0,
        };
        let tx = SignedTransaction::new(&godmode, "ALICE", 1, payload).unwrap();
        
        let result = manager.initiate(&tx);
        assert!(matches!(result, Err(BridgeError::InvalidDirection(_))));
    }
}
