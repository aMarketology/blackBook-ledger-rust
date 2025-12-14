/// BlackBook L2 Prediction Market Ledger
/// Exports all modules for use as a library crate

pub mod market_resolve;
pub mod easteregg;
pub mod bridge;
pub mod auth;
pub mod ledger;

#[path = "../rss/mod.rs"]
pub mod rss;

#[path = "../rpc/mod.rs"]
pub mod rpc;

// Re-export from market_resolve
pub use market_resolve::{Ledger as MarketLedger, Transaction as MarketTransaction, Recipe};
pub use market_resolve::{Market, MarketManager, Bet, MarketStatus, BetStatus};
pub use market_resolve::{CPMMPool, SwapResult, EventStatus, PendingEvent, LP_FEE_RATE, MINIMUM_LAUNCH_LIQUIDITY, VIABILITY_THRESHOLD, VIABILITY_PERIOD_SECONDS};
pub use market_resolve::escrow::*;
pub use easteregg::{GodMode, TestAccount, AccountInfo, SignedMessage, GodModeError};
pub use ledger::{Ledger, L1Client, Balance, Transaction, TxType, LedgerStats};
pub use rpc::{SignedTransaction, SignedTxType, TransactionPayload, SignedTxError, TX_EXPIRY_SECS};
pub use rpc::{L1BlackBookRpc, L1RpcConfig, L1HealthResponse, L1WalletLookupResponse, L1BalanceResponse, L1PoHStatus};
pub use bridge::{BridgeManager, BridgeStatus, BridgeDirection, PendingBridge, BridgeError, BridgeRequest, BridgeResponse, BridgeCompleteRequest, BridgeCompleteResponse, BridgeStatusResponse, BridgeStats};
pub use rss::{RssEvent, ResolutionRules, RssFeedManager, EventDates, write_rss_event_to_file, load_rss_events_from_folder};
