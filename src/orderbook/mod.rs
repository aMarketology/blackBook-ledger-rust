// ============================================================================
// Order Book Module - BlackBook Prediction Market
// ============================================================================
//
// Central Limit Order Book (CLOB) system for the prediction market.
// Provides professional-grade order matching with maker-taker fee model.
//
// Architecture:
//   - CLOB Primary: Liquid markets use order book matching
//   - CPMM Fallback: Illiquid markets fall back to AMM pricing
//
// Dynamic Odds:
//   - Derived from best bid/ask spread on the order book
//   - YES price = best_ask (cost to buy YES shares)
//   - NO price = 1 - YES price = best_bid (cost to buy NO shares)
//   - More liquidity = tighter spread = more accurate pricing
//
// ============================================================================

pub mod orders;
pub mod matching;

pub use orders::*;
pub use matching::*;

use crate::market_resolve::cpmm::CPMMPool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum liquidity depth (in BB) before falling back to CPMM
pub const MIN_CLOB_DEPTH: f64 = 100.0;

/// Spread threshold (in bps) above which we consider market illiquid
pub const MAX_SPREAD_BPS: u64 = 20; // 20% spread = illiquid

/// Number of price levels to consider for liquidity check
pub const DEPTH_CHECK_LEVELS: usize = 5;

// ============================================================================
// ORDER BOOK MANAGER
// ============================================================================

/// Manages all order books and provides hybrid CLOB/CPMM pricing
#[derive(Debug)]
pub struct OrderBookManager {
    /// The matching engine
    pub engine: MatchingEngine,
    
    /// CPMM pools for illiquid markets (fallback)
    pub cpmm_pools: HashMap<String, CPMMPool>,
    
    /// Market status tracking
    pub market_status: HashMap<String, MarketOrderBookStatus>,
    
    /// Total fees collected
    pub total_fees_collected: f64,
    
    /// Statistics
    pub stats: OrderBookStats,
}

/// Status of a market's order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrderBookStatus {
    pub market_id: String,
    /// Whether CLOB has sufficient liquidity
    pub clob_active: bool,
    /// Whether using CPMM fallback
    pub using_cpmm: bool,
    /// Current spread in basis points (None if no orders)
    pub spread_bps: Option<u64>,
    /// Total bid depth
    pub bid_depth: f64,
    /// Total ask depth
    pub ask_depth: f64,
    /// Last update timestamp
    pub last_update: u64,
}

/// Order book statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderBookStats {
    pub total_orders_submitted: u64,
    pub total_orders_filled: u64,
    pub total_orders_cancelled: u64,
    pub total_volume_traded: f64,
    pub total_fees_collected: f64,
    pub markets_with_clob: usize,
    pub markets_with_cpmm_fallback: usize,
}

impl OrderBookManager {
    pub fn new() -> Self {
        Self {
            engine: MatchingEngine::new(),
            cpmm_pools: HashMap::new(),
            market_status: HashMap::new(),
            total_fees_collected: 0.0,
            stats: OrderBookStats::default(),
        }
    }

    /// Initialize order book for a new market
    pub fn init_market(&mut self, market_id: &str, initial_liquidity: Option<f64>) {
        // Create CPMM pool as fallback
        if let Some(liquidity) = initial_liquidity {
            if liquidity > 0.0 {
                let outcomes = vec!["YES".to_string(), "NO".to_string()];
                let pool = CPMMPool::new(liquidity, outcomes, "HOUSE");
                self.cpmm_pools.insert(market_id.to_string(), pool);
            }
        }

        // Initialize status
        self.market_status.insert(market_id.to_string(), MarketOrderBookStatus {
            market_id: market_id.to_string(),
            clob_active: false,
            using_cpmm: initial_liquidity.is_some(),
            spread_bps: None,
            bid_depth: 0.0,
            ask_depth: 0.0,
            last_update: now(),
        });

        if initial_liquidity.is_some() {
            self.stats.markets_with_cpmm_fallback += 1;
        }
    }

