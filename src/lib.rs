/// BlackBook L2 Prediction Market Ledger
/// Exports all modules for use as a library crate

pub mod market_resolve;
pub mod easteregg;
pub mod bridge;
pub mod auth;
pub mod ledger;
pub mod orderbook;
pub mod shares;

#[path = "../rss/mod.rs"]
pub mod rss;

#[path = "../rpc/mod.rs"]
pub mod rpc;

// Re-export from market_resolve (kept for CPMM fallback)
pub use market_resolve::{Ledger as MarketLedger, Transaction as MarketTransaction, Recipe};
pub use market_resolve::{Market, MarketManager, Bet, MarketStatus, BetStatus};
pub use market_resolve::{CPMMPool, SwapResult, EventStatus, PendingEvent, LP_FEE_RATE, MINIMUM_LAUNCH_LIQUIDITY, VIABILITY_THRESHOLD, VIABILITY_PERIOD_SECONDS};
pub use market_resolve::escrow::*;

// Re-export from orderbook (CLOB system)
pub use orderbook::{
    OrderBookManager, MatchingEngine, MarketOdds, OddsSource, OrderBookStats,
    LimitOrder, OrderType, OrderStatus, Side, Outcome, Fill,
    PlaceOrderRequest, OrderError,
    OrderBookSnapshot, Level, MatchResult, OrderSubmitResult,
    MAKER_FEE_RATE, TAKER_FEE_RATE,
};

// Re-export from shares (outcome share system)
pub use shares::{
    SharesManager, ShareBalance, SharePosition, OutcomeIndex,
    ShareTransaction, ShareTxType, SharesStats, UserPositionsSummary, PositionInfo,
    SimplePosition,
    MintRequest, MintResult, RedeemRequest, RedeemResult,
    execute_mint, execute_paired_redeem, execute_resolution_redeem,
    check_arbitrage_opportunity, ArbitrageOpportunity, ArbitrageType,
};

pub use easteregg::{GodMode, TestAccount, AccountInfo, SignedMessage, GodModeError};
pub use ledger::{Ledger, L1Client, Balance, Transaction, TxType, LedgerStats};
pub use rpc::{SignedTransaction, SignedTxType, TransactionPayload, SignedTxError, TX_EXPIRY_SECS};
pub use rpc::{L1BlackBookRpc, L1RpcConfig, L1HealthResponse, L1WalletLookupResponse, L1BalanceResponse, L1PoHStatus};
pub use bridge::{BridgeManager, BridgeStatus, BridgeDirection, PendingBridge, BridgeError, BridgeRequest, BridgeResponse, BridgeCompleteRequest, BridgeCompleteResponse, BridgeStatusResponse, BridgeStats};
pub use rss::{RssEvent, ResolutionRules, RssFeedManager, EventDates, write_rss_event_to_file, load_rss_events_from_folder};
