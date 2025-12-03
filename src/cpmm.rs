use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ============================================================================
// EVENT LIFECYCLE STATUS
// ============================================================================

/// Event/Market lifecycle status
/// 
/// Flow: Pending → Provisional → Active → Closed → Resolved
///                     ↓
///                  Refunded (if viability check fails)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    /// Event is in the inbox (from AI/scrapers), not yet launched
    /// - No trading allowed
    /// - No liquidity exists
    /// - Admin can edit/delete
    Pending,
    
    /// Market launched with initial liquidity, in 3-day probation
    /// - Trading is LIVE
    /// - Must reach 10,000 BB TVL within 72 hours
    /// - If fails viability → Refunded
    Provisional,
    
    /// Market passed viability check, fully operational
    /// - Trading is LIVE
    /// - Safe for larger trades
    /// - Guaranteed to run to resolution
    Active,
    
    /// Betting period has ended, awaiting resolution
    /// - No more trading
    /// - Waiting for oracle/admin to declare winner
    Closed,
    
    /// Market resolved with a winning outcome
    /// - Winners can redeem tokens 1:1 for BB
    /// - Losing tokens are worthless
    /// - LPs can withdraw remaining liquidity
    Resolved,
    
    /// Market failed viability check or was cancelled
    /// - All positions refunded
    /// - Market deleted from active list
    Refunded,
}

impl EventStatus {
    /// Check if trading is allowed in this status
    pub fn is_trading_open(&self) -> bool {
        matches!(self, EventStatus::Provisional | EventStatus::Active)
    }
    
    /// Check if the market is live (has been launched)
    pub fn is_live(&self) -> bool {
        !matches!(self, EventStatus::Pending)
    }
    
    /// Check if the market has ended (no more changes possible)
    pub fn is_terminal(&self) -> bool {
        matches!(self, EventStatus::Resolved | EventStatus::Refunded)
    }
    
    /// Check if liquidity can be added
    pub fn can_add_liquidity(&self) -> bool {
        matches!(self, EventStatus::Provisional | EventStatus::Active)
    }
    
    /// Get emoji for status display
    pub fn emoji(&self) -> &'static str {
        match self {
            EventStatus::Pending => "📥",
            EventStatus::Provisional => "⏳",
            EventStatus::Active => "🟢",
            EventStatus::Closed => "🔒",
            EventStatus::Resolved => "✅",
            EventStatus::Refunded => "💸",
        }
    }
}

impl fmt::Display for EventStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            EventStatus::Pending => "pending",
            EventStatus::Provisional => "provisional",
            EventStatus::Active => "active",
            EventStatus::Closed => "closed",
            EventStatus::Resolved => "resolved",
            EventStatus::Refunded => "refunded",
        };
        write!(f, "{}", status_str)
    }
}

impl Default for EventStatus {
    fn default() -> Self {
        EventStatus::Pending
    }
}

// ============================================================================
// PENDING EVENT (Inbox for AI-scraped events)
// ============================================================================

/// A pending event from AI scrapers, waiting to be launched as a market
/// 
/// These events sit in the "inbox" until a user decides to launch them
/// by providing initial liquidity. Events can expire if not launched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEvent {
    /// Unique event ID (e.g., "evt_abc123")
    pub id: String,
    
    /// Event title (e.g., "Will France win World Cup 2026?")
    pub title: String,
    
    /// Detailed description of the event
    pub description: String,
    
    /// Category: sports, crypto, politics, tech, business
    pub category: String,
    
    /// Betting options (e.g., ["Yes", "No"] or ["France", "Germany", "Brazil"])
    pub options: Vec<String>,
    
    /// AI confidence score (0.0 - 1.0)
    pub confidence: f64,
    
    /// Source URL where event was discovered
    pub source_url: String,
    
    /// Source domain (e.g., "espn.com", "coindesk.com")
    pub source_domain: String,
    
    /// When the event was scraped/added (Unix timestamp)
    pub created_at: u64,
    
    /// When this pending event expires if not launched (Unix timestamp)
    /// Events auto-delete after expiration to keep inbox clean
    pub expires_at: Option<u64>,
    
    /// Resolution date - when the real-world event occurs
    /// (e.g., "2026-07-19" for World Cup final)
    pub resolution_date: Option<String>,
    
    /// Always Pending for this struct
    pub status: EventStatus,
}

