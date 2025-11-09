use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Production Prediction Market System
/// Manages markets, bets, and outcome resolution

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    /// Unique market identifier
    pub id: String,

    /// Market question/title
    pub title: String,

    /// Detailed description
    pub description: String,

    /// Market status: open, closed, resolved
    pub status: MarketStatus,

    /// Outcome options (e.g., ["Yes", "No"] or ["Trump", "Harris", "Other"])
    pub outcomes: Vec<String>,

    /// Current odds for each outcome (should sum to ~1.0)
    pub outcome_odds: Vec<f64>,

    /// Total volume on each outcome
    pub outcome_volumes: Vec<f64>,

    /// Winning outcome index (None if not resolved)
    pub winning_outcome: Option<usize>,

    /// Market resolution source/description
    pub resolution_source: String,

    /// Creation timestamp
    pub created_at: u64,

    /// Resolution timestamp (if resolved)
    pub resolved_at: Option<u64>,

    /// Market category (sports, politics, crypto, etc)
    pub category: String,

    /// Total liquidity available
    pub liquidity: f64,

    /// 24h trading volume
    pub volume_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MarketStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "resolved")]
    Resolved,
}

/// User bet on a market outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bet {
    /// Unique bet ID
    pub id: String,

    /// Account name who placed bet
    pub account: String,

    /// Market ID
    pub market_id: String,

    /// Outcome index chosen
    pub outcome_index: usize,

    /// Amount wagered
    pub amount: f64,

    /// Odds at time of bet
    pub odds_at_bet: f64,

    /// Potential payout if bet wins
    pub potential_payout: f64,

    /// Actual payout (if resolved)
    pub payout: Option<f64>,

    /// Bet status: pending, won, lost
    pub status: BetStatus,

    /// Timestamp of bet
    pub created_at: u64,

    /// Resolved at timestamp
    pub resolved_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BetStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "won")]
    Won,
    #[serde(rename = "lost")]
    Lost,
}

/// Market manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketManager {
    /// All markets
    pub markets: HashMap<String, Market>,

    /// All bets keyed by bet ID
    pub bets: HashMap<String, Bet>,

    /// Bets by account
    pub account_bets: HashMap<String, Vec<String>>,

    /// Market IDs sorted by volume
    pub hot_markets: Vec<String>,
}

impl MarketManager {
    /// Create new market manager
    pub fn new() -> Self {
        Self {
            markets: HashMap::new(),
            bets: HashMap::new(),
            account_bets: HashMap::new(),
            hot_markets: Vec::new(),
        }
    }

    /// Create a new market
    pub fn create_market(
        &mut self,
        id: String,
        title: String,
        description: String,
        outcomes: Vec<String>,
        category: String,
        resolution_source: String,
    ) -> Result<String, String> {
        if self.markets.contains_key(&id) {
            return Err(format!("Market {} already exists", id));
        }

        if outcomes.len() < 2 {
            return Err("Market must have at least 2 outcomes".to_string());
        }

        let num_outcomes = outcomes.len();
        let equal_odds = 1.0 / num_outcomes as f64;

        let market = Market {
            id: id.clone(),
            title,
            description,
            status: MarketStatus::Open,
            outcomes,
            outcome_odds: vec![equal_odds; num_outcomes],
            outcome_volumes: vec![0.0; num_outcomes],
            winning_outcome: None,
            resolution_source,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            resolved_at: None,
            category,
            liquidity: 0.0,
            volume_24h: 0.0,
        };

        self.markets.insert(id.clone(), market);
        Ok(format!("Market {} created successfully", id))
    }

    /// Place a bet on a market outcome
    pub fn place_bet(
        &mut self,
        bet_id: String,
        account: String,
        market_id: String,
        outcome_index: usize,
        amount: f64,
    ) -> Result<Bet, String> {
        // Validate market exists and is open
        let market = self.markets.get_mut(&market_id)
            .ok_or_else(|| format!("Market {} not found", market_id))?;

        if market.status != MarketStatus::Open {
            return Err(format!("Market {} is not open for betting", market_id));
        }

        if outcome_index >= market.outcomes.len() {
            return Err(format!("Invalid outcome index {}", outcome_index));
        }

        if amount <= 0.0 {
            return Err("Bet amount must be positive".to_string());
        }

        // Get current odds for this outcome
        let odds_at_bet = market.outcome_odds[outcome_index];
        let potential_payout = amount * (1.0 / odds_at_bet);

        let bet = Bet {
            id: bet_id.clone(),
            account: account.clone(),
            market_id: market_id.clone(),
            outcome_index,
            amount,
            odds_at_bet,
            potential_payout,
            payout: None,
            status: BetStatus::Pending,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            resolved_at: None,
        };

        // Update market volume
        market.outcome_volumes[outcome_index] += amount;
        market.volume_24h += amount;
        market.liquidity += amount;

        // Rebalance odds using constant product market maker formula (simplified)
        self.rebalance_odds(&market_id);

        // Store bet
        self.bets.insert(bet_id.clone(), bet.clone());

        // Track bet by account
        self.account_bets
            .entry(account.clone())
            .or_insert_with(Vec::new)
            .push(bet_id);

        Ok(bet)
    }

