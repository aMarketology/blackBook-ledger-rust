// Data models for the BlackBook prediction market

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::market_resolve::cpmm;

// Individual bet record for tracking outcomes and payouts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketBet {
    pub id: String,
    pub market_id: String,
    pub bettor: String,
    pub outcome: usize,
    pub amount: f64,
    pub timestamp: u64,
    pub status: String,
    pub payout: Option<f64>,
}

// Option-level statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OptionStats {
    pub total_volume: f64,
    pub bet_count: u64,
    pub unique_bettors: Vec<String>,
}

// Prediction market struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionMarket {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub options: Vec<String>,
    pub is_resolved: bool,
    pub winning_option: Option<usize>,
    pub escrow_address: String,
    pub created_at: u64,
    pub total_volume: f64,
    pub unique_bettors: Vec<String>,
    pub bet_count: u64,
    pub on_leaderboard: bool,
    pub option_stats: Vec<OptionStats>,
    pub bets: Vec<MarketBet>,
    
    #[serde(default)]
    pub market_status: cpmm::EventStatus,
    #[serde(default)]
    pub cpmm_pool: Option<cpmm::CPMMPool>,
    #[serde(default)]
    pub provisional_deadline: Option<u64>,
    #[serde(default)]
    pub betting_closes_at: Option<u64>,
    #[serde(default)]
    pub launched_by: Option<String>,
    #[serde(default)]
    pub source_event_id: Option<String>,
    
    // === NEW OPTIMAL FIELDS ===
    
    /// Source ID from external scraper/system
    #[serde(default)]
    pub source: Option<String>,
    
    /// Tags for search/filtering
    #[serde(default)]
    pub tags: Vec<String>,
    
    /// Market type: "binary", "three_choice", "multi"
    #[serde(default)]
    pub market_type: Option<String>,
    
    /// Initial probabilities
    #[serde(default)]
    pub initial_probabilities: Vec<f64>,
    
    /// Source article URL
    #[serde(default)]
    pub source_url: Option<String>,
    
    /// Preview image URL
    #[serde(default)]
    pub image_url: Option<String>,
    
    /// Lifecycle dates
    #[serde(default)]
    pub dates: Option<MarketDates>,
    
    /// Resolution rules and oracle config
    #[serde(default)]
    pub resolution_rules: Option<ResolutionRules>,
}

impl PredictionMarket {
    pub fn new(
        id: String,
        title: String,
        description: String,
        category: String,
        options: Vec<String>,
    ) -> Self {
        let option_count = options.len();
        Self {
            id,
            title,
            description,
            category,
            options,
            is_resolved: false,
            winning_option: None,
            escrow_address: format!("escrow_{}", uuid::Uuid::new_v4().simple()),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_volume: 0.0,
            unique_bettors: Vec::new(),
            bet_count: 0,
            on_leaderboard: false,
            option_stats: vec![OptionStats::default(); option_count],
            bets: Vec::new(),
            market_status: cpmm::EventStatus::Active,
            cpmm_pool: None,
            provisional_deadline: None,
            betting_closes_at: None,
            launched_by: None,
            source_event_id: None,
            // New optimal fields
            source: None,
            tags: Vec::new(),
            market_type: None,
            initial_probabilities: Vec::new(),
            source_url: None,
            image_url: None,
            dates: None,
            resolution_rules: None,
        }
    }

    pub fn record_bet(&mut self, bettor: &str, amount: f64, outcome: usize) -> String {
        let bet_id = format!("bet_{}_{}", self.id, uuid::Uuid::new_v4().simple());
        
        if !self.unique_bettors.contains(&bettor.to_string()) {
            self.unique_bettors.push(bettor.to_string());
            if self.unique_bettors.len() >= 10 && !self.on_leaderboard {
                self.on_leaderboard = true;
            }
        }
        
        self.total_volume += amount;
        self.bet_count += 1;
        
        if outcome < self.option_stats.len() {
            let stats = &mut self.option_stats[outcome];
            stats.total_volume += amount;
            stats.bet_count += 1;
            if !stats.unique_bettors.contains(&bettor.to_string()) {
                stats.unique_bettors.push(bettor.to_string());
            }
        }
        
        let bet = MarketBet {
            id: bet_id.clone(),
            market_id: self.id.clone(),
            bettor: bettor.to_string(),
            outcome,
            amount,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            status: "PENDING".to_string(),
            payout: None,
        };
        
        self.bets.push(bet);
        bet_id
    }

