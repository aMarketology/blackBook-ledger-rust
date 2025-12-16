// ============================================================================
// Share Redemption - BlackBook Prediction Market
// ============================================================================
//
// Redemption converts outcome shares back into BB tokens.
//
// Two Redemption Types:
//
// 1. PAIRED REDEMPTION (Anytime):
//    1 YES share + 1 NO share → 1 BB
//    This is the inverse of minting and can be done at any time.
//    Enables arbitrage when YES + NO prices deviate from 1.00.
//
// 2. RESOLUTION REDEMPTION (After Market Resolves):
//    Winning shares → 1 BB each
//    Losing shares → 0 BB (burned)
//
// Why Paired Redemption Matters:
//   - If you can buy YES for 0.40 and NO for 0.50 (total 0.90)
//   - You can redeem the pair for 1.00 BB
//   - Profit = 0.10 BB per pair (arbitrage)
//   - This arbitrage keeps markets efficient
//
// ============================================================================

use super::{OutcomeIndex, SharesManager, ShareTxType, MIN_SHARE_SIZE, MAX_SHARE_SIZE};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// REDEEM REQUEST/RESPONSE
// ============================================================================

/// Request to redeem shares (paired redemption)
#[derive(Debug, Clone, Deserialize)]
pub struct RedeemRequest {
    /// Market ID
    pub market_id: String,
    
    /// Number of complete sets to redeem (1 YES + 1 NO = 1 set)
    pub sets: f64,
    
    /// Wallet address
    pub wallet_address: String,
    
    /// Signature for verification
    pub signature: String,
    
    /// Nonce for replay protection
    pub nonce: u64,
    
    /// Timestamp
    pub timestamp: u64,
}

/// Result of a redemption operation
#[derive(Debug, Clone, Serialize)]
pub struct RedeemResult {
    pub success: bool,
    pub redeem_id: Option<String>,
    pub market_id: String,
    pub wallet: String,
    /// Number of complete sets redeemed
    pub sets_redeemed: f64,
    /// YES shares burned
    pub yes_burned: f64,
    /// NO shares burned
    pub no_burned: f64,
    /// BB received
    pub bb_received: f64,
    /// Transaction fee
    pub fee: f64,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl RedeemResult {
    pub fn success(
        redeem_id: String,
        market_id: String,
        wallet: String,
        sets: f64,
        bb_received: f64,
        fee: f64,
    ) -> Self {
        Self {
            success: true,
            redeem_id: Some(redeem_id),
            market_id,
            wallet,
            sets_redeemed: sets,
            yes_burned: sets,
            no_burned: sets,
            bb_received,
            fee,
            error: None,
            timestamp: now(),
        }
    }

    pub fn error(market_id: String, wallet: String, msg: String) -> Self {
        Self {
            success: false,
            redeem_id: None,
            market_id,
            wallet,
            sets_redeemed: 0.0,
            yes_burned: 0.0,
            no_burned: 0.0,
            bb_received: 0.0,
            fee: 0.0,
            error: Some(msg),
            timestamp: now(),
        }
    }
}

// ============================================================================
// REDEMPTION LOGIC
// ============================================================================

/// Redemption fee rate (0% - redemption is free)
pub const REDEEM_FEE_RATE: f64 = 0.0;

/// Execute paired redemption (1 YES + 1 NO → 1 BB)
///
/// Burns equal amounts of YES and NO shares and returns BB.
/// User must have at least `sets` of both YES and NO shares.
///
/// # Arguments
/// * `shares_manager` - The shares manager
/// * `request` - The redemption request
/// * `credit_balance` - Function to credit BB to user's balance
///
/// # Returns
/// * `RedeemResult` - The result of the redemption
pub fn execute_paired_redeem<F>(
    shares_manager: &mut SharesManager,
    request: &RedeemRequest,
    credit_balance: F,
) -> RedeemResult
where
    F: FnOnce(&str, f64) -> Result<(), String>,
{
    // Validate amount
    if request.sets < MIN_SHARE_SIZE {
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Sets must be at least {}", MIN_SHARE_SIZE),
        );
    }