impl PendingEvent {
    /// Create a new pending event from AI scraper data
    pub fn new(
        id: String,
        title: String,
        description: String,
        category: String,
        options: Vec<String>,
        confidence: f64,
        source_url: String,
        source_domain: String,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Default expiration: 30 days from creation
        let expires_at = Some(now + 30 * 24 * 60 * 60);
        
        Self {
            id,
            title,
            description,
            category,
            options,
            confidence,
            source_url,
            source_domain,
            created_at: now,
            expires_at,
            resolution_date: None,
            status: EventStatus::Pending,
        }
    }
    
    /// Check if the event has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > expires_at
        } else {
            false
        }
    }
    
    /// Get days until expiration
    pub fn days_until_expiration(&self) -> Option<i64> {
        self.expires_at.map(|exp| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            ((exp as i64) - (now as i64)) / (24 * 60 * 60)
        })
    }
}

// ============================================================================
// CPMM CONSTANTS
// ============================================================================

/// Constant Product Market Maker (CPMM) for Prediction Markets
/// 
/// Formula: x * y = k (constant product)
/// 
/// For a binary market (Yes/No):
/// - x = YES tokens in pool
/// - y = NO tokens in pool  
/// - k = x * y (invariant that must be maintained)
///
/// Price calculation:
/// - Price(YES) = y / (x + y)
/// - Price(NO) = x / (x + y)
/// - Prices always sum to 1.0

/// Fee rate charged on each trade (2%)
pub const LP_FEE_RATE: f64 = 0.02;

/// Minimum liquidity required to launch a market
pub const MINIMUM_LAUNCH_LIQUIDITY: f64 = 1000.0;

/// TVL threshold to pass viability check (10,000 BB = $100)
pub const VIABILITY_THRESHOLD: f64 = 10000.0;

/// Viability period in seconds (72 hours = 3 days)
pub const VIABILITY_PERIOD_SECONDS: u64 = 72 * 60 * 60;

/// Constant Product Market Maker Pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CPMMPool {
    /// Tokens for each outcome in the pool
    /// For binary: [YES_tokens, NO_tokens]
    /// For multi-outcome: [A_tokens, B_tokens, C_tokens, ...]
    pub reserves: Vec<f64>,
    
    /// The constant product (k = product of all reserves)
    pub k: f64,
    
    /// Total fees collected (distributed to LPs)
    pub fees_collected: f64,
    
    /// LP shares: account -> share percentage (0.0 to 1.0)
    pub lp_shares: HashMap<String, f64>,
    
    /// Total LP tokens issued
    pub total_lp_tokens: f64,
    
    /// Outcome labels for reference
    pub outcome_labels: Vec<String>,
}

impl CPMMPool {
    /// Create a new CPMM pool with initial liquidity split evenly
    /// 
    /// # Arguments
    /// * `initial_liquidity` - Total BB tokens to seed the pool
    /// * `outcome_labels` - Labels for each outcome (e.g., ["Yes", "No"])
    /// * `initial_lp` - Account providing initial liquidity
    /// 
    /// # Returns
    /// New CPMMPool with 50/50 (or equal) split across outcomes
    pub fn new(initial_liquidity: f64, outcome_labels: Vec<String>, initial_lp: &str) -> Self {
        let num_outcomes = outcome_labels.len();
        
        // Split liquidity evenly across all outcomes
        let tokens_per_outcome = initial_liquidity / num_outcomes as f64;
        let reserves: Vec<f64> = vec![tokens_per_outcome; num_outcomes];
        
        // Calculate k = product of all reserves
        let k = reserves.iter().product();
        
        // Initial LP gets 100% of shares
        let mut lp_shares = HashMap::new();
        lp_shares.insert(initial_lp.to_string(), 1.0);
        
        Self {
            reserves,
            k,
            fees_collected: 0.0,
            lp_shares,
            total_lp_tokens: initial_liquidity, // LP tokens = initial liquidity
            outcome_labels,
        }
    }
    
