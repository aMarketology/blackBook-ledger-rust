/// Unified Ledger Module for BlackBook L1/L2
/// 
/// Comprehensive ledger tracking ALL activity across both layers:
/// - L1: Consensus, balances, bridges, settlements
/// - L2: Bets, markets, payouts, liquidity
///
/// KEY FEATURES:
/// - Reconstructs from persisted market data on startup
/// - Tracks which layer holds funds (L1 vs L2)
/// - Shows locked/escrowed amounts
/// - Provides unified view of all blockchain activity

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

// ============================================================================
// LAYER & FUND STATUS TRACKING
// ============================================================================

/// Which blockchain layer a transaction/balance exists on
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Layer {
    L1,  // Consensus layer (port 8080)
    L2,  // Prediction market layer (port 1234)
    Bridge, // In transit between layers
}

impl Default for Layer {
    fn default() -> Self { Layer::L2 }
}

/// Status of funds in a transaction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FundStatus {
    Available,   // Spendable balance
    Locked,      // Locked in bet/escrow, cannot spend
    Pending,     // Awaiting confirmation
    Settled,     // Bet resolved, funds released
    Bridging,    // Moving between L1/L2
}

impl Default for FundStatus {
    fn default() -> Self { FundStatus::Available }
}

// ============================================================================
// CORE TYPES
// ============================================================================

/// Account balance with L1/L2 tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// Confirmed balance (synced from L1)
    pub confirmed: f64,
    /// Pending changes (L2 activity not yet settled)
    pub pending: f64,
    /// Amount locked in active bets
    pub locked: f64,
    /// Layer where this balance resides
    pub layer: Layer,
    /// Last L1 sync timestamp
    pub last_sync: u64,
}

impl Balance {
    pub fn new(amount: f64) -> Self {
        Self { confirmed: amount, pending: 0.0, locked: 0.0, layer: Layer::L2, last_sync: now() }
    }
    
    pub fn new_on_layer(amount: f64, layer: Layer) -> Self {
        Self { confirmed: amount, pending: 0.0, locked: 0.0, layer, last_sync: now() }
    }
    
    pub fn available(&self) -> f64 {
        (self.confirmed + self.pending - self.locked).max(0.0)
    }
    
    pub fn total(&self) -> f64 {
        self.confirmed + self.pending
    }
    
    pub fn apply(&mut self, delta: f64) {
        self.pending += delta;
    }
    
    pub fn lock(&mut self, amount: f64) {
        self.locked += amount;
    }
    
    pub fn unlock(&mut self, amount: f64) {
        self.locked = (self.locked - amount).max(0.0);
    }
    
    pub fn settle(&mut self) {
        self.confirmed += self.pending;
        self.pending = 0.0;
        self.last_sync = now();
    }
}

/// Full balance breakdown for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceBreakdown {
    pub address: String,
    pub available: f64,      // Can spend now
    pub locked: f64,         // In active bets
    pub pending: f64,        // Awaiting confirmation
    pub confirmed: f64,      // Synced from L1
    pub total: f64,          // confirmed + pending
    pub layer: Layer,        // Where funds reside
}

/// Transaction types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxType {
    Bet,
    Transfer,
    Deposit,
    Withdraw,
    Payout,
    AccountCreated,
    MarketCreated,
    LiquidityAdded,
    MarketResolved,
    BridgeInitiate,
    BridgeComplete,
    ShareMint,
    ShareRedeem,
    OrderPlace,
    OrderCancel,
    OrderFill,
}

/// A single transaction record with full L1/L2 tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub tx_type: TxType,
    pub from: String,
    pub to: Option<String>,
    pub amount: f64,
    pub market_id: Option<String>,
    pub outcome: Option<usize>,
    pub timestamp: u64,
    pub signature: String,
    
    // === L1/L2 TRACKING FIELDS ===
    /// Which layer this transaction occurred on
    #[serde(default)]
    pub layer: Layer,
    /// Current status of the funds
    #[serde(default)]
    pub fund_status: FundStatus,
    /// If bridging, the target layer
    #[serde(default)]
    pub target_layer: Option<Layer>,
    /// L1 settlement status (for L2 transactions that settle to L1)
    #[serde(default)]
    pub l1_settled: bool,
    /// L1 settlement tx hash (if settled)
    #[serde(default)]
    pub l1_tx_hash: Option<String>,
    /// Block number on the relevant layer
    #[serde(default)]
    pub block_number: u64,
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
}