    /// Submit an order (routes to CLOB or CPMM based on liquidity)
    pub fn submit_order(&mut self, order: LimitOrder) -> OrderSubmitResult {
        let market_id = order.market_id.clone();
        
        // Update stats
        self.stats.total_orders_submitted += 1;

        // Submit to matching engine
        let match_result = self.engine.submit_order(order);

        // Update fees
        self.total_fees_collected += match_result.total_fees;
        self.stats.total_fees_collected += match_result.total_fees;

        if !match_result.fills.is_empty() {
            self.stats.total_orders_filled += 1;
            self.stats.total_volume_traded += match_result.total_filled;
        }

        // Update market status
        self.update_market_status(&market_id);

        OrderSubmitResult {
            success: true,
            order: match_result.order,
            fills: match_result.fills,
            added_to_book: match_result.added_to_book,
            total_filled: match_result.total_filled,
            fees_paid: match_result.total_fees,
            used_cpmm: false,
            error: None,
        }
    }

    /// Execute a market order using best available pricing (CLOB or CPMM)
    pub fn execute_market_order(
        &mut self,
        market_id: &str,
        outcome: Outcome,
        side: Side,
        size: f64,
        maker: &str,
        signature: &str,
    ) -> OrderSubmitResult {
        // Check CLOB liquidity
        let snapshot = self.engine.get_depth(market_id, outcome, DEPTH_CHECK_LEVELS);
        let has_clob_liquidity = self.has_sufficient_liquidity(&snapshot, size);

        if has_clob_liquidity {
            // Use CLOB
            let order = match LimitOrder::market_order(
                market_id.to_string(),
                outcome,
                side,
                size,
                maker.to_string(),
                signature.to_string(),
            ) {
                Ok(o) => o,
                Err(e) => return OrderSubmitResult::error(format!("{}", e)),
            };

            self.submit_order(order)
        } else {
            // Fall back to CPMM
            self.execute_cpmm_trade(market_id, outcome, side, size, maker, signature)
        }
    }

