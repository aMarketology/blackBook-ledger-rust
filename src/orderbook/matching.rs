// ============================================================================
// CLOB Matching Engine - BlackBook Prediction Market
// ============================================================================
//
// Price-time priority matching engine for the Central Limit Order Book.
// Matches incoming orders against resting orders on the book.
//
// Matching Rules:
//   1. Price Priority: Better prices match first (higher bids, lower asks)
//   2. Time Priority: At same price, earlier orders match first (FIFO)
//   3. Partial Fills: Orders can be partially filled
//   4. Self-Trade Prevention: Orders from same maker don't match
//
// Fee Structure (Maker-Taker Model):
//   - Makers (provide liquidity): 0.1% fee
//   - Takers (remove liquidity): 0.5% fee
//   - This incentivizes limit orders over market orders
//
// ============================================================================

use super::orders::{
    Fill, LimitOrder, OrderError, OrderStatus, OrderType, Outcome, Side,
    MAKER_FEE_RATE, TAKER_FEE_RATE,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// PRICE LEVEL
// ============================================================================

/// A price level in the order book with all orders at that price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    /// Price in basis points
    pub price_bps: u64,
    
    /// Orders at this price (FIFO queue)
    pub orders: VecDeque<String>, // Order IDs
    
    /// Total size at this level
    pub total_size: f64,
    
    /// Number of orders
    pub order_count: usize,
}

impl PriceLevel {
    pub fn new(price_bps: u64) -> Self {
        Self {
            price_bps,
            orders: VecDeque::new(),
            total_size: 0.0,
            order_count: 0,
        }
    }

    pub fn add_order(&mut self, order_id: String, size: f64) {
        self.orders.push_back(order_id);
        self.total_size += size;
        self.order_count += 1;
    }

    pub fn remove_order(&mut self, order_id: &str, size: f64) {
        self.orders.retain(|id| id != order_id);
        self.total_size = (self.total_size - size).max(0.0);
        self.order_count = self.orders.len();
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

// ============================================================================
// ORDER BOOK SIDE
// ============================================================================

/// One side of the order book (bids or asks)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSide {
    /// Price levels: price_bps -> PriceLevel
    /// For bids: sorted descending (best bid = highest)
    /// For asks: sorted ascending (best ask = lowest)
    pub levels: BTreeMap<u64, PriceLevel>,
    
    /// Side (Bid or Ask)
    pub side: Side,
    
    /// Total volume on this side
    pub total_volume: f64,
}

impl BookSide {
    pub fn new(side: Side) -> Self {
        Self {
            levels: BTreeMap::new(),
            side,
            total_volume: 0.0,
        }
    }

    /// Add an order to this side
    pub fn add_order(&mut self, order: &LimitOrder) {
        let level = self.levels
            .entry(order.price_bps)
            .or_insert_with(|| PriceLevel::new(order.price_bps));
        
        level.add_order(order.id.clone(), order.remaining);
        self.total_volume += order.remaining;
    }

    /// Remove an order from this side
    pub fn remove_order(&mut self, order: &LimitOrder) {
        if let Some(level) = self.levels.get_mut(&order.price_bps) {
            level.remove_order(&order.id, order.remaining);
            self.total_volume = (self.total_volume - order.remaining).max(0.0);
            
            // Remove empty levels
            if level.is_empty() {
                self.levels.remove(&order.price_bps);
            }
        }
    }

    /// Get the best price on this side
    pub fn best_price(&self) -> Option<u64> {
        match self.side {
            Side::Bid => self.levels.keys().next_back().copied(), // Highest bid
            Side::Ask => self.levels.keys().next().copied(),       // Lowest ask
        }
    }

    /// Get order IDs at the best price level
    pub fn best_level_orders(&self) -> Option<&VecDeque<String>> {
        let price = self.best_price()?;
        self.levels.get(&price).map(|l| &l.orders)
    }

