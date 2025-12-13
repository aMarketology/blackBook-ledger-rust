/// L1 Sync Service - Handles synchronization between L2 and L1
/// 
/// Responsibilities:
/// - Fetch L1 balances and reconcile with L2
/// - Submit batch settlements to L1
/// - Check batch finality (past challenge window)
/// - Process L1→L2 deposits

use crate::l1_rpc_client::{L1RpcClient, L1RpcError, SettlementRequest, SettlementResponse};
use crate::optimistic_ledger::{OptimisticLedger, BatchSettlement, BatchStatus, SettlementSummary};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// SYNC STATUS
// ============================================================================

/// Status of the L1 sync service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    /// Not syncing, idle
    Idle,
    /// Currently syncing balances from L1
    SyncingBalances,
    /// Currently submitting a batch to L1
    SubmittingBatch,
    /// Waiting for batch confirmation
    AwaitingConfirmation,
    /// Sync failed, will retry
    Error(String),
}

/// Result of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub success: bool,
    pub accounts_synced: usize,
    pub batches_submitted: usize,
    pub batches_confirmed: usize,
    pub error: Option<String>,
    pub timestamp: u64,
}

impl SyncResult {
    pub fn success(accounts: usize, submitted: usize, confirmed: usize) -> Self {
        Self {
            success: true,
            accounts_synced: accounts,
            batches_submitted: submitted,
            batches_confirmed: confirmed,
            error: None,
            timestamp: Self::now(),
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            accounts_synced: 0,
            batches_submitted: 0,
            batches_confirmed: 0,
            error: Some(error),
            timestamp: Self::now(),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

// ============================================================================
// L1 SYNC SERVICE
// ============================================================================

/// Service for synchronizing L2 state with L1 blockchain
pub struct L1SyncService {
    /// L1 RPC client for blockchain communication
    pub rpc_client: L1RpcClient,
    
    /// How often to attempt sync (in seconds)
    pub sync_interval_secs: u64,
    
    /// Last L1 slot we synced from
    pub last_sync_slot: u64,
    
    /// Current sync status
    pub status: SyncStatus,
    
    /// Last sync result
    pub last_sync_result: Option<SyncResult>,
    
    /// Timestamp of last sync attempt
    pub last_sync_timestamp: u64,
}

impl L1SyncService {
    pub fn new(rpc_client: L1RpcClient) -> Self {
        Self {
            rpc_client,
            sync_interval_secs: 60, // Sync every 60 seconds
            last_sync_slot: 0,
            status: SyncStatus::Idle,
            last_sync_result: None,
            last_sync_timestamp: 0,
        }
    }

    /// Create sync service from environment
    pub fn from_env() -> Self {
        Self::new(L1RpcClient::from_env())
    }

    /// Check if a sync is needed based on time interval
    pub fn should_sync(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now - self.last_sync_timestamp >= self.sync_interval_secs
    }

    /// Perform a full sync cycle
    pub async fn sync(&mut self, ledger: &mut OptimisticLedger) -> SyncResult {
        println!("🔄 [L1Sync] Starting sync cycle...");
        self.status = SyncStatus::SyncingBalances;
        self.last_sync_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut accounts_synced = 0;
        let mut batches_submitted = 0;
        let mut batches_confirmed = 0;

        // Step 1: Sync balances from L1 (if connected)
        if self.rpc_client.is_connected() {
            match self.sync_balances_from_l1(ledger).await {
                Ok(count) => {
                    accounts_synced = count;
                    println!("✅ [L1Sync] Synced {} accounts from L1", count);
                }
                Err(e) => {
                    let error = format!("Failed to sync balances: {}", e);
                    println!("❌ [L1Sync] {}", error);
                    self.status = SyncStatus::Error(error.clone());
                    let result = SyncResult::failure(error);
                    self.last_sync_result = Some(result.clone());
                    return result;
                }
            }
        } else {
            println!("⚠️  [L1Sync] Mock mode - skipping L1 balance sync");
        }

        // Step 2: Submit pending batches to L1
        self.status = SyncStatus::SubmittingBatch;
        if ledger.should_submit_batch() {
            if let Some(batch) = ledger.prepare_batch_for_submission() {
                match self.submit_batch_to_l1(&batch).await {
                    Ok(tx_hash) => {
                        println!("✅ [L1Sync] Batch {} submitted, tx: {}", batch.batch_id, tx_hash);
                        // Add to pending batches
                        let mut submitted_batch = batch;
                        submitted_batch.l1_tx_hash = Some(tx_hash);
                        submitted_batch.status = BatchStatus::Submitted;
                        ledger.pending_batches.push_back(submitted_batch);
                        batches_submitted = 1;
                    }
                    Err(e) => {
                        println!("⚠️  [L1Sync] Failed to submit batch: {} (will retry)", e);
                        // Put batch back for retry
                        ledger.current_batch = Some(batch);
                    }
                }
            }
        }

        // Step 3: Check for batch confirmations
        self.status = SyncStatus::AwaitingConfirmation;
        batches_confirmed = self.check_batch_confirmations(ledger).await;

        // Done
        self.status = SyncStatus::Idle;
        let result = SyncResult::success(accounts_synced, batches_submitted, batches_confirmed);
        self.last_sync_result = Some(result.clone());
        
        println!("✅ [L1Sync] Sync complete: {} accounts, {} batches submitted, {} confirmed",
                 accounts_synced, batches_submitted, batches_confirmed);
        
        result
    }

    /// Sync balances from L1 for all known accounts
    async fn sync_balances_from_l1(&mut self, ledger: &mut OptimisticLedger) -> Result<usize, L1RpcError> {
        let addresses: Vec<String> = ledger.balances.keys().cloned().collect();
        let mut synced = 0;

        for address in addresses {
            match self.rpc_client.get_balance(&address).await {
                Ok(l1_balance) => {
                    // Get the current L1 slot (mock returns 0)
                    let l1_slot = self.rpc_client.get_current_slot().await.unwrap_or(0);
                    ledger.sync_balance_from_l1(&address, l1_balance, l1_slot);
                    self.last_sync_slot = l1_slot;
                    synced += 1;
                }
                Err(L1RpcError::AccountNotFound(_)) => {
                    // Account doesn't exist on L1 yet, that's OK
                    println!("ℹ️  [L1Sync] Account {} not found on L1 (L2-only account)", address);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(synced)
    }

    /// Submit a batch settlement to L1
    async fn submit_batch_to_l1(&self, batch: &BatchSettlement) -> Result<String, L1RpcError> {
        if self.rpc_client.is_mock_mode() {
            // In mock mode, simulate successful submission
            let mock_tx = format!("mock_tx_{}", &batch.batch_id[..8]);
            println!("📤 [L1Sync] Mock mode - simulating batch submission: {}", mock_tx);
            return Ok(mock_tx);
        }

        // Build settlement request
        let winners: Vec<(String, f64)> = batch.balance_changes
            .iter()
            .filter(|(_, delta)| **delta > 0.0) // Only positive changes (winnings)
            .map(|(addr, delta)| (addr.clone(), *delta))
            .collect();

        let request = SettlementRequest {
            market_id: format!("batch_{}", batch.batch_id),
            outcome: 0, // Batch settlement doesn't have a single outcome
            winners,
            l2_block_height: batch.l2_block_end,
            l2_signature: batch.merkle_root.clone(),
        };

        match self.rpc_client.record_batch_settlement(request).await {
            Ok(response) if response.recorded => {
                Ok(response.l1_tx_hash.unwrap_or_else(|| "unknown".to_string()))
            }
            Ok(response) => {
                Err(L1RpcError::Other(response.error.unwrap_or_else(|| "Settlement rejected".to_string())))
            }
            Err(e) => Err(e),
        }
    }

    /// Check if pending batches have been confirmed on L1
    async fn check_batch_confirmations(&self, ledger: &mut OptimisticLedger) -> usize {
        let mut confirmed = 0;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get batch IDs that are ready to be confirmed
        let batch_ids: Vec<String> = ledger.pending_batches
            .iter()
            .filter(|b| {
                b.status == BatchStatus::Submitted 
                && b.submitted_at.map(|t| now - t >= ledger.challenge_window_secs).unwrap_or(false)
            })
            .map(|b| b.batch_id.clone())
            .collect();

        for batch_id in batch_ids {
            // In mock mode, auto-confirm after challenge window
            if self.rpc_client.is_mock_mode() {
                let mock_slot = self.last_sync_slot + 1;
                ledger.confirm_batch(&batch_id, format!("confirmed_{}", batch_id), mock_slot);
                confirmed += 1;
            } else {
                // In live mode, we would check L1 for actual confirmation
                // For now, just confirm after challenge window
                let mock_slot = self.last_sync_slot + 1;
                ledger.confirm_batch(&batch_id, format!("confirmed_{}", batch_id), mock_slot);
                confirmed += 1;
            }
        }

        confirmed
    }

    /// Get current sync status
    pub fn get_status(&self) -> SyncServiceStatus {
        SyncServiceStatus {
            status: self.status.clone(),
            is_connected: self.rpc_client.is_connected(),
            is_mock_mode: self.rpc_client.is_mock_mode(),
            last_sync_slot: self.last_sync_slot,
            last_sync_timestamp: self.last_sync_timestamp,
            sync_interval_secs: self.sync_interval_secs,
            last_result: self.last_sync_result.clone(),
        }
    }

    /// Manually trigger balance sync for a specific account
    pub async fn sync_account(&mut self, ledger: &mut OptimisticLedger, address: &str) -> Result<f64, String> {
        if self.rpc_client.is_mock_mode() {
            // In mock mode, just return current L2 balance
            let balance = ledger.get_available_balance(address);
            return Ok(balance);
        }

        match self.rpc_client.get_balance(address).await {
            Ok(l1_balance) => {
                let l1_slot = self.rpc_client.get_current_slot().await.unwrap_or(0);
                ledger.sync_balance_from_l1(address, l1_balance, l1_slot);
                Ok(l1_balance)
            }
            Err(e) => Err(format!("Failed to sync from L1: {}", e)),
        }
    }

    /// Force submit current batch (for testing/manual settlement)
    pub async fn force_submit_batch(&mut self, ledger: &mut OptimisticLedger) -> Result<String, String> {
        if let Some(batch) = ledger.prepare_batch_for_submission() {
            match self.submit_batch_to_l1(&batch).await {
                Ok(tx_hash) => {
                    let mut submitted_batch = batch;
                    submitted_batch.l1_tx_hash = Some(tx_hash.clone());
                    submitted_batch.status = BatchStatus::Submitted;
                    ledger.pending_batches.push_back(submitted_batch);
                    Ok(tx_hash)
                }
                Err(e) => {
                    // Put batch back
                    ledger.current_batch = Some(batch);
                    Err(format!("Failed to submit batch: {}", e))
                }
            }
        } else {
            Err("No batch ready for submission".to_string())
        }
    }
}

// ============================================================================
// STATUS TYPES
// ============================================================================

/// Full status of the sync service for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncServiceStatus {
    pub status: SyncStatus,
    pub is_connected: bool,
    pub is_mock_mode: bool,
    pub last_sync_slot: u64,
    pub last_sync_timestamp: u64,
    pub sync_interval_secs: u64,
    pub last_result: Option<SyncResult>,
}

/// Combined status for settlement endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSettlementStatus {
    /// Sync service status
    pub sync: SyncServiceStatus,
    /// Ledger settlement summary
    pub settlement: SettlementSummary,
}
