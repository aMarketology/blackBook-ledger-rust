// Application state management

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono;
use crate::models::PredictionMarket;
use crate::market_resolve::{Ledger, cpmm::PendingEvent};
use crate::auth::{SupabaseConfig, User};
use crate::bridge::BridgeManager;
use crate::optimistic_ledger::OptimisticLedger;
use crate::l1_sync::L1SyncService;
use crate::l1_rpc_client::L1RpcClient;

pub type SharedState = Arc<Mutex<AppState>>;

pub struct AppState {
    pub ledger: Ledger,
    pub markets: HashMap<String, PredictionMarket>,
    pub nonces: HashMap<String, u64>,
    pub blockchain_activity: Vec<String>,
    pub supabase_config: SupabaseConfig,
    pub supabase_users: HashMap<String, User>,
    pub bridge_manager: BridgeManager,
    pub pending_events: Vec<PendingEvent>,
    
    // ===== HYBRID L1/L2 SETTLEMENT =====
    /// Optimistic ledger for L2 execution with L1 settlement
    pub optimistic_ledger: OptimisticLedger,
    /// L1 sync service for batch settlements
    pub l1_sync: L1SyncService,
    /// Whether to use optimistic (hybrid) mode
    pub use_optimistic_mode: bool,
}

impl AppState {
    pub fn new() -> Self {
        println!("🚀 Initializing BlackBook Layer 2 Prediction Market...");
        
        // Initialize optimistic ledger
        let mut optimistic_ledger = OptimisticLedger::new();
        
        // Initialize L1 sync service
        let l1_sync = L1SyncService::from_env();
        l1_sync.rpc_client.log_status();
        
        // Check if optimistic mode is enabled via environment
        let use_optimistic_mode = std::env::var("USE_OPTIMISTIC_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true); // Default to true for hybrid L1/L2
        
        if use_optimistic_mode {
            println!("🔄 Optimistic Mode: ENABLED (Hybrid L1/L2 Settlement)");
        } else {
            println!("⚡ Optimistic Mode: DISABLED (Direct execution)");
        }
        
        let mut state = Self {
            ledger: Ledger::new_full_node(),
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
            optimistic_ledger,
            l1_sync,
            use_optimistic_mode,
        };

        println!("✅ BlackBook Prediction Market Blockchain Initialized");
        println!("🔗 Network: Layer 1 Blockchain (L1)");
        println!("💎 Token: BlackBook (BB) - Stable at $0.01");
        println!("");
        
        // Initialize optimistic ledger with accounts from the main ledger
        state.sync_ledger_to_optimistic();

        // Try to load persisted state first
        if let Ok(()) = state.load_from_disk() {
            println!("✅ Loaded persisted state from disk");
        } else {
            println!("ℹ️  No persisted state found, starting fresh");
            // Load markets from RSS events only if no persisted state
            if let Err(e) = state.load_events_from_rss() {
                eprintln!("⚠️  Warning: Failed to load RSS events: {}", e);
            }
        }

        state
    }
    
    /// Sync accounts from main ledger to optimistic ledger
    pub fn sync_ledger_to_optimistic(&mut self) {
        for (name, address) in &self.ledger.accounts {
            let balance = self.ledger.get_balance(address);
            self.optimistic_ledger.init_account(name.clone(), address.clone(), balance);
        }
        println!("🔄 Synced {} accounts to optimistic ledger", self.ledger.accounts.len());
    }
    
    /// Get balance - uses optimistic ledger in hybrid mode
    pub fn get_balance_hybrid(&self, address_or_name: &str) -> f64 {
        if self.use_optimistic_mode {
            self.optimistic_ledger.get_available_balance(address_or_name)
        } else {
            self.ledger.get_balance(address_or_name)
        }
    }
    
    /// Get confirmed (L1) balance
    pub fn get_confirmed_balance(&self, address_or_name: &str) -> f64 {
        if self.use_optimistic_mode {
            self.optimistic_ledger.get_confirmed_balance(address_or_name)
        } else {
            self.ledger.get_balance(address_or_name)
        }
    }
    
    /// Get pending delta (L2 changes awaiting settlement)
    pub fn get_pending_delta(&self, address_or_name: &str) -> f64 {
        if self.use_optimistic_mode {
            self.optimistic_ledger.get_pending_delta(address_or_name)
        } else {
            0.0
        }
    }

    pub fn save_to_disk(&self) -> Result<(), String> {
        use std::fs;
        use serde_json;

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
        
        fs::write("data/state.json", json)
            .map_err(|e| format!("Failed to write state file: {}", e))?;
        
        println!("💾 State saved to disk");
        Ok(())
    }

    fn load_from_disk(&mut self) -> Result<(), String> {
        use std::fs;
        use serde_json;

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

    pub fn log_blockchain_activity(&mut self, emoji: &str, action: &str, details: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let entry = format!("[{}] {} {} | {}", timestamp, emoji, action, details);
        println!("{}", entry);
        self.blockchain_activity.push(entry);
        if self.blockchain_activity.len() > 1000 {
            self.blockchain_activity.remove(0);
        }
    }

    pub fn track_activity(
        &mut self,
        action_type: String,
        _from: Option<String>,
        _to: Option<String>,
        _account: Option<String>,
        _amount: Option<f64>,
        details: String,
    ) {
        let emoji = match action_type.as_str() {
            "transfer" => "💸",
            "bet" => "🎯",
            "market_created" => "📊",
            "market_resolved" => "✅",
            _ => "📝",
        };
        self.log_blockchain_activity(emoji, &action_type.to_uppercase(), &details);
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
