// ============================================================================
// CLOB Order Types - BlackBook Prediction Market
// ============================================================================
//
// Order types for the Central Limit Order Book (CLOB) system.
// Supports limit orders with price-time priority matching.
//
// Price Convention:
//   - Prices are in basis points (1-99 representing 0.01-0.99 BB per share)
//   - YES price + NO price = 100 (1.00 BB) always
//   - A YES price of 65 means 0.65 BB per YES share (65% implied probability)
//
// ============================================================================

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum price in basis points (0.01 BB = 1%)
pub const MIN_PRICE_BPS: u64 = 1;

/// Maximum price in basis points (0.99 BB = 99%)
pub const MAX_PRICE_BPS: u64 = 99;

/// Minimum order size in BB (0.01 BB)
pub const MIN_ORDER_SIZE: f64 = 0.01;

/// Maximum order size in BB (1,000,000 BB)
pub const MAX_ORDER_SIZE: f64 = 1_000_000.0;

/// Maker fee rate (0.1% - incentive for liquidity providers)
pub const MAKER_FEE_RATE: f64 = 0.001;

/// Taker fee rate (0.5% - market orders pay more)
pub const TAKER_FEE_RATE: f64 = 0.005;

/// Order expiration time (7 days in seconds)
pub const ORDER_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

// ============================================================================
// ENUMS
// ============================================================================

/// Order side - which outcome the order is for
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Buying YES shares (betting event will happen)
    Bid,
    /// Selling YES shares (betting event won't happen)
    Ask,
}

impl Side {
    pub fn opposite(&self) -> Self {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Bid => "bid",
            Side::Ask => "ask",
        }
    }
}

/// Order type - execution behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderType {
    /// Good Till Cancelled - stays on book until filled or cancelled
    GTC,
    /// Immediate Or Cancel - fill what you can immediately, cancel rest
    IOC,
    /// Fill Or Kill - fill entire order immediately or cancel completely
    FOK,
    /// Market Order - execute at best available price
    Market,
}

impl Default for OrderType {
    fn default() -> Self {
        OrderType::GTC
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// Order is active on the book
    Open,
    /// Order has been partially filled
    PartiallyFilled,
    /// Order has been completely filled
    Filled,
    /// Order was cancelled by user
    Cancelled,
    /// Order expired
    Expired,
    /// Order was rejected (validation failed)
    Rejected,
}

impl OrderStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::Open | OrderStatus::PartiallyFilled)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Expired | OrderStatus::Rejected)
    }
}

/// Outcome for multi-outcome markets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Outcome(pub usize);

impl Outcome {
    pub const YES: Outcome = Outcome(0);
    pub const NO: Outcome = Outcome(1);

    pub fn new(index: usize) -> Self {
        Outcome(index)
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

// ============================================================================
// LIMIT ORDER
// ============================================================================

/// A limit order on the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitOrder {
    /// Unique order identifier
    pub id: String,

    /// Market this order is for
    pub market_id: String,

    /// Which outcome (0=YES, 1=NO for binary markets)
    pub outcome: Outcome,

    /// Bid (buy) or Ask (sell)
    pub side: Side,

    /// Price in basis points (1-99)
    pub price_bps: u64,

    /// Original size in shares
    pub size: f64,

    /// Amount filled so far
    pub filled: f64,

    /// Remaining size
    pub remaining: f64,

    /// Order type (GTC, IOC, FOK, Market)
    pub order_type: OrderType,

    /// Current status
    pub status: OrderStatus,

    /// Wallet address of the maker
    pub maker: String,

    /// Signature for verification
    pub signature: String,

    /// Unix timestamp when created
    pub created_at: u64,

    /// Unix timestamp when last updated
    pub updated_at: u64,

    /// Unix timestamp when order expires (for GTC orders)
    pub expires_at: Option<u64>,

    /// Average fill price (in basis points)
    pub avg_fill_price: Option<f64>,

    /// Total fees paid
    pub fees_paid: f64,
}

impl LimitOrder {
    /// Create a new limit order
    pub fn new(
        market_id: String,
        outcome: Outcome,
        side: Side,
        price_bps: u64,
        size: f64,
        order_type: OrderType,
        maker: String,
        signature: String,
    ) -> Result<Self, OrderError> {
        // Validate price
        if price_bps < MIN_PRICE_BPS || price_bps > MAX_PRICE_BPS {
            return Err(OrderError::InvalidPrice(format!(
                "Price must be between {} and {} bps, got {}",
                MIN_PRICE_BPS, MAX_PRICE_BPS, price_bps
            )));
        }

        // Validate size
        if size < MIN_ORDER_SIZE {
            return Err(OrderError::InvalidSize(format!(
                "Size must be at least {} BB, got {}",
                MIN_ORDER_SIZE, size
            )));
        }
        if size > MAX_ORDER_SIZE {
            return Err(OrderError::InvalidSize(format!(
                "Size must be at most {} BB, got {}",
                MAX_ORDER_SIZE, size
            )));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expires_at = match order_type {
            OrderType::GTC => Some(now + ORDER_EXPIRY_SECS),
            _ => None, // IOC, FOK, Market orders don't expire - they execute or cancel immediately
        };

        Ok(Self {
            id: format!("ord_{}", Uuid::new_v4().to_string().replace("-", "")[..16].to_string()),
            market_id,
            outcome,
            side,
            price_bps,
            size,
            filled: 0.0,
            remaining: size,
            order_type,
            status: OrderStatus::Open,
            maker,
            signature,
            created_at: now,
            updated_at: now,
            expires_at,
            avg_fill_price: None,
            fees_paid: 0.0,
        })
    }

    /// Create a market order (executes at best available price)
    pub fn market_order(
        market_id: String,
        outcome: Outcome,
        side: Side,
        size: f64,
        maker: String,
        signature: String,
    ) -> Result<Self, OrderError> {
        // Market orders use price 0 for bids (will match any ask) or 100 for asks
        let price_bps = match side {
            Side::Bid => MAX_PRICE_BPS, // Willing to pay up to 99
            Side::Ask => MIN_PRICE_BPS, // Willing to sell down to 1
        };
        
        Self::new(market_id, outcome, side, price_bps, size, OrderType::Market, maker, signature)
    }

    /// Get price as decimal (0.01 - 0.99)
    pub fn price(&self) -> f64 {
        self.price_bps as f64 / 100.0
    }

    /// Check if order is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > expires_at
        } else {
            false
        }
    }