    /// Calculate current price for each outcome
    /// 
    /// For binary market: Price(YES) = NO_reserve / (YES_reserve + NO_reserve)
    /// Prices always sum to 1.0
    /// 
    /// # Returns
    /// Vec of prices for each outcome (0.0 to 1.0)
    pub fn calculate_prices(&self) -> Vec<f64> {
        let total: f64 = self.reserves.iter().sum();
        if total == 0.0 {
            // Equal prices if pool is empty
            return vec![1.0 / self.reserves.len() as f64; self.reserves.len()];
        }
        
        // Price of outcome i = (total - reserve_i) / total / (n-1)
        // Simplified for binary: Price(YES) = NO / (YES + NO)
        self.reserves.iter().map(|reserve| {
            let other_reserves: f64 = total - reserve;
            other_reserves / (total * (self.reserves.len() - 1) as f64)
        }).collect()
    }
    
    /// Calculate cost to buy a specific amount of outcome tokens
    /// 
    /// Uses constant product formula: x * y = k
    /// When buying outcome i, we remove tokens from reserve i
    /// and add tokens to other reserves to maintain k
    /// 
    /// # Arguments
    /// * `outcome_index` - Which outcome to buy (0 = first, 1 = second, etc.)
    /// * `amount` - Number of outcome tokens to buy
    /// 
    /// # Returns
    /// Ok((cost_before_fee, fee, total_cost)) or Err if invalid
    pub fn calculate_cost(&self, outcome_index: usize, amount: f64) -> Result<(f64, f64, f64), String> {
        if outcome_index >= self.reserves.len() {
            return Err(format!("Invalid outcome index: {}", outcome_index));
        }
        
        if amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }
        
        let current_reserve = self.reserves[outcome_index];
        if amount >= current_reserve {
            return Err(format!(
                "Cannot buy {} tokens, only {} available in pool",
                amount, current_reserve
            ));
        }
        
        // New reserve after removing purchased tokens
        let new_reserve = current_reserve - amount;
        
        // For binary market: x * y = k
        // If buying YES: new_YES * new_NO = k
        // new_NO = k / new_YES
        // Cost = new_NO - old_NO
        
        // For multi-outcome, we need to distribute the cost across other outcomes
        // Simplified: use the product formula
        let other_reserves_product: f64 = self.reserves.iter()
            .enumerate()
            .filter(|(i, _)| *i != outcome_index)
            .map(|(_, r)| r)
            .product();
        
        // k = reserve_i * other_product
        // new_other_product = k / new_reserve_i
        let new_other_product = self.k / new_reserve;
        