impl Transaction {
    pub fn new(tx_type: TxType, from: &str, amount: f64, signature: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tx_type,
            from: from.to_string(),
            to: None,
            amount,
            market_id: None,
            outcome: None,
            timestamp: now(),
            signature: signature.to_string(),
            layer: Layer::L2,
            fund_status: FundStatus::Available,
            target_layer: None,
            l1_settled: false,
            l1_tx_hash: None,
            block_number: 0,
            description: None,
        }
    }
    
    pub fn bet(from: &str, market_id: &str, outcome: usize, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::Bet, from, amount, sig);
        tx.market_id = Some(market_id.to_string());
        tx.outcome = Some(outcome);
        tx.fund_status = FundStatus::Locked;
        tx.description = Some(format!("Bet {} BB on outcome {} in market", amount, outcome));
        tx
    }
    
    pub fn transfer(from: &str, to: &str, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::Transfer, from, amount, sig);
        tx.to = Some(to.to_string());
        tx.description = Some(format!("Transfer {} BB", amount));
        tx
    }
    
    pub fn market_created(market_id: &str, title: &str, liquidity: f64) -> Self {
        let mut tx = Self::new(TxType::MarketCreated, "SYSTEM", liquidity, "market_genesis");
        tx.market_id = Some(market_id.to_string());
        tx.to = Some(title.to_string());
        tx.fund_status = FundStatus::Locked;
        tx.description = Some(format!("Market created: {}", title));
        tx
    }
    
    pub fn liquidity_added(market_id: &str, funder: &str, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::LiquidityAdded, funder, amount, sig);
        tx.market_id = Some(market_id.to_string());
        tx.fund_status = FundStatus::Locked;
        tx.description = Some(format!("Added {} BB liquidity", amount));
        tx
    }
    
    pub fn market_resolved(market_id: &str, winning_outcome: usize) -> Self {
        let mut tx = Self::new(TxType::MarketResolved, "SYSTEM", 0.0, "resolution");
        tx.market_id = Some(market_id.to_string());
        tx.outcome = Some(winning_outcome);
        tx.description = Some(format!("Market resolved: outcome {} wins", winning_outcome));
        tx
    }
    
    pub fn payout(to: &str, market_id: &str, amount: f64) -> Self {
        let mut tx = Self::new(TxType::Payout, "SYSTEM", amount, "payout");
        tx.to = Some(to.to_string());
        tx.market_id = Some(market_id.to_string());
        tx.fund_status = FundStatus::Settled;
        tx.description = Some(format!("Payout {} BB from resolved market", amount));
        tx
    }
    
    pub fn bridge_initiate(from: &str, amount: f64, from_layer: Layer, to_layer: Layer, sig: &str) -> Self {
        let mut tx = Self::new(TxType::BridgeInitiate, from, amount, sig);
        tx.layer = from_layer;
        tx.target_layer = Some(to_layer);
        tx.fund_status = FundStatus::Bridging;
        tx.description = Some(format!("Bridge {} BB from {:?} to {:?}", amount, from_layer, to_layer));
        tx
    }
    
    pub fn bridge_complete(to: &str, amount: f64, layer: Layer) -> Self {
        let mut tx = Self::new(TxType::BridgeComplete, "BRIDGE", amount, "bridge_complete");
        tx.to = Some(to.to_string());
        tx.layer = layer;
        tx.fund_status = FundStatus::Available;
        tx.description = Some(format!("Bridge complete: {} BB arrived on {:?}", amount, layer));
        tx
    }
    
    pub fn deposit(to: &str, amount: f64, layer: Layer) -> Self {
        let mut tx = Self::new(TxType::Deposit, "SYSTEM", amount, "deposit");
        tx.to = Some(to.to_string());
        tx.layer = layer;
        tx.description = Some(format!("Deposit {} BB on {:?}", amount, layer));
        tx
    }
    
    /// Create from a persisted MarketBet (for reconstruction)
    pub fn from_market_bet(bet_id: &str, market_id: &str, bettor: &str, outcome: usize, amount: f64, timestamp: u64, status: &str) -> Self {
        let fund_status = match status {
            "pending" => FundStatus::Locked,
            "won" => FundStatus::Settled,
            "lost" => FundStatus::Settled,
            _ => FundStatus::Locked,
        };
        
        Self {
            id: bet_id.to_string(),
            tx_type: TxType::Bet,
            from: bettor.to_string(),
            to: None,
            amount,
            market_id: Some(market_id.to_string()),
            outcome: Some(outcome),
            timestamp,
            signature: String::new(),
            layer: Layer::L2,
            fund_status,
            target_layer: None,
            l1_settled: false,
            l1_tx_hash: None,
            block_number: 0,
            description: Some(format!("Bet {} BB on outcome {}", amount, outcome)),
        }
    }
}

