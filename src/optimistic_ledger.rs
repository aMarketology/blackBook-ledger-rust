/// Optimistic Ledger for Hybrid L1/L2 Balance Management
/// 
/// Implements "Optimistic Execution with Batch Settlement" pattern:
/// - L2 executes bets instantly (optimistic)
/// - L1 remains source of truth
/// - Periodic batch settlements sync L2 state to L1

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use sha2::{Sha256, Digest};

// ============================================================================
// OPTIMISTIC BALANCE
// ============================================================================

/// Tracks both confirmed (L1) and pending (L2) balances for an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimisticBalance {
    /// L1-confirmed balance (source of truth from blockchain)
    pub confirmed_balance: f64,
    
    /// Pending changes from L2 activity (not yet settled to L1)
    pub pending_delta: f64,
    
    /// Last L1 slot/block when this balance was confirmed
    pub last_l1_sync_slot: u64,
    
    /// Timestamp of last L1 sync
    pub last_l1_sync_timestamp: u64,
}

impl OptimisticBalance {
    pub fn new(initial_balance: f64) -> Self {
        Self {
            confirmed_balance: initial_balance,
            pending_delta: 0.0,
            last_l1_sync_slot: 0,
            last_l1_sync_timestamp: Self::now(),
        }
    }

    /// Get the "available" balance (confirmed + pending changes)
    pub fn available_balance(&self) -> f64 {
        self.confirmed_balance + self.pending_delta
    }

    /// Apply a pending change (bet placed, bet won, etc.)
    pub fn apply_pending_change(&mut self, delta: f64) {
        self.pending_delta += delta;
    }

    /// Confirm pending changes after L1 settlement
    pub fn confirm_settlement(&mut self, l1_slot: u64) {
        self.confirmed_balance += self.pending_delta;
        self.pending_delta = 0.0;
        self.last_l1_sync_slot = l1_slot;
        self.last_l1_sync_timestamp = Self::now();
    }

    /// Sync with L1 balance (called after fetching from L1)
    pub fn sync_from_l1(&mut self, l1_balance: f64, l1_slot: u64) {
        self.confirmed_balance = l1_balance;
        self.last_l1_sync_slot = l1_slot;
        self.last_l1_sync_timestamp = Self::now();
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

// ============================================================================
// PENDING BET
// ============================================================================

/// Status of a pending bet in the optimistic execution queue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PendingBetStatus {
    /// Accepted on L2, not yet included in a batch
    Pending,
    /// Included in a batch submission to L1
    Batched,
    /// L1 confirmed the batch containing this bet
    Settled,
    /// Failed fraud proof or L1 rejection
    Rejected,
}

/// A bet that has been executed optimistically on L2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBet {
    /// Unique bet identifier
    pub id: String,
    
    /// Account that placed the bet (wallet address)
    pub account: String,
    
    /// Market being bet on
    pub market_id: String,
    
    /// Outcome index (0 or 1)
    pub outcome: usize,
    
    /// Amount wagered
    pub amount: f64,
    
    /// Unix timestamp when bet was placed
    pub timestamp: u64,
    
    /// L2 block height when bet was accepted
    pub l2_block: u64,
    
    /// Signature from user authorizing this bet
    pub signature: String,
    
    /// Current status of this bet
    pub status: PendingBetStatus,
    
    /// Batch ID if included in a batch (None if pending)
    pub batch_id: Option<String>,
}

impl PendingBet {
    pub fn new(
        id: String,
        account: String,
        market_id: String,
        outcome: usize,
        amount: f64,
        l2_block: u64,
        signature: String,
    ) -> Self {
        Self {
            id,
            account,
            market_id,
            outcome,
            amount,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            l2_block,
            signature,
            status: PendingBetStatus::Pending,
            batch_id: None,
        }
    }
}

// ============================================================================
// BATCH SETTLEMENT
// ============================================================================

/// Status of a batch settlement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BatchStatus {
    /// Batch is being accumulated
    Accumulating,
    /// Batch submitted to L1, awaiting confirmation
    Submitted,
    /// L1 confirmed, in challenge window
    Confirmed,
    /// Past challenge window, fully finalized
    Finalized,
    /// Challenged and rejected
    Rejected,
}