    /// Execute trade against CPMM pool
    fn execute_cpmm_trade(
        &mut self,
        market_id: &str,
        outcome: Outcome,
        side: Side,
        size: f64,
        maker: &str,
        signature: &str,
    ) -> OrderSubmitResult {
        let pool = match self.cpmm_pools.get_mut(market_id) {
            Some(p) => p,
            None => return OrderSubmitResult::error("No CPMM pool and insufficient CLOB liquidity".to_string()),
        };

        // Convert to CPMM trade
        // Side::Bid = buying YES = CPMM buy outcome 0
        // Side::Ask = selling YES = CPMM sell outcome 0 (buy outcome 1)
        let cpmm_outcome = outcome.index();
        
        match side {
            Side::Bid => {
                // Buying shares - execute swap
                match pool.swap(cpmm_outcome, size, None) {
                    Ok(swap) => {
                        let fill = Fill {
                            id: format!("cpmm_fill_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
                            market_id: market_id.to_string(),
                            outcome,
                            maker_order_id: "CPMM_POOL".to_string(),
                            taker_order_id: format!("market_order_{}", now()),
                            maker: "CPMM_POOL".to_string(),
                            taker: maker.to_string(),
                            price_bps: (swap.new_price * 100.0) as u64,
                            size,
                            value: swap.total_cost,
                            maker_fee: 0.0,
                            taker_fee: swap.fee,
                            timestamp: now(),
                            taker_side: side,
                        };

                        OrderSubmitResult {
                            success: true,
                            order: LimitOrder::market_order(
                                market_id.to_string(),
                                outcome,
                                side,
                                size,
                                maker.to_string(),
                                signature.to_string(),
                            ).unwrap(),
                            fills: vec![fill],
                            added_to_book: false,
                            total_filled: size,
                            fees_paid: swap.fee,
                            used_cpmm: true,
                            error: None,
                        }
                    }
                    Err(e) => OrderSubmitResult::error(format!("CPMM swap failed: {}", e)),
                }
            }
            Side::Ask => {
                // Selling shares - user is selling YES (buying NO)
                // This is equivalent to buying the opposite outcome
                let opposite_outcome = if cpmm_outcome == 0 { 1 } else { 0 };
                
                match pool.swap(opposite_outcome, size, None) {
                    Ok(swap) => {
                        let fill = Fill {
                            id: format!("cpmm_fill_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
                            market_id: market_id.to_string(),
                            outcome,
                            maker_order_id: "CPMM_POOL".to_string(),
                            taker_order_id: format!("market_order_{}", now()),
                            maker: "CPMM_POOL".to_string(),
                            taker: maker.to_string(),
                            price_bps: ((1.0 - swap.new_price) * 100.0) as u64,
                            size,
                            value: swap.total_cost,
                            maker_fee: 0.0,
                            taker_fee: swap.fee,
                            timestamp: now(),
                            taker_side: side,
                        };

                        OrderSubmitResult {
                            success: true,
                            order: LimitOrder::market_order(
                                market_id.to_string(),
                                outcome,
                                side,
                                size,
                                maker.to_string(),
                                signature.to_string(),
                            ).unwrap(),
                            fills: vec![fill],
                            added_to_book: false,
                            total_filled: size,
                            fees_paid: swap.fee,
                            used_cpmm: true,
                            error: None,
                        }
                    }
                    Err(e) => OrderSubmitResult::error(format!("CPMM swap failed: {}", e)),
                }
            }
        }
    }

    /// Check if CLOB has sufficient liquidity for a trade
    fn has_sufficient_liquidity(&self, snapshot: &OrderBookSnapshot, size: f64) -> bool {
        // Check if there's enough depth
        let bid_depth: f64 = snapshot.bids.iter().map(|l| l.size).sum();
        let ask_depth: f64 = snapshot.asks.iter().map(|l| l.size).sum();

        // Check spread
        let spread_ok = snapshot.spread.map(|s| s <= MAX_SPREAD_BPS).unwrap_or(false);

        // Check depth
        let depth_ok = bid_depth >= MIN_CLOB_DEPTH && ask_depth >= MIN_CLOB_DEPTH;

        // Check if we can fill the order
        let can_fill = match size {
            s if s > 0.0 => bid_depth >= s || ask_depth >= s,
            _ => true,
        };

        spread_ok && depth_ok && can_fill
    }

    /// Update market status based on current order book state
    fn update_market_status(&mut self, market_id: &str) {
        let snapshot = self.engine.get_depth(market_id, Outcome::YES, DEPTH_CHECK_LEVELS);
        
        let bid_depth: f64 = snapshot.bids.iter().map(|l| l.size).sum();
        let ask_depth: f64 = snapshot.asks.iter().map(|l| l.size).sum();
        
        let clob_active = bid_depth >= MIN_CLOB_DEPTH && ask_depth >= MIN_CLOB_DEPTH
            && snapshot.spread.map(|s| s <= MAX_SPREAD_BPS).unwrap_or(false);

        let using_cpmm = !clob_active && self.cpmm_pools.contains_key(market_id);

        let status = MarketOrderBookStatus {
            market_id: market_id.to_string(),
            clob_active,
            using_cpmm,
            spread_bps: snapshot.spread,
            bid_depth,
            ask_depth,
            last_update: now(),
        };

        // Update stats
        let was_clob = self.market_status.get(market_id).map(|s| s.clob_active).unwrap_or(false);
        if clob_active && !was_clob {
            self.stats.markets_with_clob += 1;
            if using_cpmm {
                self.stats.markets_with_cpmm_fallback = self.stats.markets_with_cpmm_fallback.saturating_sub(1);
            }
        } else if !clob_active && was_clob {
            self.stats.markets_with_clob = self.stats.markets_with_clob.saturating_sub(1);
            if using_cpmm {
                self.stats.markets_with_cpmm_fallback += 1;
            }
        }

        self.market_status.insert(market_id.to_string(), status);
    }

    /// Cancel an order
    pub fn cancel_order(&mut self, order_id: &str, requester: &str) -> Result<LimitOrder, OrderError> {
        self.stats.total_orders_cancelled += 1;
        self.engine.cancel_order(order_id, requester)
    }

    /// Get current odds for a market (from CLOB spread or CPMM)
    pub fn get_odds(&self, market_id: &str) -> MarketOdds {
        // Try CLOB first
        let snapshot = self.engine.get_depth(market_id, Outcome::YES, 1);
        
        if let (Some(bid), Some(ask)) = (snapshot.best_bid, snapshot.best_ask) {
            // CLOB has liquidity - derive odds from spread
            let mid = (bid + ask) as f64 / 2.0 / 100.0;
            
            return MarketOdds {
                market_id: market_id.to_string(),
                source: OddsSource::CLOB,
                yes_price: ask as f64 / 100.0,
                no_price: (100 - bid) as f64 / 100.0,
                yes_probability: mid,
                no_probability: 1.0 - mid,
                spread_bps: Some(ask - bid),
                liquidity: snapshot.bids.iter().map(|l| l.size).sum::<f64>()
                    + snapshot.asks.iter().map(|l| l.size).sum::<f64>(),
                timestamp: now(),
            };
        }

        // Fall back to CPMM
        if let Some(pool) = self.cpmm_pools.get(market_id) {
            let prices = pool.calculate_prices();
            if prices.len() >= 2 {
                return MarketOdds {
                    market_id: market_id.to_string(),
                    source: OddsSource::CPMM,
                    yes_price: prices[0],
                    no_price: prices[1],
                    yes_probability: prices[0],
                    no_probability: prices[1],
                    spread_bps: None, // CPMM has no spread
                    liquidity: pool.get_tvl(),
                    timestamp: now(),
                };
            }
        }

        // No pricing available
        MarketOdds {
            market_id: market_id.to_string(),
            source: OddsSource::None,
            yes_price: 0.5,
            no_price: 0.5,
            yes_probability: 0.5,
            no_probability: 0.5,
            spread_bps: None,
            liquidity: 0.0,
            timestamp: now(),
        }
    }

    /// Get order book depth for a market
    pub fn get_orderbook(&self, market_id: &str, outcome: Outcome, levels: usize) -> OrderBookSnapshot {
        self.engine.get_depth(market_id, outcome, levels)
    }

    /// Get user's open orders
    pub fn get_user_orders(&self, wallet: &str) -> Vec<&LimitOrder> {
        self.engine.get_user_orders(wallet)
    }

    /// Get recent trades for a market
    pub fn get_recent_trades(&self, market_id: &str, limit: usize) -> Vec<&Fill> {
        self.engine.get_market_fills(market_id, limit)
    }

    /// Add CPMM liquidity (for market makers)
    pub fn add_cpmm_liquidity(&mut self, market_id: &str, amount: f64, provider: &str) -> Result<f64, String> {
        let outcomes = vec!["YES".to_string(), "NO".to_string()];
        let pool = self.cpmm_pools.entry(market_id.to_string())
            .or_insert_with(|| CPMMPool::new(0.0, outcomes, "HOUSE"));

        match pool.add_liquidity(provider, amount) {
            Ok(lp_tokens) => {
                self.update_market_status(market_id);
                Ok(lp_tokens)
            }
            Err(e) => Err(e),
        }
    }

    /// Remove CPMM liquidity
    pub fn remove_cpmm_liquidity(&mut self, market_id: &str, share_to_remove: f64, provider: &str) -> Result<f64, String> {
        let pool = self.cpmm_pools.get_mut(market_id)
            .ok_or_else(|| "Market not found".to_string())?;

        match pool.remove_liquidity(provider, share_to_remove) {
            Ok(amount) => {
                self.update_market_status(market_id);
                Ok(amount)
            }
            Err(e) => Err(e),
        }
    }

    /// Get statistics
    pub fn get_stats(&self) -> &OrderBookStats {
        &self.stats
    }
}

// ============================================================================
// RESULT TYPES
// ============================================================================

/// Result of submitting an order
#[derive(Debug, Clone, Serialize)]
pub struct OrderSubmitResult {
    pub success: bool,
    pub order: LimitOrder,
    pub fills: Vec<Fill>,
    pub added_to_book: bool,
    pub total_filled: f64,
    pub fees_paid: f64,
    pub used_cpmm: bool,
    pub error: Option<String>,
}

impl OrderSubmitResult {
    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            order: LimitOrder {
                id: String::new(),
                market_id: String::new(),
                outcome: Outcome::YES,
                side: Side::Bid,
                price_bps: 0,
                size: 0.0,
                filled: 0.0,
                remaining: 0.0,
                order_type: OrderType::GTC,
                status: OrderStatus::Rejected,
                maker: String::new(),
                signature: String::new(),
                created_at: 0,
                updated_at: 0,
                expires_at: None,
                avg_fill_price: None,
                fees_paid: 0.0,
            },
            fills: Vec::new(),
            added_to_book: false,
            total_filled: 0.0,
            fees_paid: 0.0,
            used_cpmm: false,
            error: Some(msg),
        }
    }
}