// ============================================================================
// LEDGER
// ============================================================================

/// The main ledger tracking all accounts and transactions
#[derive(Debug)]
pub struct Ledger {
    /// Account balances (address -> Balance)
    pub balances: HashMap<String, Balance>,
    /// Name -> Address mapping
    pub accounts: HashMap<String, String>,
    /// All transactions
    pub transactions: Vec<Transaction>,
    /// Current L2 block
    pub block: u64,
    /// L1 RPC URL
    pub l1_url: String,
    /// Use mock mode (no real L1 calls)
    pub mock_mode: bool,
}

impl Ledger {
    pub fn new() -> Self {
        let l1_url = std::env::var("L1_RPC_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
        let mock_mode = std::env::var("L1_MOCK_MODE").map(|v| v == "true").unwrap_or(true);
        
        println!("📒 Ledger initialized (L1: {}, mock: {})", l1_url, mock_mode);
        
        Self {
            balances: HashMap::new(),
            accounts: HashMap::new(),
            transactions: Vec::new(),
            block: 0,
            l1_url,
            mock_mode,
        }
    }
    
    /// Register an account with initial balance
    pub fn register(&mut self, name: &str, address: &str, initial: f64) {
        self.accounts.insert(name.to_uppercase(), address.to_string());
        self.balances.insert(address.to_string(), Balance::new(initial));
        self.transactions.push(Transaction::new(TxType::AccountCreated, address, initial, ""));
        let display_addr = if address.len() > 16 { &address[..16] } else { address };
        println!("👤 Registered {} ({}) with {} BB", name, display_addr, initial);
    }
    
    /// Resolve name or address to address
    pub fn resolve(&self, id: &str) -> Option<String> {
        if let Some(addr) = self.accounts.get(&id.to_uppercase()) {
            return Some(addr.clone());
        }
        if self.balances.contains_key(id) {
            return Some(id.to_string());
        }
        None
    }
    
    /// Get available balance
    pub fn balance(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.available())
            .unwrap_or(0.0)
    }
    
