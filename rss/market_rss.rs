// ============================================================================
// Market RSS Event Model
// ============================================================================
//
// Defines the payload structure for initializing prediction markets from
// RSS feed events. Supports 3-outcome markets (Yes/No Change/No) with
// initial odds and resolution rules.
//
// ============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// RESOLUTION RULES
// ============================================================================

/// Resolution rules for market outcomes
/// Defines objective conditions for resolving each outcome
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResolutionRules {
    /// Whether resolution rules are provided
    pub optional: bool,
    
    /// Map of outcome name → resolution condition
    /// e.g., { "YES": "BTC price > $100k on Jan 1, 2025" }
    pub rules: HashMap<String, String>,
}

impl ResolutionRules {
    /// Create new resolution rules
    pub fn new(rules: HashMap<String, String>) -> Self {
        Self {
            optional: false,
            rules,
        }
    }
    
    /// Create empty/optional resolution rules
    pub fn empty() -> Self {
        Self {
            optional: true,
            rules: HashMap::new(),
        }
    }
    
    /// Check if rules are defined for all outcomes
    pub fn has_rules_for(&self, outcomes: &[String]) -> bool {
        if self.optional {
            return true;
        }
        outcomes.iter().all(|o| self.rules.contains_key(o))
    }
}

// ============================================================================
// RSS EVENT PAYLOAD
// ============================================================================

/// RSS Event payload for initializing a prediction market
/// 
/// This is the format received from the RSS feed and used to create
/// a new market on-chain with initial liquidity and CPMM pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssEvent {
    /// Unique market ID (from RSS guid or hash)
    pub market_id: String,
    
    /// Market title for display
    pub meta_title: String,
    
    /// Market description
    pub meta_description: String,
    
    /// Market type: "binary", "three_choice", "multi"
    #[serde(default = "default_market_type")]
    pub market_type: String,
    
    /// Betting outcomes (e.g., ["Yes", "No Change", "No"])
    pub outcomes: Vec<String>,
    
    /// Initial odds for each outcome (should sum to 1.0)
    /// e.g., [0.49, 0.02, 0.49] for Yes/NoChange/No
    pub initial_odds: Vec<f64>,
    
    /// Source URL where event was discovered
    pub source: String,
    
    /// Publication date (ISO8601)
    pub pub_date: String,
    
    /// When the market resolves (ISO8601)
    pub resolution_date: String,
    
    /// When betting freezes (ISO8601) - typically before resolution
    pub freeze_date: String,
    
    /// Resolution rules for each outcome
    #[serde(default)]
    pub resolution_rules: Option<ResolutionRules>,
    
    /// Category: sports, crypto, politics, tech, business
    #[serde(default)]
    pub category: Option<String>,
    
    /// AI confidence score (0.0 - 1.0)
    #[serde(default)]
    pub confidence: Option<f64>,
}

fn default_market_type() -> String {
    "three_choice".to_string()
}

impl RssEvent {
    /// Create a new RSS event
    pub fn new(
        market_id: String,
        meta_title: String,
        meta_description: String,
        outcomes: Vec<String>,
        initial_odds: Vec<f64>,
        source: String,
        pub_date: String,
        resolution_date: String,
        freeze_date: String,
    ) -> Self {
        Self {
            market_id,
            meta_title,
            meta_description,
            market_type: default_market_type(),
            outcomes,
            initial_odds,
            source,
            pub_date,
            resolution_date,
            freeze_date,
            resolution_rules: None,
            category: None,
            confidence: None,
        }
    }
    
    /// Validate the RSS event
    pub fn validate(&self) -> Result<(), String> {
        // Check market_id
        if self.market_id.is_empty() {
            return Err("market_id is required".to_string());
        }
        
        // Check title
        if self.meta_title.is_empty() {
            return Err("meta_title is required".to_string());
        }
        
        // Check outcomes match initial_odds
        if self.outcomes.len() != self.initial_odds.len() {
            return Err(format!(
                "outcomes count ({}) must match initial_odds count ({})",
                self.outcomes.len(),
                self.initial_odds.len()
            ));
        }
        
        // Check at least 2 outcomes
        if self.outcomes.len() < 2 {
            return Err("At least 2 outcomes required".to_string());
        }
        
        // Check initial_odds sum to ~1.0 (with tolerance for floating point)
        let odds_sum: f64 = self.initial_odds.iter().sum();
        if odds_sum < 0.99 || odds_sum > 1.01 {
            return Err(format!(
                "initial_odds must sum to 1.0 (got {})",
                odds_sum
            ));
        }
        
        // Check all odds are positive
        if self.initial_odds.iter().any(|&o| o < 0.0) {
            return Err("All initial_odds must be non-negative".to_string());
        }
        
        Ok(())
    }
    
