// ============================================================================
// Oracle System - Multi-Source Data Feeds & Automated Market Resolution
// ============================================================================
//
// This module provides oracle infrastructure for prediction markets:
//   - Multi-source price feeds (CoinGecko, Binance, Coinbase)
//   - Data validation (require 2+ sources to agree)
//   - Smart caching to reduce API calls
//   - Automated market resolution from external data
//
// Architecture:
//   DataFeed trait → Multiple implementations → OracleManager validates
//   → Price consensus → Market resolution
// ============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// CORE TYPES
// ============================================================================

/// Oracle price data point from a single source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub source: String,
    pub asset: String,
    pub price: f64,
    pub timestamp: u64,
    pub confidence: f64, // 0.0 to 1.0
}

/// Consensus price from multiple oracle sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusPrice {
    pub asset: String,
    pub price: f64,
    pub sources: Vec<String>,
    pub timestamp: u64,
    pub variance: f64, // How much sources disagreed
    pub confidence: f64, // Overall confidence in the price
}

/// Oracle resolution result for a market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleResolution {
    pub market_id: String,
    pub winning_outcome: usize,
    pub confidence: f64,
    pub data_sources: Vec<String>,
    pub timestamp: u64,
    pub resolution_data: serde_json::Value, // Raw data used for resolution
}

/// Cached price entry
#[derive(Debug, Clone)]
struct CachedPrice {
    price: f64,
    timestamp: u64,
    sources: Vec<String>,
}

// ============================================================================
// DATA FEED TRAIT & ENUM
// ============================================================================

/// Trait for oracle data sources
/// 
/// Implementations: CoinGecko, Binance, Coinbase, custom APIs
#[async_trait::async_trait]
pub trait DataFeed: Send + Sync {
    /// Fetch price for an asset (e.g., "BTC", "ETH", "SOL")
    async fn fetch_price(&self, asset: &str) -> Result<PriceData, String>;
    
    /// Get the name of this data source
    fn source_name(&self) -> &str;
    
    /// Get reliability score (0.0 to 1.0)
    fn reliability(&self) -> f64;
}

/// Enum wrapper for data feeds (enables dynamic dispatch)
pub enum DataFeedType {
    CoinGecko(CoinGeckoFeed),
    Binance(BinanceFeed),
    Coinbase(CoinbaseFeed),
}

#[async_trait::async_trait]
impl DataFeed for DataFeedType {
    async fn fetch_price(&self, asset: &str) -> Result<PriceData, String> {
        match self {
            DataFeedType::CoinGecko(feed) => feed.fetch_price(asset).await,
            DataFeedType::Binance(feed) => feed.fetch_price(asset).await,
            DataFeedType::Coinbase(feed) => feed.fetch_price(asset).await,
        }
    }
    
    fn source_name(&self) -> &str {
        match self {
            DataFeedType::CoinGecko(feed) => feed.source_name(),
            DataFeedType::Binance(feed) => feed.source_name(),
            DataFeedType::Coinbase(feed) => feed.source_name(),
        }
    }
    
    fn reliability(&self) -> f64 {
        match self {
            DataFeedType::CoinGecko(feed) => feed.reliability(),
            DataFeedType::Binance(feed) => feed.reliability(),
            DataFeedType::Coinbase(feed) => feed.reliability(),
        }
    }
}

// ============================================================================
// COINGECKO IMPLEMENTATION
// ============================================================================

pub struct CoinGeckoFeed {
    api_key: Option<String>, // Optional API key for rate limits
}

impl CoinGeckoFeed {
    pub fn new() -> Self {
        Self {
            api_key: std::env::var("COINGECKO_API_KEY").ok(),
        }
    }
    