    /// Get confirmed balance only
    pub fn confirmed_balance(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.confirmed)
            .unwrap_or(0.0)
    }
    
    /// Get pending delta
    pub fn pending(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.pending)
            .unwrap_or(0.0)
    }
    
    /// Place a bet - locks funds until market resolution
    pub fn place_bet(&mut self, from: &str, market_id: &str, outcome: usize, amount: f64, sig: &str) -> Result<Transaction, String> {
        let addr = self.resolve(from).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        
        if bal.available() < amount {
            return Err(format!("Insufficient balance: {} < {}", bal.available(), amount));
        }
        
        // Lock funds for the bet (deduct from available, add to locked)
        bal.apply(-amount);  // Reduce available balance
        bal.lock(amount);    // Track as locked in bet
        self.block += 1;
        
        let mut tx = Transaction::bet(&addr, market_id, outcome, amount, sig);
        tx.fund_status = FundStatus::Locked; // Mark as locked
        self.transactions.push(tx.clone());
        
        println!("🎯 Bet: {} wagered {} BB on {} (outcome {}) [🔒 locked]", &addr[..16], amount, market_id, outcome);
        Ok(tx)
    }
    
    /// Get locked balance for an account (funds in active bets)
    pub fn locked(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.locked)
            .unwrap_or(0.0)
    }
    
    /// Get full balance breakdown for an account
    pub fn balance_breakdown(&self, id: &str) -> Option<BalanceBreakdown> {
        let addr = self.resolve(id)?;
        let bal = self.balances.get(&addr)?;
        Some(BalanceBreakdown {
            address: addr.clone(),
            available: bal.available(),
            locked: bal.locked,
            pending: bal.pending,
            confirmed: bal.confirmed,
            total: bal.total(),
            layer: bal.layer,
        })
    }
    
    /// Transfer between accounts
    pub fn transfer(&mut self, from: &str, to: &str, amount: f64, sig: &str) -> Result<Transaction, String> {
        let from_addr = self.resolve(from).ok_or("Sender not found")?;
        let to_addr = self.resolve(to).ok_or("Recipient not found")?;
        
        {
            let from_bal = self.balances.get(&from_addr).ok_or("Sender balance not found")?;
            if from_bal.available() < amount {
                return Err(format!("Insufficient balance: {} < {}", from_bal.available(), amount));
            }
        }
        
        self.balances.get_mut(&from_addr).unwrap().apply(-amount);
        self.balances.get_mut(&to_addr).unwrap().apply(amount);
        self.block += 1;
        
        let tx = Transaction::transfer(&from_addr, &to_addr, amount, sig);
        self.transactions.push(tx.clone());
        
        println!("💸 Transfer: {} -> {} ({} BB)", &from_addr[..16], &to_addr[..16], amount);
        Ok(tx)
    }
    
    /// Add tokens to account (deposit/mint)
    pub fn add_tokens(&mut self, id: &str, amount: f64) -> Result<f64, String> {
        let addr = self.resolve(id).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        bal.confirmed += amount;
        
        let mut tx = Transaction::new(TxType::Deposit, &addr, amount, "");
        self.transactions.push(tx);
        
        println!("📥 Deposit: {} received {} BB", &addr[..16], amount);
        Ok(bal.available())
    }
    
    /// Payout winnings - also unlocks the original bet amount
    pub fn payout(&mut self, to: &str, amount: f64, market_id: &str) -> Result<f64, String> {
        let addr = self.resolve(to).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        
        // Add winnings to balance
        bal.apply(amount);
        
        let mut tx = Transaction::new(TxType::Payout, &addr, amount, "");
        tx.market_id = Some(market_id.to_string());
        tx.fund_status = FundStatus::Settled;
        self.transactions.push(tx);
        
        println!("🏆 Payout: {} won {} BB from {}", &addr[..16], amount, market_id);
        Ok(bal.available())
    }
    
    /// Unlock funds from a resolved bet (loser gets nothing back, but unlock tracking)
    pub fn unlock_bet(&mut self, account: &str, bet_amount: f64, market_id: &str) -> Result<(), String> {
        let addr = self.resolve(account).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        
        // Unlock the bet amount from tracking
        bal.unlock(bet_amount);
        
        println!("🔓 Unlocked {} BB for {} from {} (bet resolved)", bet_amount, &addr[..16], market_id);
        Ok(())
    }
    
    /// Get transactions for an address
    pub fn get_transactions(&self, id: &str) -> Vec<&Transaction> {
        let addr = match self.resolve(id) {
            Some(a) => a,
            None => return vec![],
        };
        self.transactions.iter()
            .filter(|tx| tx.from == addr || tx.to.as_ref() == Some(&addr))
            .collect()
    }
    
    /// Get recent transactions
    pub fn recent_transactions(&self, limit: usize) -> Vec<&Transaction> {
        self.transactions.iter().rev().take(limit).collect()
    }
    
    /// Credit an account (add balance) - simplified version for handlers
    pub fn credit(&mut self, id: &str, amount: f64) {
        if let Some(addr) = self.resolve(id) {
            if let Some(bal) = self.balances.get_mut(&addr) {
                bal.apply(amount);
            }
        } else {
            // Create account if doesn't exist
            let addr = id.to_string();
            self.accounts.insert(id.to_string(), addr.clone());
            self.balances.insert(addr.clone(), Balance::new(amount));
        }
    }
    
    /// Debit an account (subtract balance) - simplified version for handlers
    pub fn debit(&mut self, id: &str, amount: f64) {
        if let Some(addr) = self.resolve(id) {
            if let Some(bal) = self.balances.get_mut(&addr) {
                bal.apply(-amount);
            }
        }
    }
    
    /// Record any transaction (used for market events, liquidity, etc.)
    pub fn record(&mut self, tx: Transaction) -> Transaction {
        self.block += 1;
        self.transactions.push(tx.clone());
        tx
    }
    
    /// Get stats
    pub fn stats(&self) -> LedgerStats {
        let total_bets = self.transactions.iter().filter(|t| t.tx_type == TxType::Bet).count();
        let bet_volume: f64 = self.transactions.iter()
            .filter(|t| t.tx_type == TxType::Bet)
            .map(|t| t.amount)
            .sum();
        
        let locked_volume: f64 = self.transactions.iter()
            .filter(|t| t.fund_status == FundStatus::Locked)
            .map(|t| t.amount)
            .sum();
        
        LedgerStats {
            accounts: self.balances.len(),
            transactions: self.transactions.len(),
            block: self.block,
            total_bets,
            bet_volume,
            locked_volume,
            l1_transactions: self.transactions.iter().filter(|t| t.layer == Layer::L1).count(),
            l2_transactions: self.transactions.iter().filter(|t| t.layer == Layer::L2).count(),
            bridge_transactions: self.transactions.iter().filter(|t| t.layer == Layer::Bridge).count(),
        }
    }
    
    /// Clear and rebuild transactions from market data
    /// Call this on startup to sync ledger with persisted market state
    pub fn clear_transactions(&mut self) {
        self.transactions.clear();
        self.block = 0;
        println!("📒 Ledger transactions cleared for reconstruction");
    }
    
    /// Add a transaction from market reconstruction (doesn't affect balances)
    pub fn add_reconstructed_transaction(&mut self, tx: Transaction) {
        self.transactions.push(tx);
        self.block += 1;
    }
    
    /// Get locked balance for an address (funds in active bets)
    pub fn locked_balance(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.locked)
            .unwrap_or(0.0)
    }
    
    /// Get total balance for an address (confirmed + pending)
    pub fn total_balance(&self, id: &str) -> f64 {
        self.resolve(id)
            .and_then(|addr| self.balances.get(&addr))
            .map(|b| b.total())
            .unwrap_or(0.0)
    }
    
    /// Get full balance info for an address
    pub fn full_balance(&self, id: &str) -> Option<FullBalanceInfo> {
        let addr = self.resolve(id)?;
        let bal = self.balances.get(&addr)?;
        
        Some(FullBalanceInfo {
            address: addr,
            available: bal.available(),
            confirmed: bal.confirmed,
            pending: bal.pending,
            locked: bal.locked,
            layer: bal.layer,
            last_sync: bal.last_sync,
        })
    }
    
    /// Get transactions by type
    pub fn get_transactions_by_type(&self, tx_type: TxType) -> Vec<&Transaction> {
        self.transactions.iter()
            .filter(|tx| tx.tx_type == tx_type)
            .collect()
    }
    
    /// Get transactions by layer
    pub fn get_transactions_by_layer(&self, layer: Layer) -> Vec<&Transaction> {
        self.transactions.iter()
            .filter(|tx| tx.layer == layer)
            .collect()
    }
    
    /// Get transactions by fund status
    pub fn get_transactions_by_status(&self, status: FundStatus) -> Vec<&Transaction> {
        self.transactions.iter()
            .filter(|tx| tx.fund_status == status)
            .collect()
    }
    
    /// Get all locked funds (active bets, pending bridges)
    pub fn get_locked_funds(&self) -> Vec<&Transaction> {
        self.transactions.iter()
            .filter(|tx| matches!(tx.fund_status, FundStatus::Locked | FundStatus::Bridging | FundStatus::Pending))
            .collect()
    }
    
    /// Get comprehensive unified view of all activity
    pub fn unified_view(&self) -> UnifiedLedgerView {
        let mut total_l1_volume = 0.0;
        let mut total_l2_volume = 0.0;
        let mut total_locked = 0.0;
        let mut total_bridging = 0.0;
        
        for tx in &self.transactions {
            match tx.layer {
                Layer::L1 => total_l1_volume += tx.amount,
                Layer::L2 => total_l2_volume += tx.amount,
                Layer::Bridge => total_bridging += tx.amount,
            }
            
            if matches!(tx.fund_status, FundStatus::Locked | FundStatus::Pending) {
                total_locked += tx.amount;
            }
            if tx.fund_status == FundStatus::Bridging {
                total_bridging += tx.amount;
            }
        }
        
        // Build account summaries
        let mut account_summaries: Vec<AccountSummary> = self.balances.iter()
            .map(|(addr, bal)| {
                let tx_count = self.transactions.iter()
                    .filter(|tx| tx.from == *addr || tx.to.as_ref() == Some(addr))
                    .count();
                
                AccountSummary {
                    address: addr.clone(),
                    available: bal.available(),
                    locked: bal.locked,
                    layer: bal.layer,
                    transaction_count: tx_count,
                }
            })
            .collect();
        
        account_summaries.sort_by(|a, b| b.available.partial_cmp(&a.available).unwrap());
        
        UnifiedLedgerView {
            total_transactions: self.transactions.len(),
            total_accounts: self.balances.len(),
            current_block: self.block,
            l1_volume: total_l1_volume,
            l2_volume: total_l2_volume,
            total_locked,
            total_bridging,
            stats: self.stats(),
            accounts: account_summaries,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStats {
    pub accounts: usize,
    pub transactions: usize,
    pub block: u64,
    pub total_bets: usize,
    pub bet_volume: f64,
    pub locked_volume: f64,
    pub l1_transactions: usize,
    pub l2_transactions: usize,
    pub bridge_transactions: usize,
}

/// Full balance info for an address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullBalanceInfo {
    pub address: String,
    pub available: f64,
    pub confirmed: f64,
    pub pending: f64,
    pub locked: f64,
    pub layer: Layer,
    pub last_sync: u64,
}

/// Summary of an account for unified view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub address: String,
    pub available: f64,
    pub locked: f64,
    pub layer: Layer,
    pub transaction_count: usize,
}