    /// Convert initial odds to CPMM reserves
    /// 
    /// For a pool with total liquidity L and odds [p1, p2, p3],
    /// reserves are calculated to achieve those prices.
    /// 
    /// For binary: reserve_i = L * (1 - p_i)
    /// Price(i) = other_reserves / total = 1 - reserve_i/total = p_i
    pub fn calculate_initial_reserves(&self, total_liquidity: f64) -> Vec<f64> {
        // For CPMM, price of outcome i = (sum of other reserves) / total
        // To achieve price p_i, we need reserve_i = L * (1 - p_i) / (n-1)
        // where n is number of outcomes
        
        let n = self.outcomes.len() as f64;
        
        self.initial_odds.iter().map(|&odds| {
            // Higher odds = lower reserve (more valuable = scarcer)
            total_liquidity * (1.0 - odds) / (n - 1.0)
        }).collect()
    }
    
    /// Get the dominant outcome (highest initial odds)
    pub fn get_favorite(&self) -> Option<(usize, &str, f64)> {
        self.initial_odds
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, &odds)| (i, self.outcomes[i].as_str(), odds))
    }
    
    /// Check if this is a standard Yes/No/NoChange market
    pub fn is_three_choice(&self) -> bool {
        self.outcomes.len() == 3 && self.market_type == "three_choice"
    }
    
    /// Check if this is a binary Yes/No market
    pub fn is_binary(&self) -> bool {
        self.outcomes.len() == 2
    }
}

// ============================================================================
// RSS FEED MANAGER
// ============================================================================

/// Manages RSS feed subscriptions and event processing
#[derive(Debug, Clone, Default)]
pub struct RssFeedManager {
    /// Processed market IDs (to avoid duplicates)
    pub processed_markets: Vec<String>,
    
    /// Feed URLs being monitored
    pub feed_urls: Vec<String>,
    
    /// Last poll timestamp
    pub last_poll: u64,
}

impl RssFeedManager {
    /// Create a new RSS feed manager
    pub fn new() -> Self {
        Self {
            processed_markets: Vec::new(),
            feed_urls: Vec::new(),
            last_poll: 0,
        }
    }
    
    /// Add a feed URL to monitor
    pub fn add_feed(&mut self, url: String) {
        if !self.feed_urls.contains(&url) {
            self.feed_urls.push(url);
        }
    }
    
    /// Check if a market has already been processed
    pub fn is_processed(&self, market_id: &str) -> bool {
        self.processed_markets.contains(&market_id.to_string())
    }
    
    /// Mark a market as processed
    pub fn mark_processed(&mut self, market_id: String) {
        if !self.processed_markets.contains(&market_id) {
            self.processed_markets.push(market_id);
        }
    }
    
    /// Update last poll timestamp
    pub fn update_poll_time(&mut self) {
        self.last_poll = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rss_event_validation() {
        let event = RssEvent::new(
            "test-market-1".to_string(),
            "Will BTC hit $100k?".to_string(),
            "Prediction market for Bitcoin price".to_string(),
            vec!["Yes".to_string(), "No Change".to_string(), "No".to_string()],
            vec![0.49, 0.02, 0.49],
            "https://example.com/article".to_string(),
            "2024-12-10T00:00:00Z".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            "2024-12-31T23:00:00Z".to_string(),
        );
        
        assert!(event.validate().is_ok());
        assert!(event.is_three_choice());
    }
    
    #[test]
    fn test_invalid_odds_sum() {
        let event = RssEvent::new(
            "test-market-2".to_string(),
            "Test".to_string(),
            "Test".to_string(),
            vec!["Yes".to_string(), "No".to_string()],
            vec![0.5, 0.3], // Sum = 0.8, invalid
            "https://example.com".to_string(),
            "2024-12-10T00:00:00Z".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            "2024-12-31T23:00:00Z".to_string(),
        );
        
        assert!(event.validate().is_err());
    }
    
    #[test]
    fn test_initial_reserves() {
        let event = RssEvent::new(
            "test-market-3".to_string(),
            "Test".to_string(),
            "Test".to_string(),
            vec!["Yes".to_string(), "No".to_string()],
            vec![0.6, 0.4], // 60% Yes, 40% No
            "https://example.com".to_string(),
            "2024-12-10T00:00:00Z".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            "2024-12-31T23:00:00Z".to_string(),
        );
        
        let reserves = event.calculate_initial_reserves(1000.0);
        assert_eq!(reserves.len(), 2);
        // Higher odds = lower reserve
        assert!(reserves[0] < reserves[1]); // Yes reserve < No reserve
    }
}
