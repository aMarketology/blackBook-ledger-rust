// ============================================================================
// Share Minting - BlackBook Prediction Market
// ============================================================================
//
// Minting creates new outcome shares from BB tokens.
// 
// Core Mechanic:
//   1 BB → 1 YES share + 1 NO share
//
// This is the fundamental operation that creates tradeable positions.
// The BB is locked in the market's collateral pool until shares are redeemed.
//
// Why This Matters:
//   - Creates the shares that can be traded on the order book
//   - Enables arbitrage that keeps YES + NO prices summing to 1.00
//   - All shares are fully collateralized (no leverage)
//
// ============================================================================

use super::{OutcomeIndex, SharesManager, ShareTxType, MIN_SHARE_SIZE, MAX_SHARE_SIZE};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// MINT REQUEST/RESPONSE
// ============================================================================

/// Request to mint shares
#[derive(Debug, Clone, Deserialize)]
pub struct MintRequest {
    /// Market ID to mint shares for
    pub market_id: String,
    
    /// Amount of BB to convert to shares
    /// User pays this amount and receives same amount of YES + NO shares
    pub bb_amount: f64,
    
    /// Wallet address
    pub wallet_address: String,
    
    /// Signature for verification
    pub signature: String,
    
    /// Nonce for replay protection
    pub nonce: u64,
    
    /// Timestamp
    pub timestamp: u64,
}

/// Result of a mint operation
#[derive(Debug, Clone, Serialize)]
pub struct MintResult {
    pub success: bool,
    pub mint_id: Option<String>,
    pub market_id: String,
    pub wallet: String,
    /// Amount of BB consumed
    pub bb_spent: f64,
    /// YES shares received
    pub yes_shares: f64,
    /// NO shares received  
    pub no_shares: f64,
    /// Transaction fee (if any)
    pub fee: f64,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl MintResult {
    pub fn success(
        mint_id: String,
        market_id: String,
        wallet: String,
        bb_spent: f64,
        shares: f64,
        fee: f64,
    ) -> Self {
        Self {
            success: true,
            mint_id: Some(mint_id),
            market_id,
            wallet,
            bb_spent,
            yes_shares: shares,
            no_shares: shares,
            fee,
            error: None,
            timestamp: now(),
        }
    }

    pub fn error(market_id: String, wallet: String, msg: String) -> Self {
        Self {
            success: false,
            mint_id: None,
            market_id,
            wallet,
            bb_spent: 0.0,
            yes_shares: 0.0,
            no_shares: 0.0,
            fee: 0.0,
            error: Some(msg),
            timestamp: now(),
        }
    }
}

// ============================================================================
// MINT RECORD
// ============================================================================

/// Record of a completed mint operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintRecord {
    pub id: String,
    pub market_id: String,
    pub wallet: String,
    pub bb_amount: f64,
    pub shares_minted: f64,
    pub fee: f64,
    pub timestamp: u64,
}

// ============================================================================
// MINTING LOGIC
// ============================================================================

/// Mint fee rate (0% - minting is free to encourage liquidity)
pub const MINT_FEE_RATE: f64 = 0.0;

/// Execute a mint operation
/// 
/// Converts BB tokens into equal amounts of YES and NO shares.
/// The BB is held as collateral until shares are redeemed or market resolves.
///
/// # Arguments
/// * `shares_manager` - The shares manager to credit shares to
/// * `request` - The mint request
/// * `check_balance` - Function to check if user has sufficient BB balance
/// * `deduct_balance` - Function to deduct BB from user's balance
///
/// # Returns
/// * `MintResult` - The result of the mint operation
pub fn execute_mint<F, G>(
    shares_manager: &mut SharesManager,
    request: &MintRequest,
    check_balance: F,
    deduct_balance: G,
) -> MintResult 
where
    F: FnOnce(&str, f64) -> bool,
    G: FnOnce(&str, f64) -> Result<(), String>,
{
    // Validate amount
    if request.bb_amount < MIN_SHARE_SIZE {
        return MintResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Amount must be at least {} BB", MIN_SHARE_SIZE),
        );
    }

    if request.bb_amount > MAX_SHARE_SIZE {
        return MintResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Amount must be at most {} BB", MAX_SHARE_SIZE),
        );
    }

    // Check balance
    if !check_balance(&request.wallet_address, request.bb_amount) {
        return MintResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            "Insufficient BB balance".to_string(),
        );
    }

    // Calculate fee
    let fee = request.bb_amount * MINT_FEE_RATE;
    let net_amount = request.bb_amount - fee;

    // Deduct BB from user
    if let Err(e) = deduct_balance(&request.wallet_address, request.bb_amount) {
        return MintResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Failed to deduct BB: {}", e),
        );
    }

    // Generate mint ID
    let mint_id = format!("mint_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_string());

    // Credit YES shares
    shares_manager.credit_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::YES,
        net_amount,
        0.5, // Minted shares have 0.50 cost basis (half the pair)
        ShareTxType::Mint,
    );

    // Credit NO shares
    shares_manager.credit_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::NO,
        net_amount,
        0.5, // Minted shares have 0.50 cost basis (half the pair)
        ShareTxType::Mint,
    );

    // Update stats
    shares_manager.stats.total_bb_locked += net_amount;

    MintResult::success(
        mint_id,
        request.market_id.clone(),
        request.wallet_address.clone(),
        request.bb_amount,
        net_amount,
        fee,
    )
}