/// Unified view of all ledger activity across L1/L2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedLedgerView {
    pub total_transactions: usize,
    pub total_accounts: usize,
    pub current_block: u64,
    pub l1_volume: f64,
    pub l2_volume: f64,
    pub total_locked: f64,
    pub total_bridging: f64,
    pub stats: LedgerStats,
    pub accounts: Vec<AccountSummary>,
}

// ============================================================================
// L1 RPC CLIENT
// ============================================================================

/// Simple L1 RPC client for blockchain communication
pub struct L1Client {
    pub url: String,
    pub mock: bool,
    client: reqwest::Client,
}

impl L1Client {
    pub fn new() -> Self {
        let url = std::env::var("L1_RPC_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
        let mock = std::env::var("L1_MOCK_MODE").map(|v| v == "true").unwrap_or(true);
        
        Self {
            url,
            mock,
            client: reqwest::Client::new(),
        }
    }
    
    /// Get account balance from L1
    pub async fn get_balance(&self, address: &str) -> Result<f64, String> {
        if self.mock {
            return Ok(30000.0); // Mock balance
        }
        
        let url = format!("{}/rpc/accounts/{}/balance", self.url, address);
        let resp = self.client.get(&url).send().await
            .map_err(|e| format!("L1 request failed: {}", e))?;
        
        #[derive(Deserialize)]
        struct BalanceResp { balance: f64 }
        
        let data: BalanceResp = resp.json().await
            .map_err(|e| format!("Failed to parse L1 response: {}", e))?;
        
        Ok(data.balance)
    }
    
    /// Verify signature via L1
    pub async fn verify_signature(&self, address: &str, message: &str, signature: &str) -> Result<bool, String> {
        if self.mock {
            return Ok(true); // Always valid in mock
        }
        
        let url = format!("{}/rpc/verify_signature", self.url);
        
        #[derive(Serialize)]
        struct VerifyReq<'a> {
            address: &'a str,
            message: &'a str,
            signature: &'a str,
        }
        
        let resp = self.client.post(&url)
            .json(&VerifyReq { address, message, signature })
            .send().await
            .map_err(|e| format!("L1 verify failed: {}", e))?;
        
        #[derive(Deserialize)]
        struct VerifyResp { valid: bool }
        
        let data: VerifyResp = resp.json().await
            .map_err(|e| format!("Failed to parse verify response: {}", e))?;
        
        Ok(data.valid)
    }
    