    pub fn calculate_odds(&self) -> Vec<f64> {
        // Use CPMM prices if pool exists (dynamic odds)
        if let Some(ref pool) = self.cpmm_pool {
            return pool.calculate_prices();
        }
        
        // Fallback to initial probabilities if set
        if !self.initial_probabilities.is_empty() && self.total_volume == 0.0 {
            return self.initial_probabilities.clone();
        }
        
        // Fallback to volume-weighted odds
        if self.total_volume == 0.0 {
            let equal_prob = 1.0 / self.options.len() as f64;
            return vec![equal_prob; self.options.len()];
        }
        
        self.option_stats
            .iter()
            .map(|stat| {
                if self.total_volume > 0.0 {
                    stat.total_volume / self.total_volume
                } else {
                    0.0
                }
            })
            .collect()
    }

    pub fn get_bets_for_account(&self, account: &str) -> Vec<MarketBet> {
        self.bets
            .iter()
            .filter(|b| b.bettor == account)
            .cloned()
            .collect()
    }
}

// Request/Response structs
#[derive(Debug, Deserialize)]
pub struct SignedBetRequest {
    pub signed_tx: crate::rpc::SignedTransaction,
}

#[derive(Debug, Serialize)]
pub struct SignedBetResponse {
    pub success: bool,
    pub bet_id: Option<String>,
    pub transaction_id: Option<String>,
    pub market_id: Option<String>,
    pub outcome: Option<usize>,
    pub amount: Option<f64>,
    pub new_balance: Option<f64>,
    pub nonce_used: Option<u64>,
    pub error: Option<String>,
    // CPMM pricing fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_purchased: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_impact: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_paid: Option<f64>,
}

impl SignedBetResponse {
    pub fn error(msg: &str) -> Self {
        Self {
            success: false,
            bet_id: None,
            transaction_id: None,
            market_id: None,
            outcome: None,
            amount: None,
            new_balance: None,
            nonce_used: None,
            error: Some(msg.to_string()),
            entry_price: None,
            shares_purchased: None,
            price_impact: None,
            new_price: None,
            fee_paid: None,
        }
    }
}

/// Dates for market lifecycle
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MarketDates {
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub freeze: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
}

/// Resolution rules configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResolutionRules {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub data_source: Option<String>,
    #[serde(default)]
    pub conditions: Option<std::collections::HashMap<String, String>>,
}

/// POST /markets request body
/// 
/// # Minimal Payload (3 fields):
/// ```json
/// {
///   "title": "Will X happen?",
///   "description": "Market resolves YES if...",
///   "outcomes": ["Yes", "No"]
/// }
/// ```
/// 
/// # Optimal Payload (full structure):
/// ```json
/// {
///   "source": "article_123_abc",
///   "title": "Bitcoin breaks $100k before 2025?",
///   "description": "Market resolves YES if BTC > $100k...",
///   "category": "crypto",
///   "tags": ["bitcoin", "price-action"],
///   "market_type": "binary",
///   "outcomes": ["Yes", "No"],
///   "initial_probabilities": [0.49, 0.51],
///   "source_url": "https://...",
///   "image_url": "https://...",
///   "dates": { "published": "...", "freeze": null, "resolution": null },
///   "resolution_rules": { "provider": "oracle_v1", "data_source": "...", "conditions": {...} }
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct CreateMarketRequest {
    // === REQUIRED FIELDS ===
    pub title: String,
    pub description: String,
    pub outcomes: Vec<String>,
    
    // === OPTIONAL FIELDS ===
    
    /// Source ID from scraper/external system (also accepts "id" or "market_id")
    #[serde(default, alias = "id", alias = "market_id")]
    pub source: Option<String>,
    
    /// Category: "crypto", "sports", "politics", "tech", etc.
    #[serde(default)]
    pub category: Option<String>,
    
    /// Tags for search/filtering
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    
    /// Market type: "binary", "three_choice", "multi" (default: "binary")
    #[serde(default)]
    pub market_type: Option<String>,
    
    /// Initial probabilities (defaults to equal split if null)
    #[serde(default)]
    pub initial_probabilities: Option<Vec<f64>>,
    
    /// Source article URL
    #[serde(default)]
    pub source_url: Option<String>,
    
    /// Preview image URL
    #[serde(default)]
    pub image_url: Option<String>,
    
    /// Lifecycle dates
    #[serde(default)]
    pub dates: Option<MarketDates>,
    
    /// Resolution rules and oracle config
    #[serde(default)]
    pub resolution_rules: Option<ResolutionRules>,
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub from: String,
    pub to: String,
    pub amount: f64,
}