/// Calculate how many shares can be minted for a given BB amount
pub fn calculate_mint_output(bb_amount: f64) -> (f64, f64, f64) {
    let fee = bb_amount * MINT_FEE_RATE;
    let shares = bb_amount - fee;
    (shares, shares, fee) // (yes_shares, no_shares, fee)
}

/// Calculate BB required to mint a specific number of shares
pub fn calculate_mint_input(desired_shares: f64) -> f64 {
    // shares = bb_amount * (1 - fee_rate)
    // bb_amount = shares / (1 - fee_rate)
    if MINT_FEE_RATE >= 1.0 {
        return f64::INFINITY;
    }
    desired_shares / (1.0 - MINT_FEE_RATE)
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

    fn dummy_check_balance(_wallet: &str, _amount: f64) -> bool {
        true
    }

    fn dummy_deduct_balance(_wallet: &str, _amount: f64) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn test_mint_creates_equal_shares() {
        let mut manager = SharesManager::new();
        
        let request = MintRequest {
            market_id: "market1".to_string(),
            bb_amount: 100.0,
            wallet_address: "alice".to_string(),
            signature: "sig".to_string(),
            nonce: 1,
            timestamp: 0,
        };

        let result = execute_mint(
            &mut manager,
            &request,
            dummy_check_balance,
            dummy_deduct_balance,
        );

        assert!(result.success);
        assert_eq!(result.yes_shares, 100.0);
        assert_eq!(result.no_shares, 100.0);
        assert_eq!(result.bb_spent, 100.0);

        // Verify shares were credited
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::YES), 100.0);
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::NO), 100.0);
    }

    #[test]
    fn test_mint_insufficient_balance() {
        let mut manager = SharesManager::new();
        
        let request = MintRequest {
            market_id: "market1".to_string(),
            bb_amount: 100.0,
            wallet_address: "alice".to_string(),
            signature: "sig".to_string(),
            nonce: 1,
            timestamp: 0,
        };

        let result = execute_mint(
            &mut manager,
            &request,
            |_, _| false, // No balance
            dummy_deduct_balance,
        );

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_mint_amount_validation() {
        let mut manager = SharesManager::new();
        
        // Too small
        let request = MintRequest {
            market_id: "market1".to_string(),
            bb_amount: 0.001, // Below minimum
            wallet_address: "alice".to_string(),
            signature: "sig".to_string(),
            nonce: 1,
            timestamp: 0,
        };

        let result = execute_mint(
            &mut manager,
            &request,
            dummy_check_balance,
            dummy_deduct_balance,
        );

        assert!(!result.success);
        assert!(result.error.unwrap().contains("must be at least"));
    }

    #[test]
    fn test_calculate_mint_output() {
        let (yes, no, fee) = calculate_mint_output(100.0);
        
        // With 0% fee
        assert_eq!(yes, 100.0);
        assert_eq!(no, 100.0);
        assert_eq!(fee, 0.0);
    }

    #[test]
    fn test_calculate_mint_input() {
        let input = calculate_mint_input(100.0);
        
        // With 0% fee, input = output
        assert_eq!(input, 100.0);
    }
}