    /// Check if order can be matched
    pub fn is_matchable(&self) -> bool {
        self.status.is_active() && !self.is_expired() && self.remaining > 0.0
    }

    /// Fill a portion of the order
    pub fn fill(&mut self, fill_size: f64, fill_price: f64, fee: f64) {
        let old_filled = self.filled;
        self.filled += fill_size;
        self.remaining = (self.size - self.filled).max(0.0);
        self.fees_paid += fee;

        // Update average fill price
        if old_filled == 0.0 {
            self.avg_fill_price = Some(fill_price);
        } else if let Some(avg) = self.avg_fill_price {
            // Weighted average
            self.avg_fill_price = Some((avg * old_filled + fill_price * fill_size) / self.filled);
        }

        // Update status
        if self.remaining <= 0.0001 { // Small epsilon for floating point
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }

        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Cancel the order
    pub fn cancel(&mut self) {
        if self.status.is_active() {
            self.status = OrderStatus::Cancelled;
            self.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }

    /// Mark as expired
    pub fn expire(&mut self) {
        if self.status.is_active() {
            self.status = OrderStatus::Expired;
            self.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }

    /// Calculate the cost to place this order (collateral required)
    pub fn required_collateral(&self) -> f64 {
        match self.side {
            // Buying YES: pay price * size
            Side::Bid => self.price() * self.remaining,
            // Selling YES: need to own the shares OR have (1-price) * size as collateral
            // (because you're effectively buying NO at 1-price)
            Side::Ask => (1.0 - self.price()) * self.remaining,
        }
    }
}

// ============================================================================
// FILL (Trade Record)
// ============================================================================

/// A record of a trade execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Unique fill identifier
    pub id: String,

    /// Market ID
    pub market_id: String,

    /// Outcome traded
    pub outcome: Outcome,

    /// Maker order ID
    pub maker_order_id: String,

    /// Taker order ID
    pub taker_order_id: String,

    /// Maker wallet address
    pub maker: String,

    /// Taker wallet address
    pub taker: String,

    /// Execution price in basis points
    pub price_bps: u64,

    /// Size filled
    pub size: f64,

    /// BB value of the trade (price * size)
    pub value: f64,

    /// Fee paid by maker
    pub maker_fee: f64,

    /// Fee paid by taker
    pub taker_fee: f64,

    /// Unix timestamp
    pub timestamp: u64,

    /// Which side was the taker
    pub taker_side: Side,
}

impl Fill {
    pub fn new(
        market_id: String,
        outcome: Outcome,
        maker_order: &LimitOrder,
        taker_order: &LimitOrder,
        price_bps: u64,
        size: f64,
    ) -> Self {
        let value = (price_bps as f64 / 100.0) * size;
        let maker_fee = value * MAKER_FEE_RATE;
        let taker_fee = value * TAKER_FEE_RATE;

        Self {
            id: format!("fill_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            market_id,
            outcome,
            maker_order_id: maker_order.id.clone(),
            taker_order_id: taker_order.id.clone(),
            maker: maker_order.maker.clone(),
            taker: taker_order.maker.clone(),
            price_bps,
            size,
            value,
            maker_fee,
            taker_fee,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            taker_side: taker_order.side,
        }
    }

    /// Price as decimal
    pub fn price(&self) -> f64 {
        self.price_bps as f64 / 100.0
    }
}

// ============================================================================
// ORDER REQUEST (API Input)
// ============================================================================

/// Request to place a new order
#[derive(Debug, Clone, Deserialize)]
pub struct PlaceOrderRequest {
    pub market_id: String,
    pub outcome: usize,
    pub side: Side,
    pub price_bps: Option<u64>,  // None for market orders
    pub size: f64,
    pub order_type: Option<OrderType>,
    pub wallet_address: String,
    pub signature: String,
    pub nonce: u64,
    pub timestamp: u64,
}

/// Request to cancel an order
#[derive(Debug, Clone, Deserialize)]
pub struct CancelOrderRequest {
    pub order_id: String,
    pub wallet_address: String,
    pub signature: String,
    pub nonce: u64,
    pub timestamp: u64,
}

// ============================================================================
// ERRORS
// ============================================================================

/// Order-related errors
#[derive(Debug, Clone, Serialize)]
pub enum OrderError {
    InvalidPrice(String),
    InvalidSize(String),
    InvalidOutcome(String),
    InsufficientBalance(String),
    InsufficientShares(String),
    OrderNotFound(String),
    OrderNotActive(String),
    Unauthorized(String),
    MarketNotFound(String),
    MarketClosed(String),
    InvalidSignature(String),
    InvalidNonce(String),
    Expired(String),
}

impl std::fmt::Display for OrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderError::InvalidPrice(msg) => write!(f, "Invalid price: {}", msg),
            OrderError::InvalidSize(msg) => write!(f, "Invalid size: {}", msg),
            OrderError::InvalidOutcome(msg) => write!(f, "Invalid outcome: {}", msg),
            OrderError::InsufficientBalance(msg) => write!(f, "Insufficient balance: {}", msg),
            OrderError::InsufficientShares(msg) => write!(f, "Insufficient shares: {}", msg),
            OrderError::OrderNotFound(msg) => write!(f, "Order not found: {}", msg),
            OrderError::OrderNotActive(msg) => write!(f, "Order not active: {}", msg),
            OrderError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            OrderError::MarketNotFound(msg) => write!(f, "Market not found: {}", msg),
            OrderError::MarketClosed(msg) => write!(f, "Market closed: {}", msg),
            OrderError::InvalidSignature(msg) => write!(f, "Invalid signature: {}", msg),
            OrderError::InvalidNonce(msg) => write!(f, "Invalid nonce: {}", msg),
            OrderError::Expired(msg) => write!(f, "Expired: {}", msg),
        }
    }
}

