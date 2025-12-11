/// BlackBook L2 Prediction Market Ledger
/// Exports all modules for use as a library crate

pub mod market_resolve;
pub mod hot_upgrades;
pub mod easteregg;
pub mod l1_rpc_client;
pub mod bridge;
pub mod auth;

#[path = "../rss/mod.rs"]
pub mod rss;

#[path = "../rpc/mod.rs"]
pub mod rpc;

// Re-export from market_resolve
pub use market_resolve::{Ledger, Transaction, Recipe};
pub use market_resolve::{Market, MarketManager, Bet, MarketStatus, BetStatus};
pub use market_resolve::{CPMMPool, SwapResult, EventStatus, PendingEvent, LP_FEE_RATE, MINIMUM_LAUNCH_LIQUIDITY, VIABILITY_THRESHOLD, VIABILITY_PERIOD_SECONDS};
pub use market_resolve::escrow::*;
pub use easteregg::{GodMode, TestAccount, AccountInfo, SignedMessage, GodModeError};
pub use l1_rpc_client::{L1RpcClient, L1RpcError, VerifySignatureRequest, VerifySignatureResponse, L1AccountInfo};
pub use rpc::{SignedTransaction, SignedTxType, TransactionPayload, SignedTxError, TX_EXPIRY_SECS};
pub use rpc::{L1BlackBookRpc, L1RpcConfig, L1HealthResponse, L1WalletLookupResponse, L1BalanceResponse, L1PoHStatus};
pub use bridge::{BridgeManager, BridgeStatus, BridgeDirection, PendingBridge, BridgeError, BridgeRequest, BridgeResponse, BridgeCompleteRequest, BridgeCompleteResponse, BridgeStatusResponse, BridgeStats};
pub use rss::{RssEvent, ResolutionRules, RssFeedManager, EventDates, write_rss_event_to_file, load_rss_events_from_folder};