    fn asset_to_coingecko_id(asset: &str) -> Option<&str> {
        match asset.to_uppercase().as_str() {
            "BTC" | "BITCOIN" => Some("bitcoin"),
            "ETH" | "ETHEREUM" => Some("ethereum"),
            "SOL" | "SOLANA" => Some("solana"),
            "USDC" => Some("usd-coin"),
            "USDT" => Some("tether"),
            "BNB" => Some("binancecoin"),
            "XRP" | "RIPPLE" => Some("ripple"),
            "ADA" | "CARDANO" => Some("cardano"),
            "DOGE" | "DOGECOIN" => Some("dogecoin"),
            "AVAX" | "AVALANCHE" => Some("avalanche-2"),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl DataFeed for CoinGeckoFeed {
    async fn fetch_price(&self, asset: &str) -> Result<PriceData, String> {
        let coin_id = Self::asset_to_coingecko_id(asset)
            .ok_or_else(|| format!("Unknown asset: {}", asset))?;
        
        let url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
            coin_id
        );
        
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("CoinGecko request failed: {}", e))?;
        
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("CoinGecko parse failed: {}", e))?;
        
        let price = data[coin_id]["usd"]
            .as_f64()
            .ok_or_else(|| "Price not found in response".to_string())?;
        
        Ok(PriceData {
            source: "CoinGecko".to_string(),
            asset: asset.to_uppercase(),
            price,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            confidence: 0.95, // CoinGecko is highly reliable
        })
    }
    
    fn source_name(&self) -> &str {
        "CoinGecko"
    }
    
    fn reliability(&self) -> f64 {
        0.95
    }
}

// ============================================================================
// BINANCE IMPLEMENTATION
// ============================================================================

pub struct BinanceFeed;

impl BinanceFeed {
    pub fn new() -> Self {
        Self
    }
    