        // For binary: cost = new_other - old_other
        // For multi: we need a different approach
        if self.reserves.len() == 2 {
            // Binary market - simple case
            let other_index = 1 - outcome_index;
            let old_other = self.reserves[other_index];
            let new_other = new_other_product; // For binary, other_product = other_reserve
            let cost_before_fee = new_other - old_other;
            let fee = cost_before_fee * LP_FEE_RATE;
            let total_cost = cost_before_fee + fee;
            
            Ok((cost_before_fee, fee, total_cost))
        } else {
            // Multi-outcome market - use approximation
            // Cost ≈ amount * price / (1 - price)
            let prices = self.calculate_prices();
            let price = prices[outcome_index];
            let cost_before_fee = amount * price / (1.0 - price).max(0.01);
            let fee = cost_before_fee * LP_FEE_RATE;
            let total_cost = cost_before_fee + fee;
            
            Ok((cost_before_fee, fee, total_cost))
        }
    }
    
    /// Execute a swap (buy outcome tokens)
    /// 
    /// # Arguments
    /// * `outcome_index` - Which outcome to buy
    /// * `amount` - Number of outcome tokens to buy
    /// * `max_cost` - Maximum BB willing to pay (slippage protection)
    /// 
    /// # Returns
    /// Ok(SwapResult) with details, or Err if failed
    pub fn swap(&mut self, outcome_index: usize, amount: f64, max_cost: Option<f64>) -> Result<SwapResult, String> {
        // Calculate cost
        let (cost_before_fee, fee, total_cost) = self.calculate_cost(outcome_index, amount)?;
        
        // Check slippage
        if let Some(max) = max_cost {
            if total_cost > max {
                return Err(format!(
                    "Cost {} exceeds max_cost {}. Reduce amount or increase slippage tolerance.",
                    total_cost, max
                ));
            }
        }
        
        // Execute the swap
        let old_reserve = self.reserves[outcome_index];
        self.reserves[outcome_index] -= amount;
        
        // For binary market, add cost to other reserve
        if self.reserves.len() == 2 {
            let other_index = 1 - outcome_index;
            self.reserves[other_index] += cost_before_fee;
        } else {
            // For multi-outcome, distribute proportionally
            let total_other: f64 = self.reserves.iter()
                .enumerate()
                .filter(|(i, _)| *i != outcome_index)
                .map(|(_, r)| r)
                .sum();
            
            for (i, reserve) in self.reserves.iter_mut().enumerate() {
                if i != outcome_index && total_other > 0.0 {
                    *reserve += cost_before_fee * (*reserve / total_other);
                }
            }
        }
        
        // Collect fees
        self.fees_collected += fee;
        
        // Recalculate k (should be approximately the same, but recalc for precision)
        self.k = self.reserves.iter().product();
        
        let new_prices = self.calculate_prices();
        
        Ok(SwapResult {
            outcome_index,
            outcome_label: self.outcome_labels[outcome_index].clone(),
            tokens_bought: amount,
            cost_before_fee,
            fee,
            total_cost,
            old_price: old_reserve / self.reserves.iter().sum::<f64>(),
            new_price: new_prices[outcome_index],
            new_prices,
        })
    }
    
    /// Add liquidity to the pool (become an LP)
    /// 
    /// # Arguments
    /// * `account` - Account adding liquidity
    /// * `amount` - BB tokens to add
    /// 
    /// # Returns
    /// Ok(LP shares received) or Err
    pub fn add_liquidity(&mut self, account: &str, amount: f64) -> Result<f64, String> {
        if amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }
        
        // Calculate share of pool this deposit represents
        let current_tvl = self.get_tvl();
        let share_of_new_liquidity = if current_tvl > 0.0 {
            amount / (current_tvl + amount)
        } else {
            1.0 // First LP gets 100%
        };
        
        // Add tokens proportionally to each reserve
        let tokens_per_outcome = amount / self.reserves.len() as f64;
        for reserve in &mut self.reserves {
            *reserve += tokens_per_outcome;
        }
        
        // Recalculate k
        self.k = self.reserves.iter().product();
        
        // Update total LP tokens
        let new_lp_tokens = self.total_lp_tokens * share_of_new_liquidity / (1.0 - share_of_new_liquidity);
        self.total_lp_tokens += new_lp_tokens;
        
        // Dilute existing LP shares and add new LP
        let dilution_factor = 1.0 - share_of_new_liquidity;
        for share in self.lp_shares.values_mut() {
            *share *= dilution_factor;
        }
        
        let current_share = self.lp_shares.entry(account.to_string()).or_insert(0.0);
        *current_share += share_of_new_liquidity;
        
        Ok(share_of_new_liquidity)
    }
    
    /// Remove liquidity from the pool (LP exits)
    /// 
    /// # Arguments
    /// * `account` - Account removing liquidity
    /// * `share_to_remove` - Fraction of their share to remove (0.0 to 1.0)
    /// 
    /// # Returns
    /// Ok(BB tokens returned) or Err
    pub fn remove_liquidity(&mut self, account: &str, share_to_remove: f64) -> Result<f64, String> {
        if share_to_remove <= 0.0 || share_to_remove > 1.0 {
            return Err("Share to remove must be between 0 and 1".to_string());
        }
        
        let account_share = *self.lp_shares.get(account).unwrap_or(&0.0);
        if account_share <= 0.0 {
            return Err("Account has no LP shares".to_string());
        }
        
        let share_being_removed = account_share * share_to_remove;
        
        // Calculate tokens to return
        let tvl = self.get_tvl();
        let tokens_to_return = tvl * share_being_removed;
        
        // Remove proportionally from each reserve
        let tokens_per_outcome = tokens_to_return / self.reserves.len() as f64;
        for reserve in &mut self.reserves {
            *reserve -= tokens_per_outcome;
        }
        
        // Recalculate k
        self.k = self.reserves.iter().product();
        
        // Update LP tokens
        self.total_lp_tokens -= self.total_lp_tokens * share_being_removed;
        
        // Update shares
        if share_to_remove >= 1.0 {
            self.lp_shares.remove(account);
        } else {
            if let Some(share) = self.lp_shares.get_mut(account) {
                *share -= share_being_removed;
            }
        }
        
        // Redistribute remaining shares proportionally
        let total_remaining: f64 = self.lp_shares.values().sum();
        if total_remaining > 0.0 && total_remaining < 1.0 {
            let adjustment = 1.0 / total_remaining;
            for share in self.lp_shares.values_mut() {
                *share *= adjustment;
            }
        }
        
        Ok(tokens_to_return)
    }
    
    /// Get Total Value Locked (TVL) in the pool
    pub fn get_tvl(&self) -> f64 {
        self.reserves.iter().sum()
    }
    
    /// Get LP share for a specific account
    pub fn get_lp_share(&self, account: &str) -> f64 {
        *self.lp_shares.get(account).unwrap_or(&0.0)
    }
    
    /// Get pending fees for a specific LP
    pub fn get_pending_fees(&self, account: &str) -> f64 {
        let share = self.get_lp_share(account);
        self.fees_collected * share
    }
    
    /// Distribute collected fees to LP reserves (compounds liquidity)
    pub fn compound_fees(&mut self) {
        if self.fees_collected > 0.0 {
            let fee_per_outcome = self.fees_collected / self.reserves.len() as f64;
            for reserve in &mut self.reserves {
                *reserve += fee_per_outcome;
            }
            self.k = self.reserves.iter().product();
            self.fees_collected = 0.0;
        }
    }
}

