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
use sha2::{Sha256, Digest};
use hex;
use std::fs;
use std::path::Path;

// ============================================================================
// EVENT DATES
// ============================================================================

/// Flexible date structure for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDates {
    /// Publication date (required) - ISO8601
    pub published: String,
    
    /// When betting freezes (optional) - ISO8601
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze: Option<String>,
    
    /// When market resolves (optional) - ISO8601
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

// ============================================================================
// RESOLUTION RULES
// ============================================================================

/// Resolution rules for market outcomes
/// Defines objective conditions for resolving each outcome
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResolutionRules {
    /// Optional resolution provider (e.g., "CoinGecko", "Manual")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    
    /// Optional data source URL for automated resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    
    /// Map of outcome name → resolution condition
    /// e.g., { "YES": "BTC price > $100k on Jan 1, 2025" }
    pub conditions: HashMap<String, String>,
}

impl ResolutionRules {
    /// Create new resolution rules
    pub fn new(conditions: HashMap<String, String>) -> Self {
        Self {
            provider: None,
            data_source: None,
            conditions,
        }
    }
    
    /// Create empty resolution rules
    pub fn empty() -> Self {
        Self {
            provider: None,
            data_source: None,
            conditions: HashMap::new(),
        }
    }
    
    /// Check if rules are defined for all outcomes
    pub fn has_rules_for(&self, outcomes: &[String]) -> bool {
        if self.conditions.is_empty() {
            return false;
        }
        outcomes.iter().all(|o| self.conditions.contains_key(o))
    }
}

// ============================================================================
// RSS EVENT PAYLOAD
// ============================================================================

/// RSS Event payload for initializing a prediction market
/// 
/// Flexible structure where scrapers send minimal data and L2 enriches with defaults.
/// Required: title, description, outcomes, source_url, dates.published
/// Optional: source, category, tags, initial_probabilities, image_url, resolution_rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssEvent {
    /// Market title (required)
    pub title: String,
    
    /// Market description (required)
    pub description: String,
    
    /// Source identifier (optional) - e.g., "AI_Scraper_v1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    
    /// Category (optional) - e.g., "crypto", "sports", "politics"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    
    /// Tags for filtering (optional)
    #[serde(default)]
    pub tags: Vec<String>,
    
    /// Market type: "binary", "three_choice", "multi" (default: "three_choice")
    #[serde(default = "default_market_type")]
    pub market_type: String,
    
    /// Betting outcomes (required) - e.g., ["Yes", "No Change", "No"]
    pub outcomes: Vec<String>,
    
    /// Initial probabilities (optional) - defaults to equal split if not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_probabilities: Option<Vec<f64>>,
    
    /// Source URL (required) - used for content hash deduplication
    pub source_url: String,
    
    /// Image URL for market display (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    
    /// Event dates (required: published; optional: freeze, resolution)
    pub dates: EventDates,
    
    /// Resolution rules (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_rules: Option<ResolutionRules>,
    
    /// Market ID assigned by L2 using content hash (internal)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub market_id: String,
    
    /// Whether market was added to ledger (internal)
    #[serde(skip_serializing, default)]
    pub added_to_ledger: bool,
}

fn default_market_type() -> String {
    "three_choice".to_string()
}

impl RssEvent {
    /// Get probabilities with fallback to equal split
    pub fn get_probabilities(&self) -> Vec<f64> {
        if let Some(ref probs) = self.initial_probabilities {
            probs.clone()
        } else {
            // Equal split across outcomes
            let equal_prob = 1.0 / self.outcomes.len() as f64;
            vec![equal_prob; self.outcomes.len()]
        }
    }
    
    /// Get category with fallback to "uncategorized"
    pub fn get_category(&self) -> String {
        self.category.clone().unwrap_or_else(|| "uncategorized".to_string())
    }
    
    /// Generate content hash for deduplication
    /// Uses SHA-256 of title + source_url
    pub fn generate_content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.title.as_bytes());
        hasher.update(self.source_url.as_bytes());
        hex::encode(hasher.finalize())
    }
    
    /// Validate the RSS event
    pub fn validate(&self) -> Result<(), String> {
        // Check title
        if self.title.is_empty() {
            return Err("title is required".to_string());
        }
        
        // Check description
        if self.description.is_empty() {
            return Err("description is required".to_string());
        }
        
        // Check source_url
        if self.source_url.is_empty() {
            return Err("source_url is required".to_string());
        }
        
        // Check at least 2 outcomes
        if self.outcomes.len() < 2 {
            return Err("At least 2 outcomes required".to_string());
        }
        
        // Check published date
        if self.dates.published.is_empty() {
            return Err("dates.published is required".to_string());
        }
        
        // If probabilities provided, validate them
        if let Some(ref probs) = self.initial_probabilities {
            if probs.len() != self.outcomes.len() {
                return Err(format!(
                    "initial_probabilities count ({}) must match outcomes count ({})",
                    probs.len(),
                    self.outcomes.len()
                ));
            }
            
            let sum: f64 = probs.iter().sum();
            if sum < 0.99 || sum > 1.01 {
                return Err(format!(
                    "initial_probabilities must sum to 1.0 (got {})",
                    sum
                ));
            }
            
            if probs.iter().any(|&p| p < 0.0 || p > 1.0) {
                return Err("All probabilities must be between 0.0 and 1.0".to_string());
            }
        }
        
        Ok(())
    }
    
    /// Convert initial probabilities to CPMM reserves
    /// 
    /// For a pool with total liquidity L and odds [p1, p2, p3],
    /// reserves are calculated to achieve those prices.
    /// 
    /// For binary: reserve_i = L * (1 - p_i)
    /// Price(i) = other_reserves / total = 1 - reserve_i/total = p_i
    pub fn calculate_initial_reserves(&self, total_liquidity: f64) -> Vec<f64> {
        let probs = self.get_probabilities();
        let n = self.outcomes.len() as f64;
        
        probs.iter().map(|&prob| {
            // Higher probability = lower reserve (more valuable = scarcer)
            total_liquidity * (1.0 - prob) / (n - 1.0)
        }).collect()
    }
    
    /// Get the dominant outcome (highest probability)
    pub fn get_favorite(&self) -> Option<(usize, &str, f64)> {
        let probs = self.get_probabilities();
        probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, &prob)| (i, self.outcomes[i].as_str(), prob))
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
// RSS FILE PERSISTENCE
// ============================================================================