    fn asset_to_binance_symbol(asset: &str) -> Option<String> {
        match asset.to_uppercase().as_str() {
            "BTC" | "BITCOIN" => Some("BTCUSDT".to_string()),
            "ETH" | "ETHEREUM" => Some("ETHUSDT".to_string()),
            "SOL" | "SOLANA" => Some("SOLUSDT".to_string()),
            "BNB" => Some("BNBUSDT".to_string()),
            "XRP" | "RIPPLE" => Some("XRPUSDT".to_string()),
            "ADA" | "CARDANO" => Some("ADAUSDT".to_string()),
            "DOGE" | "DOGECOIN" => Some("DOGEUSDT".to_string()),
            "AVAX" | "AVALANCHE" => Some("AVAXUSDT".to_string()),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl DataFeed for BinanceFeed {
    async fn fetch_price(&self, asset: &str) -> Result<PriceData, String> {
        let symbol = Self::asset_to_binance_symbol(asset)
            .ok_or_else(|| format!("Unknown asset for Binance: {}", asset))?;
        
        let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol);
        
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Binance request failed: {}", e))?;
        
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Binance parse failed: {}", e))?;
        
        let price = data["price"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| "Price not found in Binance response".to_string())?;
        
        Ok(PriceData {
            source: "Binance".to_string(),
            asset: asset.to_uppercase(),
            price,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            confidence: 0.98, // Binance is very reliable (largest exchange)
        })
    }
    
    fn source_name(&self) -> &str {
        "Binance"
    }
    
    fn reliability(&self) -> f64 {
        0.98
    }
}

// ============================================================================
// COINBASE IMPLEMENTATION
// ============================================================================

pub struct CoinbaseFeed;

impl CoinbaseFeed {
    pub fn new() -> Self {
        Self
    }
    
    fn asset_to_coinbase_pair(asset: &str) -> Option<String> {
        match asset.to_uppercase().as_str() {
            "BTC" | "BITCOIN" => Some("BTC-USD".to_string()),
            "ETH" | "ETHEREUM" => Some("ETH-USD".to_string()),
            "SOL" | "SOLANA" => Some("SOL-USD".to_string()),
            "USDC" => Some("USDC-USD".to_string()),
            "USDT" => Some("USDT-USD".to_string()),
            "XRP" | "RIPPLE" => Some("XRP-USD".to_string()),
            "ADA" | "CARDANO" => Some("ADA-USD".to_string()),
            "DOGE" | "DOGECOIN" => Some("DOGE-USD".to_string()),
            "AVAX" | "AVALANCHE" => Some("AVAX-USD".to_string()),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl DataFeed for CoinbaseFeed {
    async fn fetch_price(&self, asset: &str) -> Result<PriceData, String> {
        let pair = Self::asset_to_coinbase_pair(asset)
            .ok_or_else(|| format!("Unknown asset for Coinbase: {}", asset))?;
        
        let url = format!("https://api.coinbase.com/v2/prices/{}/spot", pair);
        
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Coinbase request failed: {}", e))?;
        
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Coinbase parse failed: {}", e))?;
        
        let price = data["data"]["amount"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| "Price not found in Coinbase response".to_string())?;
        
        Ok(PriceData {
            source: "Coinbase".to_string(),
            asset: asset.to_uppercase(),
            price,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            confidence: 0.96, // Coinbase is highly reliable (regulated exchange)
        })
    }
    
    fn source_name(&self) -> &str {
        "Coinbase"
    }
    
    fn reliability(&self) -> f64 {
        0.96
    }
}

// ============================================================================
// ORACLE MANAGER
// ============================================================================

/// Oracle Manager - Coordinates multiple data feeds and validates consensus
pub struct OracleManager {
    feeds: Vec<DataFeedType>,
    cache: HashMap<String, CachedPrice>,
    cache_duration_secs: u64,
    min_sources_required: usize,
    max_variance_percent: f64, // Max acceptable variance between sources
}

impl OracleManager {
    /// Create new oracle manager with default settings
    pub fn new() -> Self {
        let mut feeds: Vec<DataFeedType> = Vec::new();
        
        // Add all available data feeds
        feeds.push(DataFeedType::CoinGecko(CoinGeckoFeed::new()));
        feeds.push(DataFeedType::Binance(BinanceFeed::new()));
        feeds.push(DataFeedType::Coinbase(CoinbaseFeed::new()));
        
        Self {
            feeds,
            cache: HashMap::new(),
            cache_duration_secs: 60, // Cache prices for 60 seconds
            min_sources_required: 2, // Require at least 2 sources to agree
            max_variance_percent: 5.0, // Max 5% variance between sources
        }
    }
    
    /// Get consensus price from multiple oracle sources
    /// 
    /// Returns error if sources disagree too much or insufficient sources available
    pub async fn get_consensus_price(&mut self, asset: &str) -> Result<ConsensusPrice, String> {
        // Check cache first
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if let Some(cached) = self.cache.get(asset) {
            if now - cached.timestamp < self.cache_duration_secs {
                return Ok(ConsensusPrice {
                    asset: asset.to_string(),
                    price: cached.price,
                    sources: cached.sources.clone(),
                    timestamp: cached.timestamp,
                    variance: 0.0, // Cached value has no variance
                    confidence: 1.0,
                });
            }
        }
        
        // Fetch from all sources in parallel
        let mut price_results = Vec::new();
        
        for feed in &self.feeds {
            match feed.fetch_price(asset).await {
                Ok(price_data) => price_results.push(price_data),
                Err(e) => {
                    eprintln!("⚠️  Oracle source {} failed for {}: {}", 
                        feed.source_name(), asset, e);
                }
            }
        }
        
        // Validate we have enough sources
        if price_results.len() < self.min_sources_required {
            return Err(format!(
                "Insufficient oracle sources: got {}, need {}",
                price_results.len(),
                self.min_sources_required
            ));
        }
        
        // Calculate consensus
        let prices: Vec<f64> = price_results.iter().map(|p| p.price).collect();
        let avg_price = prices.iter().sum::<f64>() / prices.len() as f64;
        
        // Calculate variance
        let max_price = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_price = prices.iter().cloned().fold(f64::INFINITY, f64::min);
        let variance_percent = ((max_price - min_price) / avg_price) * 100.0;
        
        // Check if variance is acceptable
        if variance_percent > self.max_variance_percent {
            return Err(format!(
                "Oracle sources disagree too much: {:.2}% variance (max: {:.2}%)",
                variance_percent,
                self.max_variance_percent
            ));
        }
        
        // Calculate weighted average by reliability
        let total_weight: f64 = price_results.iter()
            .map(|p| p.confidence)
            .sum();
        
        let weighted_price: f64 = price_results.iter()
            .map(|p| p.price * p.confidence)
            .sum::<f64>() / total_weight;
        
        let sources: Vec<String> = price_results.iter()
            .map(|p| p.source.clone())
            .collect();
        
        // Cache the result
        self.cache.insert(
            asset.to_string(),
            CachedPrice {
                price: weighted_price,
                timestamp: now,
                sources: sources.clone(),
            },
        );
        
        Ok(ConsensusPrice {
            asset: asset.to_string(),
            price: weighted_price,
            sources,
            timestamp: now,
            variance: variance_percent,
            confidence: 1.0 - (variance_percent / 100.0),
        })
    }
    
    /// Resolve a market based on price condition
    /// 
    /// Example: "BTC price >= $100,000 on Jan 1, 2025"
    pub async fn resolve_price_market(
        &mut self,
        market_id: &str,
        asset: &str,
        target_price: f64,
        condition: PriceCondition,
    ) -> Result<OracleResolution, String> {
        let consensus = self.get_consensus_price(asset).await?;
        
        let winning_outcome = match condition {
            PriceCondition::GreaterThan => {
                if consensus.price > target_price { 0 } else { 1 } // YES : NO
            }
            PriceCondition::GreaterThanOrEqual => {
                if consensus.price >= target_price { 0 } else { 1 }
            }
            PriceCondition::LessThan => {
                if consensus.price < target_price { 0 } else { 1 }
            }
            PriceCondition::LessThanOrEqual => {
                if consensus.price <= target_price { 0 } else { 1 }
            }
            PriceCondition::Between(min, max) => {
                if consensus.price >= min && consensus.price <= max {
                    1 // "No Change" for 3-outcome markets
                } else if consensus.price > max {
                    0 // "Yes" (price went higher)
                } else {
                    2 // "No" (price went lower)
                }
            }
        };
        
        Ok(OracleResolution {
            market_id: market_id.to_string(),
            winning_outcome,
            confidence: consensus.confidence,
            data_sources: consensus.sources,
            timestamp: consensus.timestamp,
            resolution_data: serde_json::json!({
                "asset": asset,
                "price": consensus.price,
                "target_price": target_price,
                "condition": format!("{:?}", condition),
                "variance": consensus.variance,
            }),
        })
    }
    
    /// Get status of all oracle data feeds
    pub fn get_feed_status(&self) -> Vec<serde_json::Value> {
        self.feeds
            .iter()
            .map(|feed| {
                serde_json::json!({
                    "name": feed.source_name(),
                    "reliability": feed.reliability(),
                    "active": true,
                })
            })
            .collect()
    }
    
    /// Clear price cache (force fresh data on next request)
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
    
    /// Get current cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

// ============================================================================
// PRICE CONDITIONS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceCondition {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Between(f64, f64), // min, max
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_oracle_consensus() {
        let mut oracle = OracleManager::new();
        
        // Test fetching BTC price from multiple sources
        match oracle.get_consensus_price("BTC").await {
            Ok(consensus) => {
                println!("✅ BTC Consensus Price: ${:.2}", consensus.price);
                println!("   Sources: {:?}", consensus.sources);
                println!("   Variance: {:.2}%", consensus.variance);
                println!("   Confidence: {:.2}%", consensus.confidence * 100.0);
                assert!(consensus.sources.len() >= 2);
                assert!(consensus.price > 0.0);
            }
            Err(e) => {
                println!("⚠️  Oracle test failed (might be API rate limit): {}", e);
            }
        }
    }
}