/// Result of a swap operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    pub outcome_index: usize,
    pub outcome_label: String,
    pub tokens_bought: f64,
    pub cost_before_fee: f64,
    pub fee: f64,
    pub total_cost: f64,
    pub old_price: f64,
    pub new_price: f64,
    pub new_prices: Vec<f64>,
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_pool_binary() {
        let pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        assert_eq!(pool.reserves.len(), 2);
        assert_eq!(pool.reserves[0], 500.0);
        assert_eq!(pool.reserves[1], 500.0);
        assert_eq!(pool.k, 250000.0); // 500 * 500
        assert_eq!(pool.get_lp_share("ALICE"), 1.0);
        assert_eq!(pool.get_tvl(), 1000.0);
    }
    
    #[test]
    fn test_calculate_prices_equal() {
        let pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        let prices = pool.calculate_prices();
        
        assert_eq!(prices.len(), 2);
        assert!((prices[0] - 0.5).abs() < 0.001);
        assert!((prices[1] - 0.5).abs() < 0.001);
    }
    
    #[test]
    fn test_calculate_cost() {
        let pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // Buy 100 YES tokens
        let result = pool.calculate_cost(0, 100.0);
        assert!(result.is_ok());
        
        let (cost, fee, total) = result.unwrap();
        assert!(cost > 0.0);
        assert!((fee - cost * LP_FEE_RATE).abs() < 0.001);
        assert!((total - (cost + fee)).abs() < 0.001);
    }
    
    #[test]
    fn test_swap_moves_price() {
        let mut pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        let old_prices = pool.calculate_prices();
        
        // Buy 100 YES tokens
        let result = pool.swap(0, 100.0, None);
        assert!(result.is_ok());
        
        let swap = result.unwrap();
        let new_prices = pool.calculate_prices();
        
        // YES price should increase (more people buying YES)
        assert!(new_prices[0] > old_prices[0]);
        // NO price should decrease
        assert!(new_prices[1] < old_prices[1]);
        // Prices still sum to ~1.0
        assert!((new_prices[0] + new_prices[1] - 1.0).abs() < 0.01);
        
        // Fee was collected
        assert!(pool.fees_collected > 0.0);
    }
    
    #[test]
    fn test_slippage_protection() {
        let mut pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // Try to buy with too low max_cost
        let result = pool.swap(0, 100.0, Some(1.0)); // Max cost $1 is way too low
        assert!(result.is_err());
    }
    
    #[test]
    fn test_cannot_buy_more_than_reserve() {
        let pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // Try to buy more than exists
        let result = pool.calculate_cost(0, 600.0); // Only 500 in reserve
        assert!(result.is_err());
    }
    
    #[test]
    fn test_add_liquidity() {
        let mut pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // BOB adds 1000 BB liquidity
        let result = pool.add_liquidity("BOB", 1000.0);
        assert!(result.is_ok());
        
        // TVL doubled
        assert!((pool.get_tvl() - 2000.0).abs() < 0.01);
        
        // Each LP should have ~50% share
        assert!((pool.get_lp_share("ALICE") - 0.5).abs() < 0.01);
        assert!((pool.get_lp_share("BOB") - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_remove_liquidity() {
        let mut pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // ALICE removes half her liquidity
        let result = pool.remove_liquidity("ALICE", 0.5);
        assert!(result.is_ok());
        
        let returned = result.unwrap();
        assert!((returned - 500.0).abs() < 0.01);
        assert!((pool.get_tvl() - 500.0).abs() < 0.01);
    }
    
    #[test]
    fn test_multi_outcome_pool() {
        let pool = CPMMPool::new(
            3000.0, 
            vec!["France".to_string(), "Germany".to_string(), "Brazil".to_string()], 
            "ALICE"
        );
        
        assert_eq!(pool.reserves.len(), 3);
        assert_eq!(pool.reserves[0], 1000.0);
        assert_eq!(pool.reserves[1], 1000.0);
        assert_eq!(pool.reserves[2], 1000.0);
        assert_eq!(pool.get_tvl(), 3000.0);
        
        let prices = pool.calculate_prices();
        // Each outcome should have ~33% probability
        for price in &prices {
            assert!((price - 0.333).abs() < 0.01);
        }
    }
    
    #[test]
    fn test_fee_collection() {
        let mut pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        assert_eq!(pool.fees_collected, 0.0);
        
        // Execute a trade
        let _ = pool.swap(0, 50.0, None);
        
        // Fees should be collected
        assert!(pool.fees_collected > 0.0);
        
        // ALICE (100% LP) should get all fees
        assert!((pool.get_pending_fees("ALICE") - pool.fees_collected).abs() < 0.001);
    }
    
    #[test]
    fn test_price_impact_increases_with_size() {
        let pool = CPMMPool::new(1000.0, vec!["Yes".to_string(), "No".to_string()], "ALICE");
        
        // Small trade
        let (small_cost, _, _) = pool.calculate_cost(0, 10.0).unwrap();
        let small_price_per_token = small_cost / 10.0;
        
        // Large trade
        let (large_cost, _, _) = pool.calculate_cost(0, 100.0).unwrap();
        let large_price_per_token = large_cost / 100.0;
        
        // Larger trades should have worse price per token (slippage)
        assert!(large_price_per_token > small_price_per_token);
    }
}
