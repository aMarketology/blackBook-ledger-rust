/// BlackBook L2 Prediction Market Ledger
/// Exports all modules for use as a library crate

pub mod ledger;
pub mod escrow;
pub mod hot_upgrades;
pub mod markets;
pub mod cpmm;
pub mod godmode;
pub mod l1_rpc_client;
pub mod signed_transaction;
pub mod bridge;

pub use ledger::{Ledger, Transaction, Recipe};
pub use escrow::*;
pub use hot_upgrades::*;
pub use markets::{Market, MarketManager, Bet, MarketStatus, BetStatus};
pub use cpmm::{CPMMPool, SwapResult, EventStatus, PendingEvent, LP_FEE_RATE, MINIMUM_LAUNCH_LIQUIDITY, VIABILITY_THRESHOLD, VIABILITY_PERIOD_SECONDS};
pub use godmode::{GodMode, TestAccount, AccountInfo, SignedMessage, GodModeError};
pub use l1_rpc_client::{L1RpcClient, L1RpcError, VerifySignatureRequest, VerifySignatureResponse, L1AccountInfo};
pub use signed_transaction::{SignedTransaction, SignedTxType, TransactionPayload, SignedTxError, TX_EXPIRY_SECS};
pub use bridge::{BridgeManager, BridgeStatus, BridgeDirection, PendingBridge, BridgeError, BridgeRequest, BridgeResponse, BridgeCompleteRequest, BridgeCompleteResponse, BridgeStatusResponse, BridgeStats};