    /// Get current nonce for address
    pub async fn get_nonce(&self, address: &str) -> Result<u64, String> {
        if self.mock {
            return Ok(0);
        }
        
        let url = format!("{}/rpc/accounts/{}/nonce", self.url, address);
        let resp = self.client.get(&url).send().await
            .map_err(|e| format!("L1 nonce request failed: {}", e))?;
        
        #[derive(Deserialize)]
        struct NonceResp { nonce: u64 }
        
        let data: NonceResp = resp.json().await
            .map_err(|e| format!("Failed to parse nonce response: {}", e))?;
        
        Ok(data.nonce)
    }
}

// ============================================================================
// HELPERS
// ============================================================================

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Compute SHA256 hash
pub fn hash(data: &str) -> String {
    hex::encode(Sha256::digest(data.as_bytes()))
}

// ============================================================================
// API RESPONSE TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub address: String,
    pub available: f64,
    pub confirmed: f64,
    pub pending: f64,
    pub locked: f64,
    pub layer: String,
}

/// Full transaction response with L1/L2 tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub id: String,
    pub tx_type: String,
    pub from: String,
    pub to: Option<String>,
    pub amount: f64,
    pub market_id: Option<String>,
    pub outcome: Option<usize>,
    pub timestamp: u64,
    // L1/L2 tracking
    pub layer: String,
    pub fund_status: String,
    pub target_layer: Option<String>,
    pub l1_settled: bool,
    pub l1_tx_hash: Option<String>,
    pub block_number: u64,
    pub description: Option<String>,
}