/// A batch of bets to be settled on L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSettlement {
    /// Unique batch identifier
    pub batch_id: String,
    
    /// L2 block range covered by this batch
    pub l2_block_start: u64,
    pub l2_block_end: u64,
    
    /// IDs of bets included in this batch
    pub bet_ids: Vec<String>,
    
    /// Net balance changes per account (address -> delta)
    pub balance_changes: HashMap<String, f64>,
    
    /// Merkle root of all transactions in batch
    pub merkle_root: String,
    
    /// Timestamp when batch was created
    pub created_at: u64,
    
    /// Timestamp when batch was submitted to L1
    pub submitted_at: Option<u64>,
    
    /// L1 transaction hash (after submission)
    pub l1_tx_hash: Option<String>,
    
    /// L1 slot when confirmed
    pub l1_slot: Option<u64>,
    
    /// Current status
    pub status: BatchStatus,
}

impl BatchSettlement {
    pub fn new(batch_id: String, l2_block_start: u64) -> Self {
        Self {
            batch_id,
            l2_block_start,
            l2_block_end: l2_block_start,
            bet_ids: Vec::new(),
            balance_changes: HashMap::new(),
            merkle_root: String::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            submitted_at: None,
            l1_tx_hash: None,
            l1_slot: None,
            status: BatchStatus::Accumulating,
        }
    }

    /// Add a bet to this batch
    pub fn add_bet(&mut self, bet: &PendingBet) {
        self.bet_ids.push(bet.id.clone());
        self.l2_block_end = bet.l2_block;
        
        // Track balance change for the bettor (negative = they bet)
        let current_delta = self.balance_changes.get(&bet.account).unwrap_or(&0.0);
        self.balance_changes.insert(bet.account.clone(), current_delta - bet.amount);
    }

    /// Compute the Merkle root of all balance changes
    pub fn compute_merkle_root(&mut self) {
        let mut leaves: Vec<String> = self.balance_changes
            .iter()
            .map(|(addr, delta)| format!("{}:{}", addr, delta))
            .collect();
        
        leaves.sort(); // Deterministic ordering
        
        // Simple merkle root computation (single hash of all leaves for now)
        let mut hasher = Sha256::new();
        for leaf in &leaves {
            hasher.update(leaf.as_bytes());
        }
        self.merkle_root = hex::encode(hasher.finalize());
    }

    /// Get total number of accounts affected
    pub fn affected_accounts(&self) -> usize {
        self.balance_changes.len()
    }

    /// Get total volume in this batch
    pub fn total_volume(&self) -> f64 {
        self.balance_changes.values().map(|d| d.abs()).sum()
    }
}

// ============================================================================
// OPTIMISTIC LEDGER
// ============================================================================

/// The main optimistic ledger that manages L1/L2 hybrid state
#[derive(Debug)]
pub struct OptimisticLedger {
    /// Account balances with optimistic tracking
    pub balances: HashMap<String, OptimisticBalance>,
    
    /// Account name -> address mapping (mirrors original ledger)
    pub accounts: HashMap<String, String>,
    
    /// Queue of pending bets awaiting batch settlement
    pub pending_bets: VecDeque<PendingBet>,
    
    /// Current batch being accumulated
    pub current_batch: Option<BatchSettlement>,
    
    /// Submitted batches awaiting confirmation
    pub pending_batches: VecDeque<BatchSettlement>,
    
    /// Finalized batches (history)
    pub finalized_batches: Vec<BatchSettlement>,
    
    /// Current L2 block height (increments with each operation)
    pub l2_block_height: u64,
    
    /// Batch configuration
    pub batch_interval_secs: u64,
    pub batch_max_bets: usize,
    pub challenge_window_secs: u64,
    
    /// Last time a batch was submitted
    pub last_batch_submitted_at: u64,
}

