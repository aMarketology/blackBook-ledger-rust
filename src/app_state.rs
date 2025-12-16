// Application state management - Simplified

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use crate::models::PredictionMarket;
use crate::market_resolve::{Ledger as MarketLedger, cpmm::PendingEvent};
use crate::auth::{SupabaseConfig, User};
use crate::bridge::BridgeManager;
use crate::ledger::Ledger;
use crate::orderbook::OrderBookManager;
use crate::shares::SharesManager;

pub type SharedState = Arc<Mutex<AppState>>;

// ============================================================================
// ORACLE/ADMIN AUTHORIZATION SYSTEM
// ============================================================================

/// Oracle configuration for market resolution
#[derive(Debug, Clone)]
pub struct OracleConfig {
    /// Whitelisted oracle addresses that can resolve markets
    pub oracle_whitelist: HashSet<String>,
    /// Admin addresses with full control
    pub admin_addresses: HashSet<String>,
    /// Minimum confirmations required for multi-sig (0 = single signature)
    pub multi_sig_threshold: u8,
    /// Markets requiring multi-sig (by market_id)
    pub high_value_markets: HashSet<String>,
    /// High value threshold in BB (markets above this require multi-sig)
    pub high_value_threshold: f64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        let mut admin_addresses = HashSet::new();
        let mut oracle_whitelist = HashSet::new();
        
        // Default admin is HOUSE account
        admin_addresses.insert("HOUSE".to_string());
        admin_addresses.insert("ADMIN".to_string());
        
        // Add environment-configured admin if present
        if let Ok(admin) = std::env::var("BLACKBOOK_ADMIN_ADDRESS") {
            admin_addresses.insert(admin);
        }
        
        // Default oracle is also HOUSE (can be expanded)
        oracle_whitelist.insert("HOUSE".to_string());
        oracle_whitelist.insert("ORACLE".to_string());
        
        // Add environment-configured oracle if present
        if let Ok(oracle) = std::env::var("BLACKBOOK_ORACLE_ADDRESS") {
            oracle_whitelist.insert(oracle);
        }
        
        Self {
            oracle_whitelist,
            admin_addresses,
            multi_sig_threshold: 0, // Single sig by default
            high_value_markets: HashSet::new(),
            high_value_threshold: 100_000.0, // 100k BB = high value
        }
    }
}

impl OracleConfig {
    /// Check if address is an admin
    pub fn is_admin(&self, address: &str) -> bool {
        self.admin_addresses.contains(address) || 
        self.admin_addresses.contains(&address.to_uppercase())
    }
    
    /// Check if address is a whitelisted oracle
    pub fn is_oracle(&self, address: &str) -> bool {
        self.oracle_whitelist.contains(address) || 
        self.oracle_whitelist.contains(&address.to_uppercase())
    }
    
    /// Check if address can resolve a specific market
    pub fn can_resolve(&self, address: &str, market_id: &str, market_volume: f64) -> bool {
        // Admins can always resolve
        if self.is_admin(address) {
            return true;
        }
        
        // Oracles can resolve non-high-value markets
        if self.is_oracle(address) {
            let is_high_value = self.high_value_markets.contains(market_id) || 
                               market_volume >= self.high_value_threshold;
            
            // For high value, require multi-sig (not implemented yet - return false)
            if is_high_value && self.multi_sig_threshold > 1 {
                return false;
            }
            
            return true;
        }
        
        false
    }
    
    /// Add an oracle to the whitelist
    pub fn add_oracle(&mut self, address: String) {
        self.oracle_whitelist.insert(address);
    }
    
    /// Remove an oracle from the whitelist
    pub fn remove_oracle(&mut self, address: &str) {
        self.oracle_whitelist.remove(address);
    }
    
    /// Mark a market as high value (requires multi-sig)
    pub fn mark_high_value(&mut self, market_id: String) {
        self.high_value_markets.insert(market_id);
    }
}

// ============================================================================
// MARKET RESOLUTION TRACKING
// ============================================================================

/// Tracks market resolution state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketResolution {
    pub market_id: String,
    pub winning_outcome: usize,
    pub winning_outcome_name: String,
    pub resolved_by: String,
    pub resolved_at: u64,
    pub total_payout: f64,
    pub num_winners: usize,
    pub l1_settlement_hash: Option<String>,
    pub l1_settlement_status: String, // "pending", "confirmed", "failed"
}

pub struct AppState {
    /// Core market ledger (for CPMM/pool logic)
    pub market_ledger: MarketLedger,
    /// New unified ledger for L2 tracking
    pub ledger: Ledger,
    /// Active prediction markets
    pub markets: HashMap<String, PredictionMarket>,
    /// Nonces for replay protection
    pub nonces: HashMap<String, u64>,
    /// Activity log
    pub blockchain_activity: Vec<String>,
    /// Supabase config (optional)
    pub supabase_config: SupabaseConfig,
    pub supabase_users: HashMap<String, User>,
    /// Bridge manager
    pub bridge_manager: BridgeManager,
    /// Pending market events
    pub pending_events: Vec<PendingEvent>,
    /// CLOB Order Book Manager (hybrid with CPMM fallback)
    pub orderbook: OrderBookManager,
    /// Outcome Shares Manager
    pub shares: SharesManager,
    /// Oracle/Admin configuration
    pub oracle_config: OracleConfig,
    /// Market resolution history
    pub resolutions: HashMap<String, MarketResolution>,
}