impl From<&Transaction> for TransactionResponse {
    fn from(tx: &Transaction) -> Self {
        Self {
            id: tx.id.clone(),
            tx_type: format!("{:?}", tx.tx_type),
            from: tx.from.clone(),
            to: tx.to.clone(),
            amount: tx.amount,
            market_id: tx.market_id.clone(),
            outcome: tx.outcome,
            timestamp: tx.timestamp,
            layer: format!("{:?}", tx.layer),
            fund_status: format!("{:?}", tx.fund_status),
            target_layer: tx.target_layer.map(|l| format!("{:?}", l)),
            l1_settled: tx.l1_settled,
            l1_tx_hash: tx.l1_tx_hash.clone(),
            block_number: tx.block_number,
            description: tx.description.clone(),
        }
    }
}

/// Simplified bet data for reconstruction (avoids cross-module dependencies)
#[derive(Debug, Clone)]
pub struct BetData {
    pub id: String,
    pub market_id: String,
    pub bettor: String,
    pub outcome: usize,
    pub amount: f64,
    pub timestamp: u64,
    pub status: String,
}

/// Simplified market data for reconstruction
#[derive(Debug, Clone)]
pub struct MarketData {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub total_volume: f64,
    pub is_resolved: bool,
    pub winning_option: Option<usize>,
    pub bets: Vec<BetData>,
}

/// Reconstruct ledger transactions from market data
/// This is called to rebuild the ledger from the actual source of truth (persisted markets)
pub fn reconstruct_transactions_from_market_data(markets: &[MarketData]) -> Vec<Transaction> {
    let mut transactions = Vec::new();
    
    for market in markets {
        // Add market creation transaction
        let mut market_tx = Transaction::market_created(&market.id, &market.title, market.total_volume);
        market_tx.timestamp = market.created_at;
        market_tx.block_number = transactions.len() as u64 + 1;
        transactions.push(market_tx);
        
        // Add all bets from this market
        for bet in &market.bets {
            let tx = Transaction::from_market_bet(
                &bet.id,
                &bet.market_id,
                &bet.bettor,
                bet.outcome,
                bet.amount,
                bet.timestamp,
                &bet.status,
            );
            transactions.push(tx);
        }
        
        // If market is resolved, add resolution transaction
        if market.is_resolved {
            if let Some(winning) = market.winning_option {
                let mut resolve_tx = Transaction::market_resolved(&market.id, winning);
                resolve_tx.timestamp = market.created_at + 1; // Approximate
                transactions.push(resolve_tx);
            }
        }
    }
    
    // Sort by timestamp
    transactions.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    
    // Assign block numbers
    for (i, tx) in transactions.iter_mut().enumerate() {
        tx.block_number = i as u64 + 1;
    }
    
    println!("📒 Reconstructed {} transactions from {} markets", transactions.len(), markets.len());
    transactions
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_balance() {
        let mut bal = Balance::new(1000.0);
        assert_eq!(bal.available(), 1000.0);
        
        bal.apply(-100.0);
        assert_eq!(bal.available(), 900.0);
        assert_eq!(bal.confirmed, 1000.0);
        assert_eq!(bal.pending, -100.0);
        
        bal.settle();
        assert_eq!(bal.confirmed, 900.0);
        assert_eq!(bal.pending, 0.0);
    }
    
    #[test]
    fn test_ledger_bet() {
        let mut ledger = Ledger::new();
        ledger.register("ALICE", "L1_ALICE_ADDR", 1000.0);
        
        let result = ledger.place_bet("ALICE", "market_1", 0, 100.0, "sig");
        assert!(result.is_ok());
        assert_eq!(ledger.balance("ALICE"), 900.0);
    }
}
