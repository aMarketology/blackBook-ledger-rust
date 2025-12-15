/// Unified Ledger Module for BlackBook L2
/// 
/// Simple, consolidated ledger tracking all L2 activity with L1 sync capability.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

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
    /// Last L1 sync timestamp
    pub last_sync: u64,
}

impl Balance {
    pub fn new(amount: f64) -> Self {
        Self { confirmed: amount, pending: 0.0, last_sync: now() }
    }
    
    pub fn available(&self) -> f64 {
        self.confirmed + self.pending
    }
    
    pub fn apply(&mut self, delta: f64) {
        self.pending += delta;
    }
    
    pub fn settle(&mut self) {
        self.confirmed += self.pending;
        self.pending = 0.0;
        self.last_sync = now();
    }
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
}

/// A single transaction record
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
        }
    }
    
    pub fn bet(from: &str, market_id: &str, outcome: usize, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::Bet, from, amount, sig);
        tx.market_id = Some(market_id.to_string());
        tx.outcome = Some(outcome);
        tx
    }
    
    pub fn transfer(from: &str, to: &str, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::Transfer, from, amount, sig);
        tx.to = Some(to.to_string());
        tx
    }
    
    pub fn market_created(market_id: &str, title: &str, liquidity: f64) -> Self {
        let mut tx = Self::new(TxType::MarketCreated, "SYSTEM", liquidity, "market_genesis");
        tx.market_id = Some(market_id.to_string());
        tx.to = Some(title.to_string()); // Store title in 'to' field for reference
        tx
    }
    
    pub fn liquidity_added(market_id: &str, funder: &str, amount: f64, sig: &str) -> Self {
        let mut tx = Self::new(TxType::LiquidityAdded, funder, amount, sig);
        tx.market_id = Some(market_id.to_string());
        tx
    }
    
    pub fn market_resolved(market_id: &str, winning_outcome: usize) -> Self {
        let mut tx = Self::new(TxType::MarketResolved, "SYSTEM", 0.0, "resolution");
        tx.market_id = Some(market_id.to_string());
        tx.outcome = Some(winning_outcome);
        tx
    }
    
    pub fn payout(to: &str, market_id: &str, amount: f64) -> Self {
        let mut tx = Self::new(TxType::Payout, "SYSTEM", amount, "payout");
        tx.to = Some(to.to_string());
        tx.market_id = Some(market_id.to_string());
        tx
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
        println!("👤 Registered {} ({}) with {} BB", name, &address[..16], initial);
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
    
    /// Place a bet
    pub fn place_bet(&mut self, from: &str, market_id: &str, outcome: usize, amount: f64, sig: &str) -> Result<Transaction, String> {
        let addr = self.resolve(from).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        
        if bal.available() < amount {
            return Err(format!("Insufficient balance: {} < {}", bal.available(), amount));
        }
        
        bal.apply(-amount);
        self.block += 1;
        
        let tx = Transaction::bet(&addr, market_id, outcome, amount, sig);
        self.transactions.push(tx.clone());
        
        println!("🎯 Bet: {} wagered {} BB on {} (outcome {})", &addr[..16], amount, market_id, outcome);
        Ok(tx)
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
    
    /// Payout winnings
    pub fn payout(&mut self, to: &str, amount: f64, market_id: &str) -> Result<f64, String> {
        let addr = self.resolve(to).ok_or("Account not found")?;
        let bal = self.balances.get_mut(&addr).ok_or("Balance not found")?;
        bal.apply(amount);
        
        let mut tx = Transaction::new(TxType::Payout, &addr, amount, "");
        tx.market_id = Some(market_id.to_string());
        self.transactions.push(tx);
        
        println!("🏆 Payout: {} won {} BB from {}", &addr[..16], amount, market_id);
        Ok(bal.available())
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
        
        LedgerStats {
            accounts: self.balances.len(),
            transactions: self.transactions.len(),
            block: self.block,
            total_bets,
            bet_volume,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub id: String,
    pub tx_type: String,
    pub from: String,
    pub to: Option<String>,
    pub amount: f64,
    pub market_id: Option<String>,
    pub timestamp: u64,
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
            timestamp: tx.timestamp,
        }
    }
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
