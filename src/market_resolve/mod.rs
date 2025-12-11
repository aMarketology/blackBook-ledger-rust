// ============================================================================
// Market Resolve Module - Core Market & Betting Logic
// ============================================================================
//
// This module contains the core prediction market functionality:
//   - cpmm: Constant Product Market Maker for pricing
//   - ledger: Transaction ledger and accounting
//   - escrow: Funds locking and release for bets
//   - markets: Market creation, betting, and resolution
//
// ============================================================================

pub mod cpmm;
pub mod ledger;
pub mod escrow;
pub mod markets;

pub use cpmm::*;
pub use ledger::*;
pub use escrow::*;
pub use markets::*;