    if request.sets > MAX_SHARE_SIZE {
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Sets must be at most {}", MAX_SHARE_SIZE),
        );
    }

    // Check YES shares
    let yes_shares = shares_manager.get_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::YES,
    );

    if yes_shares < request.sets {
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Insufficient YES shares: have {}, need {}", yes_shares, request.sets),
        );
    }

    // Check NO shares
    let no_shares = shares_manager.get_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::NO,
    );

    if no_shares < request.sets {
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Insufficient NO shares: have {}, need {}", no_shares, request.sets),
        );
    }

    // Calculate fee and payout
    let gross_bb = request.sets; // 1:1 redemption
    let fee = gross_bb * REDEEM_FEE_RATE;
    let net_bb = gross_bb - fee;

    // Burn YES shares
    if let Err(e) = shares_manager.debit_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::YES,
        request.sets,
        0.5, // Redemption price
        ShareTxType::Redeem,
    ) {
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Failed to burn YES shares: {}", e),
        );
    }

    // Burn NO shares
    if let Err(e) = shares_manager.debit_shares(
        &request.wallet_address,
        &request.market_id,
        OutcomeIndex::NO,
        request.sets,
        0.5, // Redemption price
        ShareTxType::Redeem,
    ) {
        // Rollback YES burn (in production, this should be atomic)
        shares_manager.credit_shares(
            &request.wallet_address,
            &request.market_id,
            OutcomeIndex::YES,
            request.sets,
            0.5,
            ShareTxType::Mint, // Credit back
        );
        
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Failed to burn NO shares: {}", e),
        );
    }

    // Credit BB to user
    if let Err(e) = credit_balance(&request.wallet_address, net_bb) {
        // Rollback share burns (in production, this should be atomic)
        shares_manager.credit_shares(
            &request.wallet_address,
            &request.market_id,
            OutcomeIndex::YES,
            request.sets,
            0.5,
            ShareTxType::Mint,
        );
        shares_manager.credit_shares(
            &request.wallet_address,
            &request.market_id,
            OutcomeIndex::NO,
            request.sets,
            0.5,
            ShareTxType::Mint,
        );
        
        return RedeemResult::error(
            request.market_id.clone(),
            request.wallet_address.clone(),
            format!("Failed to credit BB: {}", e),
        );
    }

    // Update stats
    shares_manager.stats.total_bb_locked -= net_bb;

    // Generate redeem ID
    let redeem_id = format!("redeem_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_string());

    RedeemResult::success(
        redeem_id,
        request.market_id.clone(),
        request.wallet_address.clone(),
        request.sets,
        net_bb,
        fee,
    )
}

/// Execute resolution redemption (winning shares → BB)
///
/// Called after a market resolves. Pays out winning shares 1:1 and burns losing shares.
///
/// # Arguments
/// * `shares_manager` - The shares manager
/// * `market_id` - The market that resolved
/// * `winning_outcome` - The winning outcome index
/// * `credit_balance` - Function to credit BB to user's balance
///
/// # Returns
/// * Vec of (wallet, payout_amount) tuples
pub fn execute_resolution_redeem<F>(
    shares_manager: &mut SharesManager,
    market_id: &str,
    winning_outcome: OutcomeIndex,
    num_outcomes: usize,
    mut credit_balance: F,
) -> Vec<ResolutionPayout>
where
    F: FnMut(&str, f64) -> Result<(), String>,
{
    // Get all payouts from resolution
    let payouts = shares_manager.resolve_market(market_id, winning_outcome, num_outcomes);
    
    let mut results = Vec::new();
    
    for (wallet, amount) in payouts {
        let payout_id = format!("payout_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_string());
        
        // Credit BB to winner
        let success = credit_balance(&wallet, amount).is_ok();
        
        results.push(ResolutionPayout {
            payout_id,
            market_id: market_id.to_string(),
            wallet: wallet.clone(),
            winning_shares: amount,
            bb_paid: if success { amount } else { 0.0 },
            success,
            timestamp: now(),
        });
    }
    
    results
}

/// Resolution payout record
#[derive(Debug, Clone, Serialize)]
pub struct ResolutionPayout {
    pub payout_id: String,
    pub market_id: String,
    pub wallet: String,
    pub winning_shares: f64,
    pub bb_paid: f64,
    pub success: bool,
    pub timestamp: u64,
}

// ============================================================================
// ARBITRAGE HELPERS
// ============================================================================

/// Check if arbitrage opportunity exists (YES + NO != 1.00)
///
/// # Arguments
/// * `yes_price` - Current YES share price (0.00 - 1.00)
/// * `no_price` - Current NO share price (0.00 - 1.00)
/// * `min_profit_bps` - Minimum profit in basis points to consider (e.g., 50 = 0.5%)
///
/// # Returns
/// * Some(ArbitrageOpportunity) if profitable, None otherwise
pub fn check_arbitrage_opportunity(
    yes_price: f64,
    no_price: f64,
    min_profit_bps: u64,
) -> Option<ArbitrageOpportunity> {
    let total_cost = yes_price + no_price;
    
    if total_cost < 1.0 {
        // Can buy both and redeem for profit
        let profit = 1.0 - total_cost;
        let profit_bps = (profit * 10000.0) as u64;
        
        if profit_bps >= min_profit_bps {
            return Some(ArbitrageOpportunity {
                opportunity_type: ArbitrageType::BuyAndRedeem,
                yes_price,
                no_price,
                profit_per_set: profit,
                profit_bps,
                action: "Buy 1 YES + 1 NO, redeem for 1 BB".to_string(),
            });
        }
    } else if total_cost > 1.0 {
        // Can mint and sell both for profit
        let profit = total_cost - 1.0;
        let profit_bps = (profit * 10000.0) as u64;
        
        if profit_bps >= min_profit_bps {
            return Some(ArbitrageOpportunity {
                opportunity_type: ArbitrageType::MintAndSell,
                yes_price,
                no_price,
                profit_per_set: profit,
                profit_bps,
                action: "Mint with 1 BB, sell YES + NO".to_string(),
            });
        }
    }
    
    None
}

/// Type of arbitrage opportunity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ArbitrageType {
    /// Buy YES and NO shares, redeem for BB (when total < 1.00)
    BuyAndRedeem,
    /// Mint shares with BB, sell both (when total > 1.00)
    MintAndSell,
}