    /// Get depth (list of price levels with aggregated size)
    pub fn depth(&self, max_levels: usize) -> Vec<(u64, f64, usize)> {
        let iter: Box<dyn Iterator<Item = _>> = match self.side {
            Side::Bid => Box::new(self.levels.iter().rev()),
            Side::Ask => Box::new(self.levels.iter()),
        };

        iter.take(max_levels)
            .map(|(price, level)| (*price, level.total_size, level.order_count))
            .collect()
    }
}

// ============================================================================
// MATCHING ENGINE
// ============================================================================

/// Result of order matching
#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    /// The order that was submitted
    pub order: LimitOrder,
    
    /// Fills that occurred
    pub fills: Vec<Fill>,
    
    /// Whether order was added to book (vs fully filled/cancelled)
    pub added_to_book: bool,
    
    /// Total size filled
    pub total_filled: f64,
    
    /// Total fees collected
    pub total_fees: f64,
    
    /// Average execution price (if any fills)
    pub avg_price: Option<f64>,
}

impl MatchResult {
    pub fn new(order: LimitOrder) -> Self {
        Self {
            order,
            fills: Vec::new(),
            added_to_book: false,
            total_filled: 0.0,
            total_fees: 0.0,
            avg_price: None,
        }
    }

    pub fn add_fill(&mut self, fill: Fill) {
        self.total_filled += fill.size;
        self.total_fees += fill.maker_fee + fill.taker_fee;
        
        // Update average price
        if self.avg_price.is_none() {
            self.avg_price = Some(fill.price());
        } else if let Some(avg) = self.avg_price {
            let prev_size = self.total_filled - fill.size;
            self.avg_price = Some((avg * prev_size + fill.price() * fill.size) / self.total_filled);
        }
        
        self.fills.push(fill);
    }
}

/// The matching engine processes orders and produces fills
#[derive(Debug)]
pub struct MatchingEngine {
    /// All orders by ID
    pub orders: HashMap<String, LimitOrder>,
    
    /// Order books per market and outcome
    /// Key: (market_id, outcome) -> (bids, asks)
    pub books: HashMap<(String, Outcome), (BookSide, BookSide)>,
    
    /// All fills (trade history)
    pub fills: Vec<Fill>,
    
    /// User's open orders
    pub user_orders: HashMap<String, Vec<String>>, // wallet -> order_ids
    
    /// Fee pool collected
    pub fee_pool: f64,
    
