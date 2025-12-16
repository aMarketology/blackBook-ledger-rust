// ============================================================================
// Shares Module - BlackBook Prediction Market
// ============================================================================
//
// Outcome share system for the prediction market.
// Shares represent ownership of a specific outcome in a market.
//
// Core Invariant:
//   1 YES share + 1 NO share = 1 BB (always redeemable)
//
// This creates the arbitrage mechanism that keeps prices accurate:
//   - If YES + NO prices > 1.00 BB: Mint shares and sell both for profit
//   - If YES + NO prices < 1.00 BB: Buy both and redeem for profit
//
// Share Types:
//   - YES shares: Pay 1 BB if event happens, 0 if not
//   - NO shares: Pay 1 BB if event doesn't happen, 0 if it does
//
// Resolution:
//   - Winning shares redeem 1:1 for BB
//   - Losing shares become worthless (0 BB)
//
// ============================================================================

pub mod mint;
pub mod redeem;

pub use mint::*;
pub use redeem::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum share transaction size
pub const MIN_SHARE_SIZE: f64 = 0.01;

/// Maximum share transaction size (per operation)
pub const MAX_SHARE_SIZE: f64 = 1_000_000.0;

// ============================================================================
// OUTCOME
// ============================================================================

/// Outcome index for a market
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutcomeIndex(pub usize);

impl OutcomeIndex {
    pub const YES: OutcomeIndex = OutcomeIndex(0);
    pub const NO: OutcomeIndex = OutcomeIndex(1);

    pub fn new(index: usize) -> Self {
        OutcomeIndex(index)
    }
    
    pub fn from_usize(index: usize) -> Self {
        OutcomeIndex(index)
    }

    pub fn index(&self) -> usize {
        self.0
    }

    pub fn opposite(&self) -> Self {
        // For binary markets
        OutcomeIndex(if self.0 == 0 { 1 } else { 0 })
    }
}

// ============================================================================
// SHARE POSITION
// ============================================================================

/// A user's position in a specific market outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePosition {
    /// Market ID
    pub market_id: String,
    
    /// Outcome index (0 = YES, 1 = NO for binary)
    pub outcome: OutcomeIndex,
    
    /// Wallet address of holder
    pub holder: String,
    
    /// Number of shares owned
    pub shares: f64,
    
    /// Average cost basis (in BB per share)
    pub avg_cost: f64,
    
    /// Total BB invested
    pub total_cost: f64,
    
    /// Unrealized P&L (based on current price)
    pub unrealized_pnl: f64,
    
    /// Realized P&L (from sales)
    pub realized_pnl: f64,
    
    /// Created timestamp
    pub created_at: u64,
    
    /// Last updated timestamp  
    pub updated_at: u64,
}

impl SharePosition {
    pub fn new(market_id: String, outcome: OutcomeIndex, holder: String) -> Self {
        let now = now();
        Self {
            market_id,
            outcome,
            holder,
            shares: 0.0,
            avg_cost: 0.0,
            total_cost: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add shares to position
    pub fn add_shares(&mut self, amount: f64, price: f64) {
        let new_cost = amount * price;
        let old_total = self.shares * self.avg_cost;
        
        self.shares += amount;
        self.total_cost += new_cost;
        
        if self.shares > 0.0 {
            self.avg_cost = (old_total + new_cost) / self.shares;
        }
        
        self.updated_at = now();
    }

    /// Remove shares from position
    pub fn remove_shares(&mut self, amount: f64, price: f64) -> f64 {
        let shares_to_remove = amount.min(self.shares);
        
        if shares_to_remove <= 0.0 {
            return 0.0;
        }

        // Calculate P&L on the sold portion
        let cost_basis = shares_to_remove * self.avg_cost;
        let sale_proceeds = shares_to_remove * price;
        let pnl = sale_proceeds - cost_basis;
        
        self.realized_pnl += pnl;
        self.shares -= shares_to_remove;
        self.total_cost = self.shares * self.avg_cost;
        self.updated_at = now();

        shares_to_remove
    }

    /// Update unrealized P&L based on current price
    pub fn update_unrealized_pnl(&mut self, current_price: f64) {
        let current_value = self.shares * current_price;
        self.unrealized_pnl = current_value - self.total_cost;
    }

    /// Check if position has shares
    pub fn has_shares(&self) -> bool {
        self.shares > MIN_SHARE_SIZE
    }

    /// Get position value at a given price
    pub fn value_at(&self, price: f64) -> f64 {
        self.shares * price
    }
}

// ============================================================================
// SHARE BALANCE (All positions for a user)
// ============================================================================

/// All share positions for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareBalance {
    /// Wallet address
    pub holder: String,
    
    /// Positions by (market_id, outcome)
    pub positions: HashMap<(String, usize), SharePosition>,
    
    /// Total value of all positions
    pub total_value: f64,
    
    /// Total unrealized P&L
    pub total_unrealized_pnl: f64,
    
    /// Total realized P&L
    pub total_realized_pnl: f64,
}

impl ShareBalance {
    pub fn new(holder: String) -> Self {
        Self {
            holder,
            positions: HashMap::new(),
            total_value: 0.0,
            total_unrealized_pnl: 0.0,
            total_realized_pnl: 0.0,
        }
    }