/// Arbitrage opportunity details
#[derive(Debug, Clone, Serialize)]
pub struct ArbitrageOpportunity {
    pub opportunity_type: ArbitrageType,
    pub yes_price: f64,
    pub no_price: f64,
    pub profit_per_set: f64,
    pub profit_bps: u64,
    pub action: String,
}

/// Calculate maximum sets that can be redeemed given current shares
pub fn max_redeemable_sets(yes_shares: f64, no_shares: f64) -> f64 {
    yes_shares.min(no_shares)
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

    fn dummy_credit_balance(_wallet: &str, _amount: f64) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn test_paired_redeem() {
        let mut manager = SharesManager::new();
        
        // First mint some shares
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.5, ShareTxType::Mint);
        manager.credit_shares("alice", "market1", OutcomeIndex::NO, 100.0, 0.5, ShareTxType::Mint);

        let request = RedeemRequest {
            market_id: "market1".to_string(),
            sets: 50.0,
            wallet_address: "alice".to_string(),
            signature: "sig".to_string(),
            nonce: 1,
            timestamp: 0,
        };

        let result = execute_paired_redeem(&mut manager, &request, dummy_credit_balance);

        assert!(result.success);
        assert_eq!(result.sets_redeemed, 50.0);
        assert_eq!(result.bb_received, 50.0);

        // Check remaining shares
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::YES), 50.0);
        assert_eq!(manager.get_shares("alice", "market1", OutcomeIndex::NO), 50.0);
    }

    #[test]
    fn test_redeem_insufficient_yes() {
        let mut manager = SharesManager::new();
        
        // Only have YES shares
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.5, ShareTxType::Mint);
        manager.credit_shares("alice", "market1", OutcomeIndex::NO, 30.0, 0.5, ShareTxType::Mint);

        let request = RedeemRequest {
            market_id: "market1".to_string(),
            sets: 50.0, // Try to redeem more than we have NO shares
            wallet_address: "alice".to_string(),
            signature: "sig".to_string(),
            nonce: 1,
            timestamp: 0,
        };

        let result = execute_paired_redeem(&mut manager, &request, dummy_credit_balance);

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Insufficient NO shares"));
    }

    #[test]
    fn test_resolution_redeem() {
        let mut manager = SharesManager::new();
        
        // Alice has YES, Bob has NO
        manager.credit_shares("alice", "market1", OutcomeIndex::YES, 100.0, 0.6, ShareTxType::Buy);
        manager.credit_shares("bob", "market1", OutcomeIndex::NO, 100.0, 0.4, ShareTxType::Buy);

        let payouts = execute_resolution_redeem(
            &mut manager,
            "market1",
            OutcomeIndex::YES, // YES wins
            2,
            |_, _| Ok(()),
        );

        // Alice should get paid, Bob should not
        assert!(payouts.iter().any(|p| p.wallet == "alice" && p.bb_paid == 100.0));
        assert!(!payouts.iter().any(|p| p.wallet == "bob" && p.bb_paid > 0.0));
    }

    #[test]
    fn test_check_arbitrage_buy_and_redeem() {
        // YES at 0.40, NO at 0.50 = 0.90 total (buy both, redeem for 1.00, profit 0.10)
        let arb = check_arbitrage_opportunity(0.40, 0.50, 50);
        
        assert!(arb.is_some());
        let arb = arb.unwrap();
        assert_eq!(arb.opportunity_type, ArbitrageType::BuyAndRedeem);
        assert!((arb.profit_per_set - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_check_arbitrage_mint_and_sell() {
        // YES at 0.55, NO at 0.55 = 1.10 total (mint for 1.00, sell both for 1.10, profit 0.10)
        let arb = check_arbitrage_opportunity(0.55, 0.55, 50);
        
        assert!(arb.is_some());
        let arb = arb.unwrap();
        assert_eq!(arb.opportunity_type, ArbitrageType::MintAndSell);
        assert!((arb.profit_per_set - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_no_arbitrage() {
        // YES at 0.50, NO at 0.50 = 1.00 total (no arbitrage)
        let arb = check_arbitrage_opportunity(0.50, 0.50, 50);
        assert!(arb.is_none());
    }

    #[test]
    fn test_max_redeemable_sets() {
        assert_eq!(max_redeemable_sets(100.0, 50.0), 50.0);
        assert_eq!(max_redeemable_sets(30.0, 100.0), 30.0);
        assert_eq!(max_redeemable_sets(75.0, 75.0), 75.0);
    }
}