impl OptimisticLedger {
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            accounts: HashMap::new(),
            pending_bets: VecDeque::new(),
            current_batch: None,
            pending_batches: VecDeque::new(),
            finalized_batches: Vec::new(),
            l2_block_height: 0,
            batch_interval_secs: 60,      // Submit batch every 60 seconds
            batch_max_bets: 100,          // Or when 100 bets accumulated
            challenge_window_secs: 300,   // 5 minute challenge window
            last_batch_submitted_at: 0,
        }
    }

    /// Initialize an account with L1-confirmed balance
    pub fn init_account(&mut self, name: String, address: String, l1_balance: f64) {
        self.accounts.insert(name.clone(), address.clone());
        self.balances.insert(address, OptimisticBalance::new(l1_balance));
        println!("📊 [OptimisticLedger] Initialized account {} with {} BB (L1 confirmed)", name, l1_balance);
    }

    /// Resolve name or address to address
    pub fn resolve_address(&self, id: &str) -> String {
        if let Some(addr) = self.accounts.get(&id.to_uppercase()) {
            return addr.clone();
        }
        if self.balances.contains_key(id) {
            return id.to_string();
        }
        id.to_string()
    }

    /// Get available balance (confirmed + pending) for an account
    pub fn get_available_balance(&self, address_or_name: &str) -> f64 {
        let addr = self.resolve_address(address_or_name);
        self.balances.get(&addr)
            .map(|b| b.available_balance())
            .unwrap_or(0.0)
    }

    /// Get confirmed (L1) balance only
    pub fn get_confirmed_balance(&self, address_or_name: &str) -> f64 {
        let addr = self.resolve_address(address_or_name);
        self.balances.get(&addr)
            .map(|b| b.confirmed_balance)
            .unwrap_or(0.0)
    }

    /// Get pending delta (unsettled changes)
    pub fn get_pending_delta(&self, address_or_name: &str) -> f64 {
        let addr = self.resolve_address(address_or_name);
        self.balances.get(&addr)
            .map(|b| b.pending_delta)
            .unwrap_or(0.0)
    }

    /// Place a bet optimistically (instant execution on L2)
    pub fn place_bet_optimistic(
        &mut self,
        account: &str,
        market_id: &str,
        outcome: usize,
        amount: f64,
        signature: String,
    ) -> Result<PendingBet, String> {
        let addr = self.resolve_address(account);
        
        // Check available balance
        let balance = self.balances.get_mut(&addr)
            .ok_or_else(|| format!("Account not found: {}", account))?;
        
        if balance.available_balance() < amount {
            return Err(format!(
                "Insufficient balance: {} has {} BB available but needs {}",
                account, balance.available_balance(), amount
            ));
        }

        // Increment L2 block height
        self.l2_block_height += 1;

        // Create pending bet
        let bet_id = format!("bet_{}_{}_{}", market_id, self.l2_block_height, uuid::Uuid::new_v4().simple());
        let bet = PendingBet::new(
            bet_id.clone(),
            addr.clone(),
            market_id.to_string(),
            outcome,
            amount,
            self.l2_block_height,
            signature,
        );

        // Apply optimistic balance change (deduct immediately)
        balance.apply_pending_change(-amount);
        
        // Add to pending queue
        self.pending_bets.push_back(bet.clone());

        // Add to current batch
        self.add_to_batch(&bet);

        println!("🎯 [OptimisticLedger] Bet {} placed optimistically: {} BB on market {} (L2 block {})", 
                 bet_id, amount, market_id, self.l2_block_height);

        Ok(bet)
    }

    /// Add bet to current batch (or create new batch)
    fn add_to_batch(&mut self, bet: &PendingBet) {
        if self.current_batch.is_none() {
            let batch_id = format!("batch_{}", uuid::Uuid::new_v4().simple());
            self.current_batch = Some(BatchSettlement::new(batch_id, bet.l2_block));
        }

        if let Some(batch) = &mut self.current_batch {
            batch.add_bet(bet);
        }
    }

    /// Check if current batch should be submitted
    pub fn should_submit_batch(&self) -> bool {
        if let Some(batch) = &self.current_batch {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Submit if batch is full or time interval passed
            batch.bet_ids.len() >= self.batch_max_bets 
                || (now - batch.created_at) >= self.batch_interval_secs
        } else {
            false
        }
    }

    /// Prepare current batch for submission to L1
    pub fn prepare_batch_for_submission(&mut self) -> Option<BatchSettlement> {
        if let Some(mut batch) = self.current_batch.take() {
            batch.compute_merkle_root();
            batch.status = BatchStatus::Submitted;
            batch.submitted_at = Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
            
            // Mark all bets in this batch as "Batched"
            for bet in &mut self.pending_bets {
                if batch.bet_ids.contains(&bet.id) {
                    bet.status = PendingBetStatus::Batched;
                    bet.batch_id = Some(batch.batch_id.clone());
                }
            }

            println!("📦 [OptimisticLedger] Batch {} prepared with {} bets, merkle root: {}", 
                     batch.batch_id, batch.bet_ids.len(), &batch.merkle_root[..16]);
            
            Some(batch)
        } else {
            None
        }
    }

    /// Record that a batch was confirmed on L1
    pub fn confirm_batch(&mut self, batch_id: &str, l1_tx_hash: String, l1_slot: u64) {
        // Find the batch in pending batches
        if let Some(pos) = self.pending_batches.iter().position(|b| b.batch_id == batch_id) {
            let mut batch = self.pending_batches.remove(pos).unwrap();
            batch.l1_tx_hash = Some(l1_tx_hash);
            batch.l1_slot = Some(l1_slot);
            batch.status = BatchStatus::Confirmed;

            // Mark bets as settled and confirm balances
            for bet in &mut self.pending_bets {
                if bet.batch_id.as_ref() == Some(&batch.batch_id) {
                    bet.status = PendingBetStatus::Settled;
                }
            }

            // Confirm balance changes
            for (addr, _delta) in &batch.balance_changes {
                if let Some(balance) = self.balances.get_mut(addr) {
                    balance.confirm_settlement(l1_slot);
                }
            }

            self.finalized_batches.push(batch);
            println!("✅ [OptimisticLedger] Batch {} confirmed on L1 at slot {}", batch_id, l1_slot);
        }
    }

    /// Sync account balance from L1
    pub fn sync_balance_from_l1(&mut self, address: &str, l1_balance: f64, l1_slot: u64) {
        if let Some(balance) = self.balances.get_mut(address) {
            let old_confirmed = balance.confirmed_balance;
            balance.sync_from_l1(l1_balance, l1_slot);
            println!("🔄 [OptimisticLedger] Synced {} from L1: {} -> {} BB", 
                     address, old_confirmed, l1_balance);
        } else {
            // New account discovered on L1
            self.balances.insert(address.to_string(), OptimisticBalance::new(l1_balance));
            println!("🆕 [OptimisticLedger] New account from L1: {} with {} BB", address, l1_balance);
        }
    }

    /// Get summary of pending settlements
    pub fn get_settlement_summary(&self) -> SettlementSummary {
        let pending_bet_count = self.pending_bets.iter()
            .filter(|b| b.status == PendingBetStatus::Pending)
            .count();
        
        let batched_count = self.pending_bets.iter()
            .filter(|b| b.status == PendingBetStatus::Batched)
            .count();

        let pending_volume: f64 = self.pending_bets.iter()
            .filter(|b| b.status == PendingBetStatus::Pending || b.status == PendingBetStatus::Batched)
            .map(|b| b.amount)
            .sum();

        SettlementSummary {
            l2_block_height: self.l2_block_height,
            pending_bets: pending_bet_count,
            batched_bets: batched_count,
            pending_batches: self.pending_batches.len(),
            finalized_batches: self.finalized_batches.len(),
            pending_volume,
            current_batch_size: self.current_batch.as_ref().map(|b| b.bet_ids.len()).unwrap_or(0),
        }
    }

    /// Add tokens to an account (for deposits/admin)
    pub fn add_tokens(&mut self, address_or_name: &str, amount: f64) -> Result<String, String> {
        if amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }

        let addr = self.resolve_address(address_or_name);
        
        if let Some(balance) = self.balances.get_mut(&addr) {
            // For now, treat deposits as immediately confirmed
            // In production, this would wait for L1 confirmation
            balance.confirmed_balance += amount;
            Ok(format!("Added {} BB to {} (now {} BB)", amount, addr, balance.available_balance()))
        } else {
            // Create new account
            self.balances.insert(addr.clone(), OptimisticBalance::new(amount));
            Ok(format!("Created account {} with {} BB", addr, amount))
        }
    }

    /// Transfer between accounts (optimistic)
    pub fn transfer_optimistic(&mut self, from: &str, to: &str, amount: f64) -> Result<String, String> {
        if amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }

        let from_addr = self.resolve_address(from);
        let to_addr = self.resolve_address(to);

        // Check sender balance
        let from_balance = self.balances.get(&from_addr)
            .ok_or_else(|| format!("Sender account not found: {}", from))?;
        
        if from_balance.available_balance() < amount {
            return Err(format!(
                "Insufficient balance: {} has {} BB but needs {}",
                from, from_balance.available_balance(), amount
            ));
        }

        // Apply changes
        self.balances.get_mut(&from_addr).unwrap().apply_pending_change(-amount);
        
        if let Some(to_bal) = self.balances.get_mut(&to_addr) {
            to_bal.apply_pending_change(amount);
        } else {
            // Create recipient account
            let mut new_balance = OptimisticBalance::new(0.0);
            new_balance.apply_pending_change(amount);
            self.balances.insert(to_addr.clone(), new_balance);
        }

        self.l2_block_height += 1;

        Ok(format!("Transferred {} BB from {} to {}", amount, from_addr, to_addr))
    }
}