    /// Get or create position for a market/outcome
    pub fn get_or_create_position(&mut self, market_id: &str, outcome: OutcomeIndex) -> &mut SharePosition {
        let key = (market_id.to_string(), outcome.index());
        
        if !self.positions.contains_key(&key) {
            self.positions.insert(
                key.clone(),
                SharePosition::new(market_id.to_string(), outcome, self.holder.clone()),
            );
        }
        
        self.positions.get_mut(&key).unwrap()
    }

    /// Get shares for a specific market/outcome
    pub fn get_shares(&self, market_id: &str, outcome: OutcomeIndex) -> f64 {
        let key = (market_id.to_string(), outcome.index());
        self.positions.get(&key).map(|p| p.shares).unwrap_or(0.0)
    }

    /// Get all positions for a market
    pub fn get_market_positions(&self, market_id: &str) -> Vec<&SharePosition> {
        self.positions.values()
            .filter(|p| p.market_id == market_id)
            .collect()
    }

    /// Get all active positions (with shares > 0)
    pub fn active_positions(&self) -> Vec<&SharePosition> {
        self.positions.values()
            .filter(|p| p.has_shares())
            .collect()
    }

    /// Update total stats
    pub fn update_totals(&mut self, price_fn: impl Fn(&str, OutcomeIndex) -> f64) {
        self.total_value = 0.0;
        self.total_unrealized_pnl = 0.0;
        self.total_realized_pnl = 0.0;

        for pos in self.positions.values_mut() {
            let price = price_fn(&pos.market_id, pos.outcome);
            pos.update_unrealized_pnl(price);
            
            self.total_value += pos.value_at(price);
            self.total_unrealized_pnl += pos.unrealized_pnl;
            self.total_realized_pnl += pos.realized_pnl;
        }
    }
}

// ============================================================================
// SHARES MANAGER
// ============================================================================

/// Manages all share positions and operations
#[derive(Debug)]
pub struct SharesManager {
    /// User balances: wallet -> ShareBalance
    pub balances: HashMap<String, ShareBalance>,
    
    /// Total shares minted per market/outcome
    pub total_supply: HashMap<(String, usize), f64>,
    
    /// Transaction history
    pub transactions: Vec<ShareTransaction>,
    