    /// Rebalance odds using constant product market maker (simplified version)
    fn rebalance_odds(&mut self, market_id: &str) {
        if let Some(market) = self.markets.get_mut(market_id) {
            let total_volume: f64 = market.outcome_volumes.iter().sum();
            if total_volume > 0.0 {
                // Update odds proportionally to volume
                let new_odds: Vec<f64> = market
                    .outcome_volumes
                    .iter()
                    .map(|v| (total_volume - v) / total_volume)
                    .collect();

                // Normalize odds to sum to 1.0
                let odds_sum: f64 = new_odds.iter().sum();
                market.outcome_odds = new_odds.iter().map(|o| o / odds_sum).collect();
            }
        }
    }

    /// Close a market (stop accepting bets)
    pub fn close_market(&mut self, market_id: String) -> Result<String, String> {
        let market = self.markets.get_mut(&market_id)
            .ok_or_else(|| format!("Market {} not found", market_id))?;

        market.status = MarketStatus::Closed;
        Ok(format!("Market {} closed", market_id))
    }

    /// Resolve a market with winning outcome
    pub fn resolve_market(
        &mut self,
        market_id: String,
        winning_outcome: usize,
    ) -> Result<Vec<(String, f64)>, String> {
        let market = self.markets.get_mut(&market_id)
            .ok_or_else(|| format!("Market {} not found", market_id))?;

        if winning_outcome >= market.outcomes.len() {
            return Err(format!("Invalid outcome index {}", winning_outcome));
        }

        market.status = MarketStatus::Resolved;
        market.winning_outcome = Some(winning_outcome);
        market.resolved_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        // Calculate payouts
        let mut payouts = Vec::new();
        let total_losing_volume: f64 = market
            .outcome_volumes
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != winning_outcome)
            .map(|(_, v)| v)
            .sum();

        // Find all winning bets
        for (bet_id, bet) in self.bets.iter_mut() {
            if bet.market_id == market_id && bet.status == BetStatus::Pending {
                if bet.outcome_index == winning_outcome {
                    // Winner: gets their stake back + share of losing volume
                    let payout = bet.amount + (bet.amount / market.outcome_volumes[winning_outcome]) * total_losing_volume;
                    bet.payout = Some(payout);
                    bet.status = BetStatus::Won;
                    bet.resolved_at = market.resolved_at;
                    payouts.push((bet.account.clone(), payout));
                } else {
                    // Loser: loses their bet
                    bet.status = BetStatus::Lost;
                    bet.resolved_at = market.resolved_at;
                }
            }
        }

        Ok(payouts)
    }

    /// Get all open markets
    pub fn get_open_markets(&self) -> Vec<Market> {
        self.markets
            .values()
            .filter(|m| m.status == MarketStatus::Open)
            .cloned()
            .collect()
    }

    /// Get active bets for an account
    pub fn get_account_bets(&self, account: &str) -> Vec<Bet> {
        self.account_bets
            .get(account)
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|bet_id| self.bets.get(bet_id).cloned())
            .collect()
    }

    /// Get market statistics
    pub fn get_market_stats(&self, market_id: &str) -> Option<MarketStats> {
        self.markets.get(market_id).map(|m| MarketStats {
            market_id: market_id.to_string(),
            title: m.title.clone(),
            status: format!("{:?}", m.status),
            outcomes: m.outcomes.clone(),
            outcome_odds: m.outcome_odds.clone(),
            outcome_volumes: m.outcome_volumes.clone(),
            total_volume: m.outcome_volumes.iter().sum(),
            total_liquidity: m.liquidity,
            volume_24h: m.volume_24h,
            winning_outcome: m.winning_outcome.map(|idx| m.outcomes[idx].clone()),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct MarketStats {
    pub market_id: String,
    pub title: String,
    pub status: String,
    pub outcomes: Vec<String>,
    pub outcome_odds: Vec<f64>,
    pub outcome_volumes: Vec<f64>,
    pub total_volume: f64,
    pub total_liquidity: f64,
    pub volume_24h: f64,
    pub winning_outcome: Option<String>,
}