impl std::error::Error for OrderError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_limit_order() {
        let order = LimitOrder::new(
            "market_123".to_string(),
            Outcome::YES,
            Side::Bid,
            65, // 0.65 BB per share
            100.0,
            OrderType::GTC,
            "L1ALICE000000001".to_string(),
            "sig123".to_string(),
        ).unwrap();

        assert_eq!(order.price(), 0.65);
        assert_eq!(order.size, 100.0);
        assert_eq!(order.remaining, 100.0);
        assert_eq!(order.filled, 0.0);
        assert!(order.status.is_active());
    }

    #[test]
    fn test_invalid_price() {
        let result = LimitOrder::new(
            "market_123".to_string(),
            Outcome::YES,
            Side::Bid,
            0, // Invalid - below minimum
            100.0,
            OrderType::GTC,
            "L1ALICE000000001".to_string(),
            "sig123".to_string(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_fill_order() {
        let mut order = LimitOrder::new(
            "market_123".to_string(),
            Outcome::YES,
            Side::Bid,
            65,
            100.0,
            OrderType::GTC,
            "L1ALICE000000001".to_string(),
            "sig123".to_string(),
        ).unwrap();

        order.fill(50.0, 65.0, 0.325); // Fill half at price 65, with fee

        assert_eq!(order.filled, 50.0);
        assert_eq!(order.remaining, 50.0);
        assert_eq!(order.status, OrderStatus::PartiallyFilled);

        order.fill(50.0, 65.0, 0.325); // Fill rest

        assert_eq!(order.filled, 100.0);
        assert!(order.remaining < 0.001);
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_required_collateral() {
        // Buying YES at 0.65: need 0.65 * size
        let bid = LimitOrder::new(
            "market_123".to_string(),
            Outcome::YES,
            Side::Bid,
            65,
            100.0,
            OrderType::GTC,
            "L1ALICE000000001".to_string(),
            "sig123".to_string(),
        ).unwrap();

        assert!((bid.required_collateral() - 65.0).abs() < 0.001);

        // Selling YES at 0.65: need (1-0.65) * size = 0.35 * size
        let ask = LimitOrder::new(
            "market_123".to_string(),
            Outcome::YES,
            Side::Ask,
            65,
            100.0,
            OrderType::GTC,
            "L1ALICE000000001".to_string(),
            "sig123".to_string(),
        ).unwrap();

        assert!((ask.required_collateral() - 35.0).abs() < 0.001);
    }
}