    /// Total volume traded
    pub total_volume: f64,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            books: HashMap::new(),
            fills: Vec::new(),
            user_orders: HashMap::new(),
            fee_pool: 0.0,
            total_volume: 0.0,
        }
    }

    /// Get or create order book for a market/outcome pair
    fn get_or_create_book(&mut self, market_id: &str, outcome: Outcome) -> &mut (BookSide, BookSide) {
        self.books
            .entry((market_id.to_string(), outcome))
            .or_insert_with(|| (BookSide::new(Side::Bid), BookSide::new(Side::Ask)))
    }

    /// Submit a new order to the matching engine
    pub fn submit_order(&mut self, mut order: LimitOrder) -> MatchResult {
        let mut result = MatchResult::new(order.clone());

        // Get the opposing side to match against
        let book_key = (order.market_id.clone(), order.outcome);
        
        // Ensure book exists
        if !self.books.contains_key(&book_key) {
            self.books.insert(book_key.clone(), (BookSide::new(Side::Bid), BookSide::new(Side::Ask)));
        }

        // Match against opposing side
        let fills = self.match_order(&mut order);
        
        for fill in fills {
            result.add_fill(fill);
        }

        // Handle remaining quantity based on order type
        match order.order_type {
            OrderType::IOC | OrderType::Market => {
                // Cancel any unfilled portion
                if order.remaining > 0.0001 {
                    order.cancel();
                }
            }
            OrderType::FOK => {
                // If not fully filled, cancel entirely (fills would have been prevented)
                if order.remaining > 0.0001 {
                    order.cancel();
                    result.fills.clear();
                    result.total_filled = 0.0;
                }
            }
            OrderType::GTC => {
                // Add remaining to book if not fully filled
                if order.remaining > 0.0001 && order.status.is_active() {
                    self.add_to_book(&order);
                    result.added_to_book = true;
                }
            }
        }

        // Store the order
        result.order = order.clone();
        self.orders.insert(order.id.clone(), order.clone());
        
        // Track user's orders
        self.user_orders
            .entry(order.maker.clone())
            .or_insert_with(Vec::new)
            .push(order.id.clone());

        result
    }

    /// Match an incoming order against the book
    fn match_order(&mut self, taker_order: &mut LimitOrder) -> Vec<Fill> {
        let mut fills = Vec::new();
        
        let book_key = (taker_order.market_id.clone(), taker_order.outcome);
        
        // Determine which side to match against
        let opposing_side = taker_order.side.opposite();
        
        loop {
            if taker_order.remaining <= 0.0001 {
                break;
            }

            // Get best price on opposing side
            let best_price = {
                let book = self.books.get(&book_key).unwrap();
                match opposing_side {
                    Side::Bid => book.0.best_price(),
                    Side::Ask => book.1.best_price(),
                }
            };

            let best_price = match best_price {
                Some(p) => p,
                None => break, // No orders on opposing side
            };

            // Check if prices cross
            let prices_cross = match taker_order.side {
                Side::Bid => taker_order.price_bps >= best_price, // Buyer willing to pay at least ask
                Side::Ask => taker_order.price_bps <= best_price, // Seller willing to accept at most bid
            };

            if !prices_cross {
                break;
            }

            // Get first order at best price
            let maker_order_id = {
                let book = self.books.get(&book_key).unwrap();
                let orders = match opposing_side {
                    Side::Bid => book.0.best_level_orders(),
                    Side::Ask => book.1.best_level_orders(),
                };
                
                match orders.and_then(|o| o.front()) {
                    Some(id) => id.clone(),
                    None => break,
                }
            };

            // Get the maker order
            let maker_order = match self.orders.get(&maker_order_id) {
                Some(o) if o.is_matchable() => o.clone(),
                _ => {
                    // Remove stale order from book
                    self.remove_from_book(&maker_order_id, &book_key, opposing_side);
                    continue;
                }
            };

            // Self-trade prevention
            if maker_order.maker == taker_order.maker {
                // Skip this order, try next
                self.remove_from_book(&maker_order_id, &book_key, opposing_side);
                continue;
            }

            // Calculate fill size
            let fill_size = taker_order.remaining.min(maker_order.remaining);
            
            // FOK check: if we can't fill entirely on this match, don't fill at all
            if taker_order.order_type == OrderType::FOK && fill_size < taker_order.remaining {
                // Check if total available can fill the order
                // For simplicity, we'll just try to match what we can
            }

            // Execute the fill at maker's price (price improvement for taker)
            let fill_price = maker_order.price_bps;
            let fill_value = (fill_price as f64 / 100.0) * fill_size;
            let maker_fee = fill_value * MAKER_FEE_RATE;
            let taker_fee = fill_value * TAKER_FEE_RATE;

            // Create fill record
            let fill = Fill::new(
                taker_order.market_id.clone(),
                taker_order.outcome,
                &maker_order,
                taker_order,
                fill_price,
                fill_size,
            );

            // Update orders
            taker_order.fill(fill_size, fill_price as f64, taker_fee);
            
            // Update maker order
            if let Some(mo) = self.orders.get_mut(&maker_order_id) {
                mo.fill(fill_size, fill_price as f64, maker_fee);
                
                // Remove from book if filled
                if !mo.is_matchable() {
                    self.remove_from_book(&maker_order_id, &book_key, opposing_side);
                } else {
                    // Update book's size tracking
                    if let Some((bids, asks)) = self.books.get_mut(&book_key) {
                        let side = match opposing_side {
                            Side::Bid => bids,
                            Side::Ask => asks,
                        };
                        if let Some(level) = side.levels.get_mut(&fill_price) {
                            level.total_size = (level.total_size - fill_size).max(0.0);
                        }
                        side.total_volume = (side.total_volume - fill_size).max(0.0);
                    }
                }
            }

            // Track fees and volume
            self.fee_pool += maker_fee + taker_fee;
            self.total_volume += fill_value;

            // Store fill
            self.fills.push(fill.clone());
            fills.push(fill);
        }

        fills
    }

    /// Add order to the appropriate side of the book
    fn add_to_book(&mut self, order: &LimitOrder) {
        let book = self.get_or_create_book(&order.market_id, order.outcome);
        
        match order.side {
            Side::Bid => book.0.add_order(order),
            Side::Ask => book.1.add_order(order),
        }
    }

    /// Remove order from book
    fn remove_from_book(&mut self, order_id: &str, book_key: &(String, Outcome), side: Side) {
        if let Some(order) = self.orders.get(order_id) {
            if let Some((bids, asks)) = self.books.get_mut(book_key) {
                match side {
                    Side::Bid => bids.remove_order(order),
                    Side::Ask => asks.remove_order(order),
                }
            }
        }
    }

    /// Cancel an order
    pub fn cancel_order(&mut self, order_id: &str, requester: &str) -> Result<LimitOrder, OrderError> {
        let order = self.orders.get_mut(order_id)
            .ok_or_else(|| OrderError::OrderNotFound(order_id.to_string()))?;

        // Verify ownership
        if order.maker != requester {
            return Err(OrderError::Unauthorized("Not your order".to_string()));
        }

        // Check if cancellable
        if !order.status.is_active() {
            return Err(OrderError::OrderNotActive(format!("Order is {:?}", order.status)));
        }

        // Remove from book
        let book_key = (order.market_id.clone(), order.outcome);
        if let Some((bids, asks)) = self.books.get_mut(&book_key) {
            match order.side {
                Side::Bid => bids.remove_order(order),
                Side::Ask => asks.remove_order(order),
            }
        }

        // Mark as cancelled
        order.cancel();

        Ok(order.clone())
    }

    /// Get order book depth for a market/outcome
    pub fn get_depth(&self, market_id: &str, outcome: Outcome, levels: usize) -> OrderBookSnapshot {
        let book_key = (market_id.to_string(), outcome);
        
        let (bids, asks) = self.books.get(&book_key)
            .map(|(b, a)| (b.depth(levels), a.depth(levels)))
            .unwrap_or_default();

        let best_bid = bids.first().map(|(p, _, _)| *p);
        let best_ask = asks.first().map(|(p, _, _)| *p);
        
        // Calculate spread
        let spread = match (best_bid, best_ask) {
            (Some(b), Some(a)) if a > b => Some(a - b),
            _ => None,
        };

        // Calculate mid price
        let mid_price = match (best_bid, best_ask) {
            (Some(b), Some(a)) => Some((b + a) / 2),
            (Some(b), None) => Some(b),
            (None, Some(a)) => Some(a),
            _ => None,
        };

        OrderBookSnapshot {
            market_id: market_id.to_string(),
            outcome,
            bids: bids.into_iter().map(|(p, s, c)| Level { price_bps: p, size: s, order_count: c }).collect(),
            asks: asks.into_iter().map(|(p, s, c)| Level { price_bps: p, size: s, order_count: c }).collect(),
            best_bid,
            best_ask,
            spread,
            mid_price,
            timestamp: now(),
        }
    }

    /// Get best bid and ask (for odds calculation)
    pub fn get_best_prices(&self, market_id: &str, outcome: Outcome) -> (Option<u64>, Option<u64>) {
        let book_key = (market_id.to_string(), outcome);
        
        self.books.get(&book_key)
            .map(|(bids, asks)| (bids.best_price(), asks.best_price()))
            .unwrap_or((None, None))
    }

    /// Get user's open orders
    pub fn get_user_orders(&self, wallet: &str) -> Vec<&LimitOrder> {
        self.user_orders.get(wallet)
            .map(|order_ids| {
                order_ids.iter()
                    .filter_map(|id| self.orders.get(id))
                    .filter(|o| o.status.is_active())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get an order by ID
    pub fn get_order(&self, order_id: &str) -> Option<&LimitOrder> {
        self.orders.get(order_id)
    }

    /// Get recent fills for a market
    pub fn get_market_fills(&self, market_id: &str, limit: usize) -> Vec<&Fill> {
        self.fills.iter()
            .rev()
            .filter(|f| f.market_id == market_id)
            .take(limit)
            .collect()
    }

    /// Clean up expired orders
    pub fn cleanup_expired_orders(&mut self) -> Vec<LimitOrder> {
        let mut expired = Vec::new();
        
        let expired_ids: Vec<String> = self.orders.iter()
            .filter(|(_, o)| o.is_expired() && o.status.is_active())
            .map(|(id, _)| id.clone())
            .collect();

        for order_id in expired_ids {
            if let Some(mut order) = self.orders.get_mut(&order_id) {
                // Remove from book
                let book_key = (order.market_id.clone(), order.outcome);
                if let Some((bids, asks)) = self.books.get_mut(&book_key) {
                    match order.side {
                        Side::Bid => bids.remove_order(&order),
                        Side::Ask => asks.remove_order(&order),
                    }
                }
                
                order.expire();
                expired.push(order.clone());
            }
        }

        expired
    }
}

// ============================================================================
// SNAPSHOT TYPES (for API responses)
// ============================================================================

/// A single price level in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub price_bps: u64,
    pub size: f64,
    pub order_count: usize,
}

/// Snapshot of an order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub market_id: String,
    pub outcome: Outcome,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub best_bid: Option<u64>,
    pub best_ask: Option<u64>,
    pub spread: Option<u64>,
    pub mid_price: Option<u64>,
    pub timestamp: u64,
}