// ============================================================================
// SUMMARY TYPES
// ============================================================================

/// Summary of settlement status for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementSummary {
    pub l2_block_height: u64,
    pub pending_bets: usize,
    pub batched_bets: usize,
    pub pending_batches: usize,
    pub finalized_batches: usize,
    pub pending_volume: f64,
    pub current_batch_size: usize,
}

/// Account balance details for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalanceDetails {
    pub address: String,
    pub confirmed_balance: f64,
    pub pending_delta: f64,
    pub available_balance: f64,
    pub last_l1_sync_slot: u64,
    pub last_l1_sync_timestamp: u64,
}

impl From<(&String, &OptimisticBalance)> for AccountBalanceDetails {
    fn from((address, balance): (&String, &OptimisticBalance)) -> Self {
        Self {
            address: address.clone(),
            confirmed_balance: balance.confirmed_balance,
            pending_delta: balance.pending_delta,
            available_balance: balance.available_balance(),
            last_l1_sync_slot: balance.last_l1_sync_slot,
            last_l1_sync_timestamp: balance.last_l1_sync_timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimistic_balance() {
        let mut balance = OptimisticBalance::new(1000.0);
        assert_eq!(balance.available_balance(), 1000.0);
        
        balance.apply_pending_change(-100.0);
        assert_eq!(balance.confirmed_balance, 1000.0);
        assert_eq!(balance.pending_delta, -100.0);
        assert_eq!(balance.available_balance(), 900.0);
        
        balance.confirm_settlement(1);
        assert_eq!(balance.confirmed_balance, 900.0);
        assert_eq!(balance.pending_delta, 0.0);
    }

    #[test]
    fn test_place_bet_optimistic() {
        let mut ledger = OptimisticLedger::new();
        ledger.init_account("ALICE".to_string(), "L1_ALICE_ADDR".to_string(), 1000.0);
        
        let result = ledger.place_bet_optimistic(
            "ALICE",
            "test_market",
            0,
            100.0,
            "sig_test".to_string(),
        );
        
        assert!(result.is_ok());
        assert_eq!(ledger.get_available_balance("ALICE"), 900.0);
        assert_eq!(ledger.get_confirmed_balance("ALICE"), 1000.0);
        assert_eq!(ledger.get_pending_delta("ALICE"), -100.0);
    }
}