/// Source of odds/pricing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OddsSource {
    /// Odds derived from CLOB spread
    CLOB,
    /// Odds from CPMM pool
    CPMM,
    /// No pricing available
    None,
}

/// Current market odds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOdds {
    pub market_id: String,
    pub source: OddsSource,
    /// Cost to buy 1 YES share
    pub yes_price: f64,
    /// Cost to buy 1 NO share  
    pub no_price: f64,
    /// Implied YES probability (mid-price)
    pub yes_probability: f64,
    /// Implied NO probability
    pub no_probability: f64,
    /// Spread in basis points (CLOB only)
    pub spread_bps: Option<u64>,
    /// Total liquidity available
    pub liquidity: f64,
    /// Timestamp
    pub timestamp: u64,
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
    fn test_init_market_with_cpmm() {
        let mut manager = OrderBookManager::new();
        manager.init_market("test_market", Some(10000.0));

        assert!(manager.cpmm_pools.contains_key("test_market"));
        
        let status = manager.market_status.get("test_market").unwrap();
        assert!(status.using_cpmm);
        assert!(!status.clob_active);
    }

    #[test]
    fn test_get_odds_from_cpmm() {
        let mut manager = OrderBookManager::new();
        manager.init_market("test_market", Some(10000.0));

        let odds = manager.get_odds("test_market");
        assert_eq!(odds.source, OddsSource::CPMM);
        assert!((odds.yes_probability - 0.5).abs() < 0.01); // Should be ~50/50 initially
    }

    #[test]
    fn test_submit_limit_order() {
        let mut manager = OrderBookManager::new();
        manager.init_market("test_market", Some(1000.0));

        let order = LimitOrder::new(
            "test_market".to_string(),
            Outcome::YES,
            Side::Bid,
            50,
            100.0,
            OrderType::GTC,
            "alice".to_string(),
            "sig".to_string(),
        ).unwrap();

        let result = manager.submit_order(order);
        assert!(result.success);
        assert!(result.added_to_book);
        assert!(result.fills.is_empty());
    }

    #[test]
    fn test_hybrid_pricing() {
        let mut manager = OrderBookManager::new();
        manager.init_market("test_market", Some(1000.0));

        // Initially uses CPMM (no CLOB liquidity)
        let odds1 = manager.get_odds("test_market");
        assert_eq!(odds1.source, OddsSource::CPMM);

        // Add substantial CLOB liquidity
        for i in 0..10 {
            let bid = LimitOrder::new(
                "test_market".to_string(),
                Outcome::YES,
                Side::Bid,
                45 - i,
                50.0,
                OrderType::GTC,
                format!("bidder_{}", i),
                "sig".to_string(),
            ).unwrap();
            manager.submit_order(bid);

            let ask = LimitOrder::new(
                "test_market".to_string(),
                Outcome::YES,
                Side::Ask,
                55 + i,
                50.0,
                OrderType::GTC,
                format!("asker_{}", i),
                "sig".to_string(),
            ).unwrap();
            manager.submit_order(ask);
        }

        // Now should use CLOB
        let odds2 = manager.get_odds("test_market");
        assert_eq!(odds2.source, OddsSource::CLOB);
        assert!(odds2.spread_bps.is_some());
    }
}