/// Write RssEvent to RSS XML file in the rss/ folder
pub fn write_rss_event_to_file(event: &RssEvent, rss_dir: &str) -> Result<String, String> {
    // Create rss directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(rss_dir) {
        return Err(format!("Failed to create RSS directory: {}", e));
    }
    
    // Use full market_id as filename for simplicity
    let filename = format!("{}/{}.rss", rss_dir, &event.market_id);
    
    // Build RSS XML content
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:blackbook="https://blackbook.market/rss">
  <channel>
    <title>BlackBook Prediction Market</title>
    <link>https://blackbook.market</link>
    <description>Prediction market events on BlackBook Layer 2</description>
    <item>
      <guid isPermaLink="false">{}</guid>
      <title>{}</title>
      <description>{}</description>
      <link>{}</link>
      <pubDate>{}</pubDate>
      <blackbook:marketId>{}</blackbook:marketId>
      <blackbook:category>{}</blackbook:category>
      <blackbook:marketType>{}</blackbook:marketType>
      <blackbook:outcomes>{}</blackbook:outcomes>
      <blackbook:probabilities>{}</blackbook:probabilities>
      <blackbook:source>{}</blackbook:source>
      <blackbook:tags>{}</blackbook:tags>
      <blackbook:addedToLedger>{}</blackbook:addedToLedger>
      <blackbook:freezeDate>{}</blackbook:freezeDate>
      <blackbook:resolutionDate>{}</blackbook:resolutionDate>
    </item>
  </channel>
</rss>"#,
        event.market_id,
        escape_xml(&event.title),
        escape_xml(&event.description),
        escape_xml(&event.source_url),
        event.dates.published,
        event.market_id,
        escape_xml(&event.get_category()),
        escape_xml(&event.market_type),
        event.outcomes.join(","),
        event.get_probabilities().iter()
            .map(|p| format!("{:.4}", p))
            .collect::<Vec<_>>()
            .join(","),
        escape_xml(&event.source.clone().unwrap_or_else(|| "Unknown".to_string())),
        event.tags.join(","),
        event.added_to_ledger,
        event.dates.freeze.clone().unwrap_or_else(|| "TBD".to_string()),
        event.dates.resolution.clone().unwrap_or_else(|| "TBD".to_string())
    );
    
    // Write to file
    if let Err(e) = fs::write(&filename, xml) {
        return Err(format!("Failed to write RSS file: {}", e));
    }
    
    Ok(filename)
}

/// Load all RSS events from the rss/ folder
pub fn load_rss_events_from_folder(rss_dir: &str) -> Vec<RssEvent> {
    let mut events = Vec::new();
    
    // Read all .rss files in directory
    if let Ok(entries) = fs::read_dir(rss_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("rss") {
                if let Ok(content) = fs::read_to_string(&path) {
                    // Parse RSS XML and extract event data (simplified)
                    // In production, use a proper XML parser like quick-xml
                    if let Some(event) = parse_rss_xml(&content) {
                        events.push(event);
                    }
                }
            }
        }
    }
    
    events
}

/// Parse RSS XML content into RssEvent (simplified parser)
fn parse_rss_xml(xml: &str) -> Option<RssEvent> {
    // Extract values between XML tags
    let extract = |tag: &str| -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);
        xml.find(&start_tag)
            .and_then(|start| xml.find(&end_tag).map(|end| (start, end)))
            .map(|(start, end)| xml[start + start_tag.len()..end].to_string())
    };
    
    let market_id = extract("blackbook:marketId")?;
    let title = extract("title")?;
    let description = extract("description")?;
    let source_url = extract("link")?;
    let published = extract("pubDate")?;
    let category = extract("blackbook:category");
    let market_type = extract("blackbook:marketType").unwrap_or_else(|| "three_choice".to_string());
    let source = extract("blackbook:source");
    
    let outcomes_str = extract("blackbook:outcomes")?;
    let outcomes: Vec<String> = outcomes_str.split(',').map(|s| s.to_string()).collect();
    
    let probs_str = extract("blackbook:probabilities");
    let initial_probabilities = probs_str.and_then(|s| {
        let probs: Result<Vec<f64>, _> = s.split(',').map(|p| p.parse()).collect();
        probs.ok()
    });
    
    let tags_str = extract("blackbook:tags").unwrap_or_default();
    let tags: Vec<String> = if tags_str.is_empty() {
        Vec::new()
    } else {
        tags_str.split(',').map(|s| s.to_string()).collect()
    };
    
    let freeze = extract("blackbook:freezeDate").filter(|s| s != "TBD");
    let resolution = extract("blackbook:resolutionDate").filter(|s| s != "TBD");
    
    Some(RssEvent {
        market_id,
        title,
        description,
        source,
        category,
        tags,
        market_type,
        outcomes,
        initial_probabilities,
        source_url,
        image_url: None,
        dates: EventDates {
            published,
            freeze,
            resolution,
        },
        resolution_rules: None,
        added_to_ledger: true,
    })
}

/// Escape XML special characters
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
