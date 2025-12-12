// Application state management

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono;
use crate::models::PredictionMarket;
use crate::market_resolve::{Ledger, cpmm::PendingEvent};
use crate::auth::{SupabaseConfig, User};
use crate::bridge::BridgeManager;

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
}

impl AppState {
    pub fn new() -> Self {
        println!("🚀 Initializing BlackBook Layer 2 Prediction Market...");
        
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
        };

        println!("✅ BlackBook Prediction Market Blockchain Initialized");
        println!("🔗 Network: Layer 1 Blockchain (L1)");
        println!("💎 Token: BlackBook (BB) - Stable at $0.01");
        println!("");

        // Load markets from RSS events
        if let Err(e) = state.load_events_from_rss() {
            eprintln!("⚠️  Warning: Failed to load RSS events: {}", e);
        }

        state
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