    /// Statistics
    pub stats: SharesStats,
}

/// Share transaction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareTransaction {
    pub id: String,
    pub tx_type: ShareTxType,
    pub market_id: String,
    pub outcome: OutcomeIndex,
    pub wallet: String,
    pub shares: f64,
    pub bb_amount: f64,
    pub price_per_share: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ShareTxType {
    /// Minted from BB (got YES + NO)
    Mint,
    /// Redeemed to BB (burned YES + NO)
    Redeem,
    /// Bought shares
    Buy,
    /// Sold shares
    Sell,
    /// Transfer between wallets
    Transfer,
    /// Resolution payout
    Payout,
    /// Resolution - shares burned (loser)
    Burn,
}

/// Statistics for the share system
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharesStats {
    pub total_shares_minted: f64,
    pub total_shares_redeemed: f64,
    pub total_bb_locked: f64,
    pub total_transactions: u64,
    pub unique_holders: usize,
}

impl SharesManager {
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            total_supply: HashMap::new(),
            transactions: Vec::new(),
            stats: SharesStats::default(),
        }
    }

    /// Get or create user balance
    pub fn get_or_create_balance(&mut self, wallet: &str) -> &mut ShareBalance {
        if !self.balances.contains_key(wallet) {
            self.balances.insert(wallet.to_string(), ShareBalance::new(wallet.to_string()));
            self.stats.unique_holders = self.balances.len();
        }
        self.balances.get_mut(wallet).unwrap()
    }

    /// Get user's shares for a market/outcome
    pub fn get_shares(&self, wallet: &str, market_id: &str, outcome: OutcomeIndex) -> f64 {
        self.balances.get(wallet)
            .map(|b| b.get_shares(market_id, outcome))
            .unwrap_or(0.0)
    }

    /// Get total supply for a market/outcome
    pub fn get_total_supply(&self, market_id: &str, outcome: OutcomeIndex) -> f64 {
        let key = (market_id.to_string(), outcome.index());
        self.total_supply.get(&key).copied().unwrap_or(0.0)
    }

    /// Add shares to a user (from mint, buy, or transfer)
    pub fn credit_shares(
        &mut self,
        wallet: &str,
        market_id: &str,
        outcome: OutcomeIndex,
        amount: f64,
        price: f64,
        tx_type: ShareTxType,
    ) {
        let balance = self.get_or_create_balance(wallet);
        let position = balance.get_or_create_position(market_id, outcome);
        position.add_shares(amount, price);

        // Update total supply for mints
        if tx_type == ShareTxType::Mint {
            let key = (market_id.to_string(), outcome.index());
            *self.total_supply.entry(key).or_insert(0.0) += amount;
        }

        // Record transaction
        self.record_transaction(tx_type, market_id, outcome, wallet, amount, amount * price, price);
    }

    /// Remove shares from a user (from redeem, sell, or transfer)
    pub fn debit_shares(
        &mut self,
        wallet: &str,
        market_id: &str,
        outcome: OutcomeIndex,
        amount: f64,
        price: f64,
        tx_type: ShareTxType,
    ) -> Result<f64, String> {
        let balance = self.balances.get_mut(wallet)
            .ok_or_else(|| "Wallet not found".to_string())?;

        let current_shares = balance.get_shares(market_id, outcome);
        if current_shares < amount {
            return Err(format!("Insufficient shares: have {}, need {}", current_shares, amount));
        }

        let position = balance.get_or_create_position(market_id, outcome);
        let removed = position.remove_shares(amount, price);

        // Update total supply for redeems/burns
        if tx_type == ShareTxType::Redeem || tx_type == ShareTxType::Burn {
            let key = (market_id.to_string(), outcome.index());
            if let Some(supply) = self.total_supply.get_mut(&key) {
                *supply = (*supply - removed).max(0.0);
            }
        }

        // Record transaction
        self.record_transaction(tx_type, market_id, outcome, wallet, removed, removed * price, price);

        Ok(removed)
    }

    /// Record a transaction
    fn record_transaction(
        &mut self,
        tx_type: ShareTxType,
        market_id: &str,
        outcome: OutcomeIndex,
        wallet: &str,
        shares: f64,
        bb_amount: f64,
        price: f64,
    ) {
        let tx = ShareTransaction {
            id: format!("stx_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            tx_type,
            market_id: market_id.to_string(),
            outcome,
            wallet: wallet.to_string(),
            shares,
            bb_amount,
            price_per_share: price,
            timestamp: now(),
        };

        self.transactions.push(tx);
        self.stats.total_transactions += 1;

        match tx_type {
            ShareTxType::Mint => self.stats.total_shares_minted += shares,
            ShareTxType::Redeem => self.stats.total_shares_redeemed += shares,
            _ => {}
        }
    }

    /// Get user's position summary
    pub fn get_user_positions(&self, wallet: &str) -> UserPositionsSummary {
        let balance = match self.balances.get(wallet) {
            Some(b) => b,
            None => return UserPositionsSummary::empty(wallet),
        };

        let positions: Vec<PositionInfo> = balance.positions.values()
            .filter(|p| p.has_shares())
            .map(|p| PositionInfo {
                market_id: p.market_id.clone(),
                outcome: p.outcome.index(),
                shares: p.shares,
                avg_cost: p.avg_cost,
                total_cost: p.total_cost,
                unrealized_pnl: p.unrealized_pnl,
                realized_pnl: p.realized_pnl,
            })
            .collect();

        UserPositionsSummary {
            wallet: wallet.to_string(),
            positions,
            total_value: balance.total_value,
            total_unrealized_pnl: balance.total_unrealized_pnl,
            total_realized_pnl: balance.total_realized_pnl,
        }
    }

    /// Get transaction history for a wallet
    pub fn get_wallet_transactions(&self, wallet: &str, limit: usize) -> Vec<&ShareTransaction> {
        self.transactions.iter()
            .rev()
            .filter(|tx| tx.wallet == wallet)
            .take(limit)
            .collect()
    }

    /// Resolve market - pay winners, burn losers
    pub fn resolve_market(
        &mut self,
        market_id: &str,
        winning_outcome: OutcomeIndex,
        num_outcomes: usize,
    ) -> Vec<(String, f64)> {
        let mut payouts = Vec::new();
        let mut transactions_to_record: Vec<(ShareTxType, String, OutcomeIndex, String, f64, f64, f64)> = Vec::new();

        // Get all wallets with positions in this market
        let wallets: Vec<String> = self.balances.keys().cloned().collect();

        for wallet in wallets {
            let balance = match self.balances.get_mut(&wallet) {
                Some(b) => b,
                None => continue,
            };

            // Process each outcome
            for outcome_idx in 0..num_outcomes {
                let outcome = OutcomeIndex::new(outcome_idx);
                let key = (market_id.to_string(), outcome_idx);
                
                if let Some(position) = balance.positions.get_mut(&key) {
                    if position.shares > 0.0 {
                        let shares = position.shares;
                        
                        if outcome == winning_outcome {
                            // Winner: pay out 1 BB per share
                            payouts.push((wallet.clone(), shares));
                            
                            // Queue payout transaction
                            transactions_to_record.push((
                                ShareTxType::Payout,
                                market_id.to_string(),
                                outcome,
                                wallet.clone(),
                                shares,
                                shares, // 1:1 payout
                                1.0,
                            ));
                        } else {
                            // Loser: burn shares (0 payout)
                            transactions_to_record.push((
                                ShareTxType::Burn,
                                market_id.to_string(),
                                outcome,
                                wallet.clone(),
                                shares,
                                0.0,
                                0.0,
                            ));
                        }
                        
                        // Clear the position
                        position.shares = 0.0;
                        position.total_cost = 0.0;
                    }
                }
            }
        }

        // Record all transactions (borrow on balances released)
        for (tx_type, mid, outcome, wallet, shares, bb_amount, price) in transactions_to_record {
            self.record_transaction(tx_type, &mid, outcome, &wallet, shares, bb_amount, price);
        }

        // Clear total supply for this market
        for outcome_idx in 0..num_outcomes {
            let key = (market_id.to_string(), outcome_idx);
            self.total_supply.remove(&key);
        }

        payouts
    }

    /// Get statistics
    pub fn get_stats(&self) -> &SharesStats {
        &self.stats
    }

    /// Simplified get_position for handlers - returns YES/NO shares for a market
    pub fn get_position(&self, wallet: &str, market_id: &str) -> SimplePosition {
        let yes_shares = self.get_shares(wallet, market_id, OutcomeIndex::YES);
        let no_shares = self.get_shares(wallet, market_id, OutcomeIndex::NO);
        SimplePosition {
            market_id: market_id.to_string(),
            yes_shares,
            no_shares,
        }
    }

    /// Get all positions for a wallet (simplified)
    pub fn get_all_positions(&self, wallet: &str) -> Vec<SimplePosition> {
        match self.balances.get(wallet) {
            Some(balance) => {
                let mut markets: std::collections::HashSet<String> = std::collections::HashSet::new();
                for (market_id, _) in balance.positions.keys() {
                    markets.insert(market_id.clone());
                }
                
                markets.into_iter().map(|market_id| {
                    self.get_position(wallet, &market_id)
                }).filter(|p| p.yes_shares > 0.0 || p.no_shares > 0.0).collect()
            }
            None => Vec::new(),
        }
    }

    /// Simplified credit_shares for handlers (no price tracking)
    pub fn credit_shares_simple(
        &mut self,
        wallet: &str,
        market_id: &str,
        outcome: OutcomeIndex,
        amount: f64,
    ) {
        self.credit_shares(wallet, market_id, outcome, amount, 0.0, ShareTxType::Mint);
    }

    /// Simplified debit_shares for handlers (no price tracking)
    pub fn debit_shares_simple(
        &mut self,
        wallet: &str,
        market_id: &str,
        outcome: OutcomeIndex,
        amount: f64,
    ) -> Result<f64, String> {
        self.debit_shares(wallet, market_id, outcome, amount, 0.0, ShareTxType::Redeem)
    }
}