impl AppState {
    pub fn new() -> Self {
        println!("🚀 Initializing BlackBook Layer 2 Prediction Market...");
        
        let oracle_config = OracleConfig::default();
        println!("🔐 Oracle config: {} admins, {} oracles", 
            oracle_config.admin_addresses.len(),
            oracle_config.oracle_whitelist.len()
        );
        
        let mut state = Self {
            market_ledger: MarketLedger::new_full_node(),
            ledger: Ledger::new(),
            markets: HashMap::new(),
            nonces: HashMap::new(),
            blockchain_activity: Vec::new(),
            supabase_config: SupabaseConfig {
                url: std::env::var("SUPABASE_URL").unwrap_or_default(),
                anon_key: std::env::var("SUPABASE_ANON_KEY").unwrap_or_default(),
            },
            supabase_users: HashMap::new(),
            bridge_manager: BridgeManager::new(),
            pending_events: Vec::new(),
            orderbook: OrderBookManager::new(),
            shares: SharesManager::new(),
            oracle_config,
            resolutions: HashMap::new(),
        };

        println!("✅ BlackBook Prediction Market Initialized");
        println!("🔗 Network: Layer 2 (L1 sync: {})", if state.ledger.mock_mode { "mock" } else { "live" });
        println!("💎 Token: BlackBook (BB)");
        println!("📊 CLOB: Hybrid mode (CPMM fallback for illiquid markets)");
        println!("");

        // Try to load persisted state
        if let Ok(()) = state.load_from_disk() {
            println!("✅ Loaded persisted state from disk");
        } else {
            println!("ℹ️  No persisted state found, loading RSS events");
            if let Err(e) = state.load_events_from_rss() {
                eprintln!("⚠️  Warning: Failed to load RSS events: {}", e);
            }
        }

        state
    }
    
    /// Get balance (from unified ledger)
    pub fn get_balance(&self, id: &str) -> f64 {
        self.ledger.balance(id)
    }

    pub fn save_to_disk(&self) -> Result<(), String> {
        use std::fs;

        #[derive(serde::Serialize)]
        struct PersistedState {
            markets: HashMap<String, PredictionMarket>,
            nonces: HashMap<String, u64>,
        }

        let state = PersistedState {
            markets: self.markets.clone(),
            nonces: self.nonces.clone(),
        };

        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;
        
        fs::create_dir_all("data").ok();
        fs::write("data/state.json", json)
            .map_err(|e| format!("Failed to write state file: {}", e))?;
        
        println!("💾 State saved to disk");
        Ok(())
    }

    fn load_from_disk(&mut self) -> Result<(), String> {
        use std::fs;

        #[derive(serde::Deserialize)]
        struct PersistedState {
            markets: HashMap<String, PredictionMarket>,
            nonces: HashMap<String, u64>,
        }

        let json = fs::read_to_string("data/state.json")
            .map_err(|_| "No state file found")?;
        
        let state: PersistedState = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to deserialize state: {}", e))?;
        
        self.markets = state.markets;
        self.nonces = state.nonces;
        
        Ok(())
    }

    pub fn log_activity(&mut self, emoji: &str, action: &str, details: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let entry = format!("[{}] {} {} | {}", timestamp, emoji, action, details);
        println!("{}", entry);
        self.blockchain_activity.push(entry);
        if self.blockchain_activity.len() > 1000 {
            self.blockchain_activity.remove(0);
        }
    }

    fn load_events_from_rss(&mut self) -> Result<(), String> {
        use quick_xml::Reader;
        use quick_xml::events::Event;
        use std::fs;

        let events_dir = "rss/events";
        let entries = fs::read_dir(events_dir)
            .map_err(|e| format!("Failed to read events directory: {}", e))?;

        let mut loaded_count = 0;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) != Some("rss") {
                continue;
            }

            let xml_content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read RSS file {:?}: {}", path, e))?;

            let mut reader = Reader::from_str(&xml_content);
            reader.trim_text(true);

            let mut buf = Vec::new();
            let mut current_element = String::new();
            
            let mut title = String::new();
            let mut description = String::new();
            let mut guid = String::new();
            let mut category = String::new();
            let mut options: Vec<String> = Vec::new();
            let mut in_outcomes = false;

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(ref e)) => {
                        current_element = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        if current_element == "outcomes" {
                            in_outcomes = true;
                        }
                    }
                    Ok(Event::Text(e)) => {
                        let text = e.unescape().unwrap_or_default().trim().to_string();
                        if !text.is_empty() {
                            match current_element.as_str() {
                                "title" => title = text,
                                "description" => description = text,
                                "guid" => guid = text,
                                "category" => category = text,
                                "outcome" if in_outcomes => options.push(text),
                                _ => {}
                            }
                        }
                    }
                    Ok(Event::End(ref e)) => {
                        let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        if tag == "outcomes" {
                            in_outcomes = false;
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => return Err(format!("XML parse error: {}", e)),
                    _ => {}
                }
                buf.clear();
            }

            if !title.is_empty() && !options.is_empty() {
                let market_id = guid.split('/').last().unwrap_or(&guid).to_string();
                let market = PredictionMarket::new(
                    market_id.clone(),
                    title.clone(),
                    description,
                    category,
                    options,
                );
                
                self.markets.insert(market_id, market);
                loaded_count += 1;
                println!("📈 Activated market: {}", title);
            }
        }

        println!("✅ Loaded {} markets from rss/events/", loaded_count);
        Ok(())
    }
}