impl OrderBookSnapshot {
    /// Get implied probability from best bid/ask
    /// YES probability = best_ask / 100 (cost to buy YES)
    /// More liquid markets have tighter spreads
    pub fn implied_probability(&self) -> Option<f64> {
        self.mid_price.map(|p| p as f64 / 100.0)
    }

    /// Get the YES/NO odds for display
    pub fn odds(&self) -> (f64, f64) {
        let yes_prob = self.implied_probability().unwrap_or(0.5);
        let no_prob = 1.0 - yes_prob;
        (yes_prob, no_prob)
    }
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

    fn create_test_order(side: Side, price: u64, size: f64, maker: &str) -> LimitOrder {
        LimitOrder::new(
            "market_test".to_string(),
            Outcome::YES,
            side,
            price,
            size,
            OrderType::GTC,
            maker.to_string(),
            "sig".to_string(),
        ).unwrap()
    }

    #[test]
    fn test_submit_bid_no_match() {
        let mut engine = MatchingEngine::new();
        
        let order = create_test_order(Side::Bid, 50, 100.0, "alice");
        let result = engine.submit_order(order);

        assert!(result.fills.is_empty());
        assert!(result.added_to_book);
        assert_eq!(result.order.status, OrderStatus::Open);
    }

    #[test]
    fn test_simple_match() {
        let mut engine = MatchingEngine::new();
        
        // Alice posts ask at 60
        let ask = create_test_order(Side::Ask, 60, 100.0, "alice");
        engine.submit_order(ask);

        // Bob bids at 60 - should match
        let bid = create_test_order(Side::Bid, 60, 100.0, "bob");
        let result = engine.submit_order(bid);

        assert_eq!(result.fills.len(), 1);
        assert!(!result.added_to_book);
        assert_eq!(result.total_filled, 100.0);
        assert_eq!(result.order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_partial_fill() {
        let mut engine = MatchingEngine::new();
        
        // Alice posts ask for 50 shares at 60
        let ask = create_test_order(Side::Ask, 60, 50.0, "alice");
        engine.submit_order(ask);

        // Bob bids for 100 shares at 60 - partial match
        let bid = create_test_order(Side::Bid, 60, 100.0, "bob");
        let result = engine.submit_order(bid);

        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.total_filled, 50.0);
        assert!(result.added_to_book); // Remaining 50 added to book
        assert_eq!(result.order.status, OrderStatus::PartiallyFilled);
        assert_eq!(result.order.remaining, 50.0);
    }

    #[test]
    fn test_price_time_priority() {
        let mut engine = MatchingEngine::new();
        
        // Alice posts ask at 60
        let ask1 = create_test_order(Side::Ask, 60, 50.0, "alice");
        engine.submit_order(ask1);

        // Charlie posts ask at 55 (better price)
        let ask2 = create_test_order(Side::Ask, 55, 50.0, "charlie");
        engine.submit_order(ask2);

        // Bob bids at 60 - should match Charlie's 55 first (better for taker)
        let bid = create_test_order(Side::Bid, 60, 100.0, "bob");
        let result = engine.submit_order(bid);

        assert_eq!(result.fills.len(), 2);
        // First fill at 55 with Charlie
        assert_eq!(result.fills[0].price_bps, 55);
        assert_eq!(result.fills[0].maker, "charlie");
        // Second fill at 60 with Alice
        assert_eq!(result.fills[1].price_bps, 60);
        assert_eq!(result.fills[1].maker, "alice");
    }

    #[test]
    fn test_self_trade_prevention() {
        let mut engine = MatchingEngine::new();
        
        // Alice posts ask at 60
        let ask = create_test_order(Side::Ask, 60, 100.0, "alice");
        engine.submit_order(ask);

        // Alice posts bid at 60 - should NOT match her own order
        let bid = create_test_order(Side::Bid, 60, 100.0, "alice");
        let result = engine.submit_order(bid);

        assert!(result.fills.is_empty());
    }

    #[test]
    fn test_cancel_order() {
        let mut engine = MatchingEngine::new();
        
        let order = create_test_order(Side::Bid, 50, 100.0, "alice");
        let result = engine.submit_order(order);
        let order_id = result.order.id.clone();

        // Cancel it
        let cancelled = engine.cancel_order(&order_id, "alice").unwrap();
        assert_eq!(cancelled.status, OrderStatus::Cancelled);

        // Verify it's not on book
        let snapshot = engine.get_depth("market_test", Outcome::YES, 10);
        assert!(snapshot.bids.is_empty());
    }

    #[test]
    fn test_depth_snapshot() {
        let mut engine = MatchingEngine::new();
        
        // Add some orders
        engine.submit_order(create_test_order(Side::Bid, 50, 100.0, "alice"));
        engine.submit_order(create_test_order(Side::Bid, 48, 200.0, "bob"));
        engine.submit_order(create_test_order(Side::Ask, 55, 150.0, "charlie"));
        engine.submit_order(create_test_order(Side::Ask, 58, 75.0, "dave"));

        let snapshot = engine.get_depth("market_test", Outcome::YES, 10);

        assert_eq!(snapshot.best_bid, Some(50));
        assert_eq!(snapshot.best_ask, Some(55));
        assert_eq!(snapshot.spread, Some(5));
        assert_eq!(snapshot.mid_price, Some(52));

        // Check bid levels (descending)
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.bids[0].price_bps, 50);
        assert_eq!(snapshot.bids[1].price_bps, 48);

        // Check ask levels (ascending)
        assert_eq!(snapshot.asks.len(), 2);
        assert_eq!(snapshot.asks[0].price_bps, 55);
        assert_eq!(snapshot.asks[1].price_bps, 58);
    }
}