/// Simple position info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplePosition {
    pub market_id: String,
    pub yes_shares: f64,
    pub no_shares: f64,
}

// ============================================================================
// API RESPONSE TYPES
// ============================================================================

/// Summary of a user's positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPositionsSummary {
    pub wallet: String,
    pub positions: Vec<PositionInfo>,
    pub total_value: f64,
    pub total_unrealized_pnl: f64,
    pub total_realized_pnl: f64,
}

impl UserPositionsSummary {
    pub fn empty(wallet: &str) -> Self {
        Self {
            wallet: wallet.to_string(),
            positions: Vec::new(),
            total_value: 0.0,
            total_unrealized_pnl: 0.0,
            total_realized_pnl: 0.0,
        }
    }
}

/// Info about a single position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub market_id: String,
    pub outcome: usize,
    pub shares: f64,
    pub avg_cost: f64,
    pub total_cost: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}

// ============================================================================
// HELPER
// ============================================================================

fn now() -> u64 {
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

    #[test]
    fn test_add_shares() {
        let mut pos = SharePosition::new("market1".to_string(), OutcomeIndex::YES, "alice".to_string());
        
        pos.add_shares(100.0, 0.50);
        assert_eq!(pos.shares, 100.0);
        assert_eq!(pos.avg_cost, 0.50);
        assert_eq!(pos.total_cost, 50.0);

        // Add more at different price
        pos.add_shares(100.0, 0.60);
        assert_eq!(pos.shares, 200.0);
        assert!((pos.avg_cost - 0.55).abs() < 0.001); // Average of 0.50 and 0.60
    }

    #[test]
    fn test_remove_shares_pnl() {
        let mut pos = SharePosition::new("market1".to_string(), OutcomeIndex::YES, "alice".to_string());
        
        pos.add_shares(100.0, 0.50); // Bought at 0.50
        pos.remove_shares(50.0, 0.70); // Sold at 0.70

        assert_eq!(pos.shares, 50.0);
        // P&L: (0.70 - 0.50) * 50 = 10.0
        assert!((pos.realized_pnl - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_shares_manager_credit_debit() {
        let mut manager = SharesManager::new();
        
        // Credit shares
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.50, ShareTxType::Buy);
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::YES), 100.0);

        // Debit shares
        let removed = manager.debit_shares("alice", "market1", OutcomeIndex::YES, 50.0, 0.60, ShareTxType::Sell).unwrap();
        assert_eq!(removed, 50.0);
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::YES), 50.0);
    }

    #[test]
    fn test_insufficient_shares() {
        let mut manager = SharesManager::new();
        
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 50.0, 0.50, ShareTxType::Buy);
        
        // Try to debit more than available
        let result = manager.debit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.50, ShareTxType::Sell);
        assert!(result.is_err());
    }

    #[test]
    fn test_market_resolution() {
        let mut manager = SharesManager::new();
        
        // Alice has YES shares, Bob has NO shares
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.60, ShareTxType::Buy);
        manager.credit_shares("bob", "market1", OutcomeIndex::NO, 100.0, 0.40, ShareTxType::Buy);

        // Resolve: YES wins
        let payouts = manager.resolve_market("market1", OutcomeIndex::YES, 2);

        // Alice should get payout, Bob should not
        assert!(payouts.iter().any(|(w, a)| w == "alice" && *a == 100.0));
        assert!(!payouts.iter().any(|(w, _)| w == "bob"));

        // All positions should be cleared
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::YES), 0.0);
        assert_eq!(manager.get_shares("bob", "market1", OutcomeIndex::NO), 0.0);
    }
}
