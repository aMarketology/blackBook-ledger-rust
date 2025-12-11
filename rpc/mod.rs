// ============================================================================
// RPC Module - L1/L2 Communication & Signed Transactions
// ============================================================================
//
// This module handles all RPC communication between L1 and L2 layers,
// including signed transactions, signature verification, and settlements.
//
// Components:
//   - signed_transaction: Ed25519 signed transaction handling
//   - l1_blackbook_rpc: L1 blockchain RPC client wrapper
//
// ============================================================================

pub mod signed_transaction;
pub mod l1_blackbook_rpc;

pub use signed_transaction::*;
pub use l1_blackbook_rpc::*;
