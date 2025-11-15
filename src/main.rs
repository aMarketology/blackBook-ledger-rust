use axum::{
    extract::{Path, State},
    http::{StatusCode, HeaderMap},
    response::{Json, Html},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::{Arc, Mutex}, collections::HashMap};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

mod ledger;
mod hot_upgrades;
mod markets;
mod escrow;
mod auth;
use ledger::Ledger;
use hot_upgrades::{ProxyState, AuthorizedAccount, AuthorityLevel};
use auth::{UserRegistry, SupabaseConfig, User, SignupRequest, LoginRequest, AuthResponse};

// Prediction market struct - now tracks bettors for leaderboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionMarket {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String, // tech, sports, crypto, politics, business
    pub options: Vec<String>,
    pub is_resolved: bool,
    pub winning_option: Option<usize>,
    pub escrow_address: String,
    pub created_at: u64,
    
    // NEW: Tracking for leaderboard
    pub total_volume: f64,           // Total amount bet
    pub unique_bettors: Vec<String>, // Track unique bettors
    pub bet_count: u64,              // Total number of bets
    pub on_leaderboard: bool,        // Promoted when 10+ bettors
}

impl PredictionMarket {
    pub fn new(
        id: String,
        title: String,
        description: String,
        category: String,
        options: Vec<String>,
    ) -> Self {
        Self {
            id,
            title,
            description,
            category,
            options,
            is_resolved: false,
            winning_option: None,
            escrow_address: format!("MARKET_{}", Uuid::new_v4().simple()),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_volume: 0.0,
            unique_bettors: Vec::new(),
            bet_count: 0,
            on_leaderboard: false,
        }
    }
    
    /// Record a bet and check if should be promoted to leaderboard
    pub fn record_bet(&mut self, bettor: &str, amount: f64) {
        self.bet_count += 1;
        self.total_volume += amount;
        
        // Add unique bettor if new
        if !self.unique_bettors.contains(&bettor.to_string()) {
            self.unique_bettors.push(bettor.to_string());
        }
        
        // Promote to leaderboard when 10+ unique bettors
        if self.unique_bettors.len() >= 10 && !self.on_leaderboard {
            self.on_leaderboard = true;
        }
    }
}

// Live Crypto Price Bet - for 1-min and 15-min betting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePriceBet {
    pub id: String,
    pub bettor: String,
    pub asset: String,           // "BTC", "SOL"
    pub direction: String,        // "HIGHER" or "LOWER"
    pub entry_price: f64,
    pub bet_amount: f64,
    pub timeframe_seconds: u64,   // 60 or 900
    pub created_at: u64,
    pub expires_at: u64,
    pub status: String,           // "ACTIVE", "WON", "LOST"
    pub final_price: Option<f64>,
}

impl LivePriceBet {
    pub fn new(bettor: String, asset: String, direction: String, entry_price: f64, bet_amount: f64, timeframe_seconds: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id: format!("bet_{}", Uuid::new_v4().simple()),
            bettor,
            asset,
            direction,
            entry_price,
            bet_amount,
            timeframe_seconds,
            created_at: now,
            expires_at: now + timeframe_seconds,
            status: "ACTIVE".to_string(),
            final_price: None,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at
    }
}

// Transaction Activity Tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketActivity {
    pub id: String,
    pub activity_type: String,  // "market_created", "bet_placed", "market_resolved", "ai_event_added"
    pub market_id: Option<String>,
    pub market_title: Option<String>,
    pub actor: Option<String>,  // Who performed the action
    pub amount: Option<f64>,
    pub details: String,
    pub timestamp: u64,
}

// Application state - simple prediction market storage
#[derive(Debug)]

pub struct AppState {
    pub ledger: Ledger,
    pub markets: HashMap<String, PredictionMarket>,
    pub live_bets: Vec<LivePriceBet>,  // Store all live bets for history
    // pub proxy_state: ProxyState,       // Hot upgrade system - TODO: fix integration
    pub ai_events: Vec<AiEvent>,       // AI-generated events for RSS feed
    pub market_activities: Vec<MarketActivity>,  // Track all prediction market activities
}

impl AppState {
    pub fn new() -> Self {
        // TODO: Initialize authorized accounts for hot upgrades later
        // let authorized_accounts = vec![...];
        
        let mut state = Self {
            ledger: Ledger::new_full_node(),
            markets: HashMap::new(),
            live_bets: Vec::new(),
            // proxy_state: ProxyState::new(authorized_accounts),
            ai_events: Vec::new(),
            market_activities: Vec::new(),
        };

        // The ledger now has 8 real accounts with L1_ wallet addresses
        // These are dynamically generated UUIDs in format: L1_<32 HEX UPPERCASE>
        // All accounts already initialized with 1000 BB tokens on first run
        
        // Display the real blockchain accounts
        println!("✅ BlackBook Prediction Market Blockchain Initialized");
        println!("📊 Real Blockchain Accounts (L1 Wallets):");
        
        let account_names = vec!["ALICE", "BOB", "CHARLIE", "DIANA", "ETHAN", "FIONA", "GEORGE", "HANNAH", "HOUSE"];
        
        for name in &account_names {
            let address = state.ledger.accounts.get(*name).map(|a| a.clone()).unwrap_or_default();
            let balance = state.ledger.get_balance(name);
            println!("   👤 {} | Address: {} | Balance: {} BB", name, address, balance);
            
            // Track account initialization
            state.track_activity(
                "account_initialized".to_string(),
                None,
                None,
                Some(name.to_string()),
                Some(balance),
                format!("Admin account {} initialized with {} BB at address {}", name, balance, address),
            );
        }

        println!("💰 Total Circulating Supply: {} BB", 
            account_names.iter().map(|n| state.ledger.get_balance(n)).sum::<f64>()
        );
        println!("🔗 Network: Layer 1 Blockchain (L1)");
        println!("💎 Token: BlackBook (BB) - Stable at $0.01");
        println!("");

        // Create sample markets
        state.create_sample_markets();

        state
    }
    
    /// Track prediction market activity
    pub fn track_activity(
        &mut self,
        activity_type: String,
        market_id: Option<String>,
        market_title: Option<String>,
        actor: Option<String>,
        amount: Option<f64>,
        details: String,
    ) {
        let activity = MarketActivity {
            id: Uuid::new_v4().simple().to_string(),
            activity_type,
            market_id,
            market_title,
            actor,
            amount,
            details,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        self.market_activities.push(activity);
    }
    
    /// Log blockchain activity to terminal in real-time
    pub fn log_blockchain_activity(&self, emoji: &str, action_type: &str, details: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        println!("[{}] {} {} | {}", timestamp, emoji, action_type, details);
    }
    
    fn create_sample_markets(&mut self) {
        // Sample Markets
        let events = vec![
            // Original 3
            ("tech_ai_breakthrough_2025", "Major AI Breakthrough in 2025", "Will there be a major AI breakthrough (AGI, solved alignment, etc.) announced by a major tech company in 2025?", "tech"),
            ("business_recession_2025", "US Recession in 2025", "Will the United States officially enter a recession in 2025?", "business"),
            ("crypto_bitcoin_100k", "Bitcoin reaches $100K in 2025", "Will Bitcoin (BTC) reach $100,000 USD at any point during 2025?", "crypto"),
            
            // Sports Events (50 total)
            ("sports_australian_open_2026", "Australian Open 2026 Tennis", "Will Novak Djokovic win the Australian Open 2026?", "sports"),
            ("sports_dakar_rally_2026", "Dakar Rally 2026", "Will a driver from South America win the 2026 Dakar Rally?", "sports"),
            ("sports_six_nations_2026", "Six Nations Rugby 2026", "Will France win the 2026 Six Nations Championship?", "sports"),
            ("sports_milano_cortina_2026", "Winter Olympics Milano Cortina 2026", "Will Italy win more than 10 medals at Milano Cortina 2026?", "sports"),
            ("sports_daytona_500_2026", "Daytona 500 2026", "Will a NASCAR rookie finish in top 3 at Daytona 500 2026?", "sports"),
            ("sports_masters_2026", "The Masters Golf 2026", "Will an international player win The Masters 2026?", "sports"),
            ("sports_boston_marathon_2026", "Boston Marathon 2026", "Will a world record be broken at Boston Marathon 2026?", "sports"),
            ("sports_kentucky_derby_2026", "Kentucky Derby 2026", "Will a female jockey win the Kentucky Derby 2026?", "sports"),
            ("sports_french_open_2026", "French Open 2026", "Will Serena Williams's record be broken at French Open 2026?", "sports"),
            ("sports_monaco_gp_2026", "F1 Monaco Grand Prix 2026", "Will the 2026 Monaco GP be won by a driver 25 or younger?", "sports"),
            ("sports_us_open_golf_2026", "US Open Golf 2026", "Will an American golfer win the 2026 US Open?", "sports"),
            ("sports_wimbledon_2026", "Wimbledon Tennis 2026", "Will a top-5 seed win the Wimbledon 2026 singles title?", "sports"),
            ("sports_tour_france_femmes_2026", "Tour de France Femmes 2026", "Will a European cyclist win Tour de France Femmes 2026?", "sports"),
            ("sports_us_open_tennis_2026", "US Open Tennis 2026", "Will a player ranked outside top 10 win US Open 2026?", "sports"),
            ("sports_ryder_cup_2027", "Ryder Cup 2027", "Will Europe win the 2027 Ryder Cup?", "sports"),
            ("sports_pdc_world_darts_2026", "PDC World Darts 2025/2026", "Will a player from outside UK/Europe win PDC World Championship?", "sports"),
            ("sports_world_handball_2025", "World Handball Championship 2025", "Will France win the 2025 Men's Handball Championship?", "sports"),
            ("sports_figure_skating_2026", "European Figure Skating 2026", "Will Russia be allowed to compete at European Championships 2026?", "sports"),
            ("sports_formula_e_mexico_2026", "Formula E Mexico City 2026", "Will a new driver win Formula E Mexico City ePrix 2026?", "sports"),
            ("sports_ncaa_hockey_2026", "NCAA Hockey Championship 2026", "Will a new team win NCAA Men's Ice Hockey Championship?", "sports"),
            ("sports_snooker_2026", "World Snooker Championship 2026", "Will Ronnie O'Sullivan win World Championship 2026?", "sports"),
            ("sports_f1_spain_2026", "F1 Spanish Grand Prix 2026", "Will Max Verstappen lead the championship after Spain 2026?", "sports"),
            ("sports_iihf_2026", "IIHF World Championship 2026", "Will Canada win the 2026 Men's Ice Hockey World Championship?", "sports"),
            ("sports_cheltenham_2026", "Cheltenham Festival 2026", "Will a 50-1 longshot win the Gold Cup at Cheltenham 2026?", "sports"),
            ("sports_f1_britain_2026", "F1 British Grand Prix 2026", "Will a British driver finish on podium at Silverstone 2026?", "sports"),
            ("sports_world_aquatics_2026", "World Aquatics Championships 2026", "Will a swimming world record be broken in 2026?", "sports"),
            ("sports_open_golf_2026", "The Open Championship 2026", "Will an American golfer win The Open 2026?", "sports"),
            ("sports_commonwealth_2026", "Commonwealth Games 2026", "Will Australia win more medals than England?", "sports"),
            ("sports_world_athletics_2027", "World Athletics Championships 2027", "Will Eliud Kipchoge win a medal at World Championships 2027?", "sports"),
            ("sports_f1_canada_2026", "F1 Canadian Grand Prix 2026", "Will a rookie finish top 5 at Montreal 2026?", "sports"),
            ("sports_singapore_gp_2026", "F1 Singapore Grand Prix 2026", "Will Lewis Hamilton finish top 3 at Singapore 2026?", "sports"),
            ("sports_tokyo_marathon_2026", "Tokyo Marathon 2026", "Will a female runner win the Tokyo Marathon 2026?", "sports"),
            ("sports_london_marathon_2026", "London Marathon 2026", "Will the marathon record be broken at London 2026?", "sports"),
            ("sports_uefa_champions_2026", "UEFA Champions League Final 2026", "Will a Spanish team win Champions League 2026?", "sports"),
            ("sports_uefa_europa_2026", "UEFA Europa League Final 2026", "Will an Italian team win Europa League 2026?", "sports"),
            ("sports_pga_championship_2026", "PGA Championship 2026", "Will a European golfer win PGA Championship 2026?", "sports"),
            ("sports_indy_500_2026", "Indianapolis 500 2026", "Will a new driver win the Indy 500 in their debut year?", "sports"),
            ("sports_nfl_super_bowl_2026", "NFL Super Bowl LX 2026", "Will the underdog win Super Bowl LX?", "sports"),
            ("sports_nba_finals_2026", "NBA Finals 2026", "Will a team from Eastern Conference win NBA Finals 2026?", "sports"),
            ("sports_world_cup_2026", "FIFA World Cup 2026", "Will South America win World Cup 2026?", "sports"),
            
            // Arts & Culture
            ("culture_oscars_2026", "Academy Awards 2026", "Will a superhero movie win Best Picture at 2026 Oscars?", "politics"),
            ("culture_met_gala_2026", "Met Gala 2026", "Will the theme be science fiction related at Met Gala 2026?", "politics"),
            ("culture_cannes_2026", "Cannes Film Festival 2026", "Will an Asian filmmaker win Palme d'Or at Cannes 2026?", "politics"),
            ("culture_tony_awards_2026", "Tony Awards 2026", "Will a comedy musical win Best Play at Tony Awards 2026?", "politics"),
            ("culture_nobel_2026", "Nobel Prize Ceremonies 2026", "Will AI research win Nobel Prize in Physics 2026?", "politics"),
            ("culture_venice_2026", "Venice Biennale 2026", "Will contemporary digital art dominate Venice Biennale 2026?", "politics"),
            ("culture_sundance_2026", "Sundance Film Festival 2026", "Will an indie horror film premiere at Sundance 2026?", "politics"),
            ("culture_berlin_2026", "Berlin Film Festival 2026", "Will a documentary win the Golden Bear at Berlin 2026?", "politics"),
            ("culture_fashion_week_2027", "New York Fashion Week Fall 2027", "Will sustainable fashion dominate NYFW February 2026?", "politics"),
            
            // Business & Startup Conference Events
            ("business_sxsw_pitch_2026", "SXSW Pitch Competition 2026", "Will a health tech startup win the SXSW Pitch Competition in March 2026?", "business"),
            ("business_startup_grind_2026", "Startup Grind Global Conference 2026", "Will a European founder secure Series A funding at Startup Grind Global April 2026?", "business"),
            ("business_saastr_annual_2026", "SaaStr Annual Conference 2026", "Will a SaaS startup reach unicorn status post-SaaStr Annual May 2026?", "business"),
            ("business_techcrunch_disrupt_2026", "TechCrunch Disrupt 2026", "Will an AI startup win the TechCrunch Disrupt Battlefield in October 2026?", "business"),
            ("business_money20_20_2026", "Money20/20 USA Conference 2026", "Will a blockchain fintech startup secure major funding at Money20/20 October 2026?", "business"),
            ("business_south_summit_brasil_2026", "South Summit Brasil 2026", "Will a Latin American startup achieve Series B funding at South Summit Brasil March 2026?", "business"),
            ("business_eu_startups_summit_2026", "EU-Startups Summit 2026", "Will a European deep tech startup win the pitch competition at EU-Startups Summit May 2026?", "business"),
            ("business_superventure_2026", "SuperVenture Berlin 2026", "Will a German VC firm raise over €100M at SuperVenture June 2026?", "business"),
            ("business_south_summit_madrid_2026", "South Summit Madrid 2026", "Will a Spanish startup secure Series A at South Summit Madrid June 2026?", "business"),
            ("business_slush_2026", "Slush Conference Helsinki 2026", "Will a Nordic startup become a unicorn post-Slush November 2026?", "business"),
            ("business_web_summit_2026", "Web Summit Lisbon 2026", "Will Web Summit 2026 attract over 70,000 attendees in November?", "business"),
        ];

        for (id, title, description, category) in events {
            let market_id = id.to_string();
            self.markets.insert(market_id.clone(), PredictionMarket::new(
                market_id,
                title.to_string(),
                description.to_string(),
                category.to_string(),
                vec!["Yes".to_string(), "No".to_string()],
            ));
        }
    }
}

type SharedState = Arc<Mutex<AppState>>;

// Request structures
#[derive(Deserialize)]
struct DepositRequest {
    address: String,
    amount: f64,
    memo: String,
}

#[derive(Deserialize)]
struct TransferRequest {
    from: String,
    to: String,
    amount: f64,
    memo: String,
}

#[derive(Deserialize)]
struct CreateMarketRequest {
    #[serde(default)]
    id: Option<String>,  // Optional custom ID (for Polymarket markets)
    title: String,
    description: String,
    category: String,  // tech, sports, crypto, politics, business
    options: Vec<String>,
}

#[derive(Deserialize)]
struct BetRequest {
    account: String,
    market: String,
    outcome: usize,
    amount: f64,
}

// General Wager Request - for casino games, blackjack, peer-to-peer bets
#[derive(Debug, Deserialize)]
struct WagerRequest {
    from: String,              // Player placing the wager
    to: Option<String>,        // Optional opponent (None = house/casino)
    amount: f64,               // Wager amount in BB
    game_type: String,         // "blackjack", "poker", "roulette", "dice", "custom"
    game_id: Option<String>,   // Optional game session ID
    description: String,       // Description of the wager
}

// Wager Settlement Request - for resolving casino game outcomes
#[derive(Debug, Deserialize)]
struct SettleWagerRequest {
    transaction_id: String,    // Original wager transaction ID
    winner: String,            // Winner account name
    payout_amount: f64,        // Amount to pay out
    game_result: String,       // Game outcome description
}

// Response for leaderboard
#[derive(Serialize)]
struct LeaderboardEntry {
    market_id: String,
    title: String,
    category: String,
    total_volume: f64,
    unique_bettors: usize,
    bet_count: u64,
}

// Simple request for scraping a URL
#[derive(Deserialize)]
struct ScrapeRequest {
    url: String,
    title: String,
    category: String,
}

// Live Price Bet Request - for 1-min and 15-min crypto betting
#[derive(Debug, Deserialize)]
struct LivePriceBetRequest {
    bettor: String,              // Account name or address
    asset: String,               // "BTC" or "SOL"
    direction: String,           // "HIGHER" or "LOWER"
    amount: f64,                 // Amount to bet
    timeframe: String,           // "1min" or "15min"
}

// AI Event Creation Request - for automatic market creation from AI agents
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiEventRequest {
    source: AiEventSource,
    event: AiEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiEventSource {
    domain: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiEventData {
    title: String,
    description: String,
    category: String,
    options: Vec<String>,
    confidence: f64,         // AI confidence score (0.0 - 1.0)
    source_url: String,
}

// AI Event with metadata for RSS and ledger tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiEvent {
    pub id: String,
    pub source: AiEventSource,
    pub event: AiEventData,
    pub created_at: u64,
    pub added_to_ledger: bool,   // true if confidence > 0.555
    pub market_id: Option<String>, // market ID if added to ledger
}

#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(AppState::new()));

    let app = Router::new()
        // Root - API Info
        .route("/", get(api_info))
        
        // Ledger endpoints
        .route("/accounts", get(get_all_accounts))
        .route("/balance/:address", get(get_balance))
        .route("/deposit", post(deposit_funds))
        .route("/transfer", post(transfer_funds))
        .route("/transactions/:address", get(get_user_transactions))
        .route("/transactions", get(get_all_transactions))
        .route("/transactions/recent", get(get_recent_transactions))
        .route("/ledger/stats", get(get_ledger_stats))
        .route("/stats", get(get_stats))
        
        // Wallet Connection endpoints (for frontend)
        .route("/wallet/connect/:account_name", get(connect_wallet))
        .route("/wallet/test-accounts", get(get_test_accounts))
        .route("/wallet/account-info/:account_name", get(get_account_info))
        
        // Debug endpoint - list all balances
        .route("/debug/balances", get(debug_all_balances))
        
        // Admin operations
        .route("/admin/mint", post(admin_mint_tokens))
        .route("/admin/set-balance", post(admin_set_balance))
        
        // Market endpoints
        .route("/markets", get(get_markets))
        .route("/markets", post(create_market))
        .route("/markets/:id", get(get_market))
        .route("/leaderboard", get(get_leaderboard))
        .route("/leaderboard/:category", get(get_leaderboard_by_category))
        
        // Scraper endpoint - simple URL scraping
        .route("/scrape", post(scrape_and_create_market))
        
        // Live crypto price endpoints (real-time from CoinGecko - free public API)
        .route("/bitcoin-price", get(get_bitcoin_price))
        .route("/solana-price", get(get_solana_price))
        
        // Betting endpoints
        .route("/bet", post(place_bet))
        .route("/resolve/:market_id/:winning_option", post(resolve_market))
        
        // Casino & General Wager endpoints (for blackjack, poker, etc.)
        .route("/wager", post(place_wager))
        .route("/wager/settle", post(settle_wager))
        .route("/wager/history/:account", get(get_wager_history))
        
        // Live crypto price betting endpoints (1-min and 15-min)
        .route("/live-bet", post(place_live_price_bet))
        .route("/live-bets/active", get(get_active_live_bets))
        .route("/live-bets/history/:bettor", get(get_live_bet_history))
        .route("/live-bets/check/:bet_id", get(check_live_bet_status))
        
        // Hot upgrade system endpoints - TODO: fix integration
        // .route("/upgrades/propose", post(propose_upgrade))
        // .route("/upgrades/vote", post(vote_on_upgrade))
        // .route("/upgrades/execute", post(execute_upgrade))
        // .route("/upgrades/rollback", post(rollback_upgrade))
        // .route("/upgrades/status/:version", get(get_upgrade_status))
        // .route("/upgrades/history", get(get_upgrade_history))
        // .route("/upgrades/delegatecall", post(delegate_call))
        // .route("/upgrades/validate", post(validate_code))
        
        // AI Event Creation endpoints
        .route("/ai/events", post(create_ai_event))
        .route("/ai/events/recent", get(get_recent_ai_events))
        .route("/ai/events/feed.rss", get(get_ai_events_rss))
        
        // Market Activity endpoints
        .route("/activities", get(get_market_activities))
        
        // Health check
        .route("/health", get(health_check))
        
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Bind to 0.0.0.0 for deployment (accepts external connections)
    // Use environment variable PORT or default to 8080
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 BlackBook Prediction Market starting on http://0.0.0.0:{}", port);
    println!("");
    println!("📚 HTTP REST API Endpoints (Port {}):", port);
    println!("   🔌 Wallet Connection:");
    println!("      GET  /wallet/test-accounts - Get all 8 test accounts");
    println!("      GET  /wallet/connect/:account_name - Connect wallet (ALICE, BOB, etc.)");
    println!("      GET  /wallet/account-info/:account_name - Get detailed account info");
    println!("   📊 Account Management:");
    println!("      GET  /health - Health check");
    println!("      GET  /accounts - Get all accounts");
    println!("      GET  /balance/:address - Get account balance");
    println!("   POST /deposit - Deposit funds");
    println!("   POST /transfer - Transfer between accounts");
    println!("   POST /admin/mint - Mint tokens (admin)");
    println!("   POST /admin/set-balance - Set account balance (admin)");
    println!("   GET  /transactions/:address - Get user transactions");
    println!("   GET  /transactions - Get all transactions");
    println!("   GET  /ledger/stats - Get ledger statistics");
    println!("   GET  /markets - List all prediction markets");
    println!("   GET  /markets/:id - Get specific market");
    println!("   POST /bet - Place a bet on a market");
    println!("   POST /resolve/:market_id/:winning_option - Resolve market (admin)");
    println!("   GET  /leaderboard - Get featured markets (10+ bettors)");
    println!("   GET  /activities - Get all market activities");
    println!("   POST /ai/events - Create AI-generated prediction event");
    println!("   GET  /ai/events/feed.rss - RSS feed of AI events");
    println!("");
    println!("🔌 IPC Commands (Tauri Desktop App - 27 Total):");
    println!("   📊 Account Management (3):");
    println!("      • get_accounts - Get all blockchain accounts");
    println!("      • get_balance - Get account balance");
    println!("      • admin_deposit - Admin deposit tokens");
    println!("   💸 Transfers (1):");
    println!("      • transfer - Transfer BB tokens between accounts");
    println!("   📜 Audit Trail (6):");
    println!("      • get_all_transactions - Get transaction history");
    println!("      • get_account_transactions - Get account transactions");
    println!("      • get_recipes - Get all blockchain recipes");
    println!("      • get_account_recipes - Get recipes for account");
    println!("      • get_recipes_by_type - Filter recipes by type");
    println!("      • get_stats - Get ledger statistics");
    println!("   🎯 Betting Operations (3):");
    println!("      • place_bet - Place a bet (legacy)");
    println!("      • record_bet_win - Record bet win");
    println!("      • record_bet_loss - Record bet loss");
    println!("   🎲 Prediction Markets (9):");
    println!("      • create_market - Create prediction market");
    println!("      • get_markets - Get active markets");
    println!("      • place_market_bet - Place market bet");
    println!("      • get_open_markets - Get open markets");
    println!("      • get_market_stats - Get market statistics");
    println!("      • close_market - Close market to new bets");
    println!("      • resolve_market - Resolve market with escrow");
    println!("      • get_user_bets - Get user's bets");
    println!("      • get_all_markets - Get all markets");
    println!("   🌐 External Data (3):");
    println!("      • get_prices - Get BTC/SOL prices from CoinGecko");
    println!("      • get_polymarket_events - Get Polymarket events");
    println!("      • get_blackbook_events - Get BlackBook RSS events");
    println!("   🔐 Admin Tools (2):");
    println!("      • admin_mint_tokens - Mint tokens to account");
    println!("      • admin_set_balance - Set account balance");
    println!("");
    println!("💡 Protocols:");
    println!("   • HTTP REST API - For mobile nodes, web clients, external integrations");
    println!("   • IPC (Tauri) - For desktop app, direct in-memory communication");
    println!("   • Total API Surface: {} HTTP endpoints + 27 IPC commands", 
        "40+");
    println!("");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔗 LIVE BLOCKCHAIN ACTIVITY FEED");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📡 Monitoring all ledger actions in real-time...");
    println!("");
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Handler functions

// API info at root
async fn api_info() -> Json<Value> {
    Json(json!({
        "service": "BlackBook Prediction Market Blockchain",
        "version": "1.0.0",
        "network": "Layer 1 (L1)",
        "token": "BlackBook (BB)",
        "endpoints": {
            "health": "/health",
            "accounts": "/accounts",
            "markets": "/markets",
            "stats": "/stats",
            "leaderboard": "/leaderboard"
        },
        "documentation": "https://github.com/aMarketology/blackBook-ledger-rust"
    }))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "BlackBook Prediction Market",
        "version": "1.0.0",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

// Get all accounts for GOD MODE
// These are REAL blockchain wallets on the BlackBook ledger
// Each account has a dynamically generated L1_<UUID> wallet address
// All accounts initialized with 1000 BB tokens
async fn get_all_accounts(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    // Get all REAL blockchain accounts from the ledger
    // Each account has a dynamically generated L1_<UUID> wallet address
    // All accounts initialized with 1000 BB tokens
    let mut accounts: Vec<Value> = Vec::new();
    
    for (name, address) in &app_state.ledger.accounts {
        let balance = app_state.ledger.get_balance(name);
        accounts.push(json!({
            "name": name,
            "address": address,
            "balance": balance,
            "balance_symbol": "BB"
        }));
    }
    
    Json(json!({
        "success": true,
        "network": "Layer 1 Blockchain",
        "token": "BlackBook (BB)",
        "accounts": accounts,
        "total_accounts": accounts.len(),
        "total_supply": accounts.iter().map(|a| a["balance"].as_f64().unwrap_or(0.0)).sum::<f64>()
    }))
}

// Debug endpoint - list all balances in the HashMap
async fn debug_all_balances(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    println!("🔍 [DEBUG] Listing all balances in ledger:");
    println!("   Total entries: {}", app_state.ledger.balances.len());
    
    let mut balances_list = Vec::new();
    for (address, balance) in app_state.ledger.balances.iter() {
        println!("   {} = {} BB", address, balance);
        balances_list.push(json!({
            "address": address,
            "balance": balance
        }));
    }
    
    Json(json!({
        "success": true,
        "total_entries": app_state.ledger.balances.len(),
        "balances": balances_list
    }))
}

// Wallet Connection Endpoints for Frontend

/// Connect wallet - Returns account details for frontend wallet connection
async fn connect_wallet(
    State(state): State<SharedState>,
    Path(account_name): Path<String>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    // Normalize account name to uppercase
    let account_name_upper = account_name.to_uppercase();
    
    // Check if account exists
    if let Some(address) = app_state.ledger.accounts.get(&account_name_upper as &str) {
        let balance = app_state.ledger.get_balance(&account_name_upper);
        let transactions = app_state.ledger.get_account_transactions(&account_name_upper);
        
        // Get user's bets across all markets
        let mut user_bets = Vec::new();
        for (market_id, market) in &app_state.markets {
            if market.unique_bettors.contains(&account_name_upper) {
                user_bets.push(json!({
                    "market_id": market_id,
                    "market_title": market.title,
                    "category": market.category,
                }));
            }
        }
        
        // Log wallet connection to blockchain activity feed
        app_state.log_blockchain_activity(
            "🔌",
            "WALLET_CONNECTED",
            &format!("{} connected from frontend | Balance: {} BB (${:.2} USD) | Address: {}", 
                account_name_upper, balance, balance * 0.01, address)
        );
        
        Ok(Json(json!({
            "success": true,
            "connected": true,
            "account": {
                "name": account_name_upper,
                "address": address,
                "balance": balance,
                "balance_usd": balance * 0.01, // BB token = $0.01
                "token": "BB",
                "network": "BlackBook L1"
            },
            "stats": {
                "transaction_count": transactions.len(),
                "markets_participated": user_bets.len(),
            },
            "recent_transactions": transactions.iter().rev().take(5).collect::<Vec<_>>(),
            "active_bets": user_bets
        })))
    } else {
        Ok(Json(json!({
            "success": false,
            "connected": false,
            "error": format!("Account '{}' not found. Available accounts: ALICE, BOB, CHARLIE, DIANA, ETHAN, FIONA, GEORGE, HANNAH", account_name_upper)
        })))
    }
}

/// Get all test accounts for wallet selection (God Mode)
async fn get_test_accounts(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let test_accounts = vec!["ALICE", "BOB", "CHARLIE", "DIANA", "ETHAN", "FIONA", "GEORGE", "HANNAH"];
    
    let accounts: Vec<Value> = test_accounts.iter().map(|name| {
        let address = app_state.ledger.accounts.get(*name).map(|a| a.clone()).unwrap_or_default();
        let balance = app_state.ledger.get_balance(name);
        let transactions = app_state.ledger.get_account_transactions(name);
        
        json!({
            "name": name,
            "address": address,
            "balance": balance,
            "balance_usd": balance * 0.01,
            "token": "BB",
            "transaction_count": transactions.len(),
            "avatar": format!("https://api.dicebear.com/7.x/avataaars/svg?seed={}", name.to_lowercase())
        })
    }).collect();
    
    Json(json!({
        "success": true,
        "network": "BlackBook L1",
        "rpc_url": format!("http://0.0.0.0:{}", std::env::var("PORT").unwrap_or_else(|_| "8080".to_string())),
        "test_accounts": accounts,
        "total_accounts": accounts.len(),
        "note": "These are pre-funded test accounts for development. Connect any account to start trading."
    }))
}

/// Get detailed account info
async fn get_account_info(
    State(state): State<SharedState>,
    Path(account_name): Path<String>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    let account_name_upper = account_name.to_uppercase();
    
    if let Some(address) = app_state.ledger.accounts.get(&account_name_upper as &str) {
        let balance = app_state.ledger.get_balance(&account_name_upper);
        let transactions = app_state.ledger.get_all_transactions()
            .into_iter()
            .filter(|tx| tx.from == account_name_upper || tx.to == account_name_upper)
            .collect::<Vec<_>>();
        
        // Calculate stats
        let total_sent = transactions.iter()
            .filter(|tx| tx.from == account_name_upper)
            .map(|tx| tx.amount)
            .sum::<f64>();
        
        let total_received = transactions.iter()
            .filter(|tx| tx.to == account_name_upper)
            .map(|tx| tx.amount)
            .sum::<f64>();
        
        let bets_count = transactions.iter()
            .filter(|tx| tx.tx_type == "bet" && tx.from == account_name_upper)
            .count();
        
        // Log account info request to blockchain activity feed
        app_state.log_blockchain_activity(
            "📊",
            "ACCOUNT_INFO_VIEWED",
            &format!("{} | Transactions: {} | Bets: {} | Balance: {} BB", 
                account_name_upper, transactions.len(), bets_count, balance)
        );
        
        Ok(Json(json!({
            "success": true,
            "account": {
                "name": account_name_upper,
                "address": address,
                "balance": balance,
                "balance_usd": balance * 0.01,
                "token": "BB",
                "network": "BlackBook L1"
            },
            "statistics": {
                "total_transactions": transactions.len(),
                "total_sent": total_sent,
                "total_received": total_received,
                "bets_placed": bets_count,
                "net_flow": total_received - total_sent
            },
            "recent_transactions": transactions.iter().rev().take(10).collect::<Vec<_>>()
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_balance(
    State(state): State<SharedState>,
    Path(address): Path<String>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let balance = app_state.ledger.get_balance(&address);
    
    Json(json!({
        "address": address,
        "balance": balance
    }))
}

async fn deposit_funds(
    State(state): State<SharedState>,
    Json(payload): Json<DepositRequest>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    let tx_id = app_state.ledger.deposit(&payload.address, payload.amount, &payload.memo);
    let new_balance = app_state.ledger.get_balance(&payload.address);
    
    // Log to terminal
    app_state.log_blockchain_activity(
        "💰",
        "DEPOSIT",
        &format!("Account: {} | Amount: {} BB | New Balance: {} BB | Memo: {}", 
            payload.address, payload.amount, new_balance, payload.memo)
    );
    
    Ok(Json(json!({
        "success": true,
        "transaction_id": tx_id,
        "new_balance": new_balance
    })))
}

async fn transfer_funds(
    State(state): State<SharedState>,
    Json(payload): Json<TransferRequest>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.ledger.transfer(&payload.from, &payload.to, payload.amount) {
        Ok(tx_id) => {
            let from_balance = app_state.ledger.get_balance(&payload.from);
            let to_balance = app_state.ledger.get_balance(&payload.to);
            
            // Log to terminal
            app_state.log_blockchain_activity(
                "💸",
                "TRANSFER",
                &format!("{} → {} | Amount: {} BB | From Balance: {} BB | To Balance: {} BB", 
                    payload.from, payload.to, payload.amount, from_balance, to_balance)
            );
            
            Ok(Json(json!({
                "success": true,
                "transaction_id": tx_id,
                "from_balance": from_balance,
                "to_balance": to_balance
            })))
        },
        Err(error) => {
            // Log failed transfer
            app_state.log_blockchain_activity(
                "❌",
                "TRANSFER_FAILED",
                &format!("{} → {} | Amount: {} BB | Error: {}", 
                    payload.from, payload.to, payload.amount, error)
            );
            
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

async fn get_user_transactions(
    State(state): State<SharedState>,
    Path(address): Path<String>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let transactions = app_state.ledger.get_account_transactions(&address);
    
    Json(json!({
        "address": address,
        "transactions": transactions,
        "count": transactions.len()
    }))
}

async fn get_all_transactions(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let transactions = app_state.ledger.get_all_transactions();
    
    Json(json!({
        "transactions": transactions,
        "count": transactions.len()
    }))
}

// NEW: Get recent transactions with limit
#[derive(Deserialize)]
struct TransactionsQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn get_recent_transactions(
    State(state): State<SharedState>,
    axum::extract::Query(params): axum::extract::Query<TransactionsQuery>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let mut transactions = app_state.ledger.get_all_transactions();
    
    // Sort by timestamp descending (most recent first)
    transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    // Take only the requested limit
    let limited: Vec<_> = transactions.into_iter().take(params.limit).collect();
    
    Json(json!({
        "transactions": limited,
        "count": limited.len(),
        "limit": params.limit
    }))
}

// NEW: Get comprehensive stats for public display
async fn get_stats(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    // Get all transactions
    let transactions = app_state.ledger.get_all_transactions();
    
    // Get all markets
    let markets = app_state.markets.clone();
    let active_markets = markets.values().filter(|m| !m.is_resolved).count();
    
    // Calculate total volume from transactions
    let total_volume: f64 = transactions.iter().map(|tx| tx.amount).sum();
    
    // Count unique accounts that have transacted
    let mut unique_accounts = std::collections::HashSet::new();
    for tx in &transactions {
        unique_accounts.insert(tx.from.clone());
        unique_accounts.insert(tx.to.clone());
    }
    
    // Count bet transactions
    let total_bets = transactions.iter().filter(|tx| tx.tx_type == "bet").count();
    
    Json(json!({
        "total_transactions": transactions.len(),
        "total_volume": total_volume,
        "total_bets": total_bets,
        "active_markets": active_markets,
        "unique_accounts": unique_accounts.len(),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}

// Admin mint tokens endpoint
#[derive(Deserialize)]
struct MintTokensRequest {
    account: String,
    amount: f64,
}

async fn admin_mint_tokens(
    State(state): State<SharedState>,
    Json(payload): Json<MintTokensRequest>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.ledger.admin_mint_tokens(&payload.account, payload.amount) {
        Ok(tx_id) => {
            let new_balance = app_state.ledger.get_balance(&payload.account);
            
            // Log to terminal
            app_state.log_blockchain_activity(
                "🪙",
                "TOKENS_MINTED",
                &format!("Account: {} | Minted: {} BB | New Balance: {} BB | TX: {}", 
                    payload.account, payload.amount, new_balance, tx_id)
            );
            
            Ok(Json(json!({
                "success": true,
                "transaction_id": tx_id,
                "account": payload.account,
                "amount_minted": payload.amount,
                "new_balance": new_balance,
                "message": format!("Successfully minted {} BB to {}", payload.amount, payload.account)
            })))
        },
        Err(error) => {
            // Log failed mint
            app_state.log_blockchain_activity(
                "❌",
                "MINT_FAILED",
                &format!("Account: {} | Amount: {} BB | Error: {}", 
                    payload.account, payload.amount, error)
            );
            
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

// Admin set balance endpoint
#[derive(Deserialize)]
struct SetBalanceRequest {
    account: String,
    new_balance: f64,
}

async fn admin_set_balance(
    State(state): State<SharedState>,
    Json(payload): Json<SetBalanceRequest>
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    let old_balance = app_state.ledger.get_balance(&payload.account);
    
    match app_state.ledger.admin_set_balance(&payload.account, payload.new_balance) {
        Ok(tx_id) => {
            // Log to terminal
            app_state.log_blockchain_activity(
                "⚖️",
                "BALANCE_SET",
                &format!("Account: {} | Old: {} BB → New: {} BB | TX: {}", 
                    payload.account, old_balance, payload.new_balance, tx_id)
            );
            
            Ok(Json(json!({
                "success": true,
                "transaction_id": tx_id,
                "account": payload.account,
                "old_balance": old_balance,
                "new_balance": payload.new_balance,
                "message": format!("Successfully set {} balance to {} BB", payload.account, payload.new_balance)
            })))
        },
        Err(error) => {
            // Log failed set balance
            app_state.log_blockchain_activity(
                "❌",
                "SET_BALANCE_FAILED",
                &format!("Account: {} | New Balance: {} BB | Error: {}", 
                    payload.account, payload.new_balance, error)
            );
            
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

async fn get_ledger_stats(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let stats = app_state.ledger.get_stats();
    
    // Get real blockchain account info
    let accounts = vec!["alice", "bob", "charlie", "diana", "ethan", "fiona", "george", "hannah"];
    let account_balances: Vec<(String, f64)> = accounts
        .iter()
        .map(|name| (name.to_string(), app_state.ledger.get_balance(name)))
        .collect();
    
    let total_circulating = account_balances.iter().map(|(_, balance)| balance).sum::<f64>();
    
    Json(json!({
        "blockchain_stats": {
            "ledger_stats": stats,
            "total_accounts": 8,
            "total_circulating_supply": total_circulating,
            "token_symbol": "BB",
            "token_name": "BlackBook Token",
            "accounts": account_balances,
            "network_status": "RUNNING",
            "api_endpoint": "http://127.0.0.1:3000"
        }
    }))
}

async fn place_bet(
    State(state): State<SharedState>,
    Json(payload): Json<BetRequest>
) -> Result<Json<Value>, StatusCode> {
    println!("🔍 [PLACE_BET DEBUG] Received bet request:");
    println!("   └─ Market ID: {}", payload.market);
    println!("   └─ Account: {}", payload.account);
    println!("   └─ Amount: {}", payload.amount);
    println!("   └─ Outcome: {}", payload.outcome);
    
    // First, get the market info without borrowing mutably
    let (market_title, market_option, is_resolved, valid_option) = {
        let app_state = state.lock().unwrap();
        
        // DEBUG: List all available markets
        println!("🔍 [PLACE_BET DEBUG] Available markets in state:");
        for (market_id, market) in app_state.markets.iter() {
            println!("   ├─ Market ID: \"{}\" - Title: \"{}\"", market_id, market.title);
        }
        println!("   └─ Total markets: {}", app_state.markets.len());
        
        let market = match app_state.markets.get(&payload.market) {
            Some(m) => {
                println!("✅ [PLACE_BET DEBUG] Market found: {}", m.title);
                m
            },
            None => {
                println!("❌ [PLACE_BET DEBUG] Market '{}' NOT FOUND in state!", payload.market);
                println!("   └─ This is why we're returning 404");
                return Err(StatusCode::NOT_FOUND)
            }
        };
        
        let valid_option = payload.outcome < market.options.len();
        let market_option = if valid_option { 
            market.options[payload.outcome].clone() 
        } else { 
            String::new() 
        };
        
        (market.title.clone(), market_option, market.is_resolved, valid_option)
    };
    
    // Check if market is resolved
    if is_resolved {
        return Ok(Json(json!({
            "success": false,
            "message": "Market is already resolved"
        })));
    }
    
    // Check if option index is valid
    if !valid_option {
        return Ok(Json(json!({
            "success": false,
            "message": "Invalid outcome index"
        })));
    }
    
    // Now place the bet with mutable access
    let mut app_state = state.lock().unwrap();
    match app_state.ledger.place_bet(&payload.account, &payload.market, payload.amount) {
        Ok(tx_id) => {
            let user_balance = app_state.ledger.get_balance(&payload.account);
            
            // Track the bet and check for leaderboard promotion
            if let Some(market) = app_state.markets.get_mut(&payload.market) {
                market.record_bet(&payload.account, payload.amount);
                
                let on_leaderboard = market.on_leaderboard;
                let unique_bettors = market.unique_bettors.len();
                
                // Log to terminal
                app_state.log_blockchain_activity(
                    "🎲",
                    "BET_PLACED",
                    &format!("{} bet {} BB on \"{}\" → {} | Balance: {} BB | Bettors: {}", 
                        payload.account, payload.amount, market_title, market_option, user_balance, unique_bettors)
                );
                
                Ok(Json(json!({
                    "success": true,
                    "transaction_id": tx_id,
                    "bet": {
                        "market": market_title,
                        "outcome": market_option,
                        "amount": payload.amount
                    },
                    "new_balance": user_balance,
                    "market_progress": {
                        "unique_bettors": unique_bettors,
                        "bettors_needed_for_leaderboard": 10,
                        "on_leaderboard": on_leaderboard,
                        "promotion_message": if on_leaderboard && unique_bettors == 10 {
                            "🎉 Market promoted to leaderboard!".to_string()
                        } else if !on_leaderboard && unique_bettors >= 10 {
                            "".to_string()
                        } else {
                            format!("{} more bettors needed for leaderboard", 10 - unique_bettors)
                        }
                    }
                })))
            } else {
                Ok(Json(json!({
                    "success": false,
                    "message": "Market not found"
                })))
            }
        },
        Err(error) => {
            Ok(Json(json!({
                "success": false,
                "message": error
            })))
        }
    }
}

// ===== CASINO & GENERAL WAGER ENDPOINTS =====

/// Place a general wager (for casino games, blackjack, poker, peer-to-peer bets)
async fn place_wager(
    State(state): State<SharedState>,
    Json(payload): Json<WagerRequest>
) -> Result<Json<Value>, StatusCode> {
    println!("🎰 [PLACE_WAGER] Received wager request:");
    println!("   └─ From: {}", payload.from);
    println!("   └─ To: {:?}", payload.to);
    println!("   └─ Amount: {} BB", payload.amount);
    println!("   └─ Game: {}", payload.game_type);
    
    let mut app_state = state.lock().unwrap();
    
    // Validate amount
    if payload.amount <= 0.0 {
        return Ok(Json(json!({
            "success": false,
            "error": "Wager amount must be positive"
        })));
    }
    
    // Check if player has sufficient balance
    let player_balance = app_state.ledger.get_balance(&payload.from);
    if player_balance < payload.amount {
        return Ok(Json(json!({
            "success": false,
            "error": format!("Insufficient balance: {} has {} BB but needs {} BB", 
                payload.from, player_balance, payload.amount)
        })));
    }
    
    // Determine the recipient (opponent or house)
    let recipient = payload.to.clone().unwrap_or_else(|| "HOUSE".to_string());
    
    // Create wager transaction using transfer
    match app_state.ledger.transfer(&payload.from, &recipient, payload.amount) {
        Ok(tx_id) => {
            let new_balance = app_state.ledger.get_balance(&payload.from);
            let game_id = payload.game_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            
            // Log to blockchain activity feed
            app_state.log_blockchain_activity(
                "🎰",
                "WAGER_PLACED",
                &format!("{} wagered {} BB on {} | Game ID: {} | To: {} | Balance: {} BB", 
                    payload.from, payload.amount, payload.game_type, game_id, recipient, new_balance)
            );
            
            Ok(Json(json!({
                "success": true,
                "transaction_id": tx_id,
                "game_id": game_id,
                "wager": {
                    "from": payload.from,
                    "to": recipient,
                    "amount": payload.amount,
                    "game_type": payload.game_type,
                    "description": payload.description
                },
                "new_balance": new_balance,
                "message": format!("Wager placed: {} BB on {}", payload.amount, payload.game_type)
            })))
        },
        Err(error) => {
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

/// Settle a wager (resolve casino game outcome)
async fn settle_wager(
    State(state): State<SharedState>,
    Json(payload): Json<SettleWagerRequest>
) -> Result<Json<Value>, StatusCode> {
    println!("💰 [SETTLE_WAGER] Settling wager:");
    println!("   └─ Transaction ID: {}", payload.transaction_id);
    println!("   └─ Winner: {}", payload.winner);
    println!("   └─ Payout: {} BB", payload.payout_amount);
    
    let mut app_state = state.lock().unwrap();
    
    // Transfer winnings from HOUSE to winner
    match app_state.ledger.transfer("HOUSE", &payload.winner, payload.payout_amount) {
        Ok(tx_id) => {
            let winner_balance = app_state.ledger.get_balance(&payload.winner);
            
            // Log to blockchain activity feed
            app_state.log_blockchain_activity(
                "🏆",
                "WAGER_SETTLED",
                &format!("{} won {} BB | Result: {} | Balance: {} BB", 
                    payload.winner, payload.payout_amount, payload.game_result, winner_balance)
            );
            
            Ok(Json(json!({
                "success": true,
                "settlement_tx_id": tx_id,
                "original_tx_id": payload.transaction_id,
                "winner": payload.winner,
                "payout": payload.payout_amount,
                "new_balance": winner_balance,
                "game_result": payload.game_result,
                "message": format!("{} won {} BB!", payload.winner, payload.payout_amount)
            })))
        },
        Err(error) => {
            app_state.log_blockchain_activity(
                "❌",
                "WAGER_SETTLEMENT_FAILED",
                &format!("Failed to settle wager for {} | Error: {}", payload.winner, error)
            );
            
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

/// Get wager history for an account
async fn get_wager_history(
    State(state): State<SharedState>,
    Path(account): Path<String>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    // Get all transactions for this account
    let all_transactions = app_state.ledger.get_all_transactions();
    
    // Filter for wager-related transactions
    let wager_transactions: Vec<_> = all_transactions
        .into_iter()
        .filter(|tx| {
            (tx.from == account || tx.to == account) && 
            (tx.to == "HOUSE" || tx.from == "HOUSE" || tx.tx_type == "transfer")
        })
        .collect();
    
    Json(json!({
        "success": true,
        "account": account,
        "wager_count": wager_transactions.len(),
        "wagers": wager_transactions
    }))
}

async fn resolve_market(
    State(state): State<SharedState>,
    Path((market_id, winning_option)): Path<(String, usize)>
) -> Result<Json<Value>, StatusCode> {
    // First get market info and escrow balance without mutable borrow
    let (market_title, winning_option_text, escrow_balance) = {
        let app_state = state.lock().unwrap();
        
        // Get the market
        let market = match app_state.markets.get(&market_id) {
            Some(m) => m,
            None => return Err(StatusCode::NOT_FOUND)
        };
        
        // Check if already resolved
        if market.is_resolved {
            return Ok(Json(json!({
                "success": false,
                "error": "Market is already resolved"
            })));
        }
        
        // Check if winning option is valid
        if winning_option >= market.options.len() {
            return Ok(Json(json!({
                "success": false,
                "error": "Invalid winning option index"
            })));
        }
        
        // Get data before mutation
        let escrow_balance = app_state.ledger.get_balance(&market.escrow_address);
        let market_title = market.title.clone();
        let winning_option_text = market.options[winning_option].clone();
        
        (market_title, winning_option_text, escrow_balance)
    };
    
    // Now get mutable access to mark as resolved
    {
        let mut app_state = state.lock().unwrap();
        let market = app_state.markets.get_mut(&market_id).unwrap(); // We already checked it exists
        market.is_resolved = true;
        market.winning_option = Some(winning_option);
        
        // Log to terminal
        app_state.log_blockchain_activity(
            "🏆",
            "MARKET_RESOLVED",
            &format!("\"{}\" | Winner: {} | Escrow: {} BB", 
                market_title, winning_option_text, escrow_balance)
        );
    }
    
    // For demo purposes, we'll just resolve without actual payout logic
    // In a real system, you'd track individual bets and pay out winners
    
    Ok(Json(json!({
        "success": true,
        "message": format!("Market '{}' resolved with winning option: {}", market_title, winning_option_text),
        "winning_option": winning_option,
        "total_escrow": escrow_balance
    })))
}

/// Create a new prediction market - EASY market creation
async fn create_market(
    State(state): State<SharedState>,
    Json(payload): Json<CreateMarketRequest>
) -> Result<Json<Value>, StatusCode> {
    println!("🔍 [CREATE_MARKET DEBUG] Received market creation request:");
    println!("   └─ Custom ID: {:?}", payload.id);
    println!("   └─ Title: {}", payload.title);
    println!("   └─ Description: {}", payload.description);
    println!("   └─ Category: {}", payload.category);
    println!("   └─ Options: {:?}", payload.options);
    
    // Validate input
    if payload.title.is_empty() || payload.title.len() > 200 {
        println!("❌ [CREATE_MARKET DEBUG] Title validation failed");
        return Ok(Json(json!({
            "success": false,
            "error": "Title must be 1-200 characters"
        })));
    }
    
    if payload.description.is_empty() || payload.description.len() > 1000 {
        println!("❌ [CREATE_MARKET DEBUG] Description validation failed");
        return Ok(Json(json!({
            "success": false,
            "error": "Description must be 1-1000 characters"
        })));
    }
    
    if payload.options.len() < 2 || payload.options.len() > 5 {
        println!("❌ [CREATE_MARKET DEBUG] Options validation failed: {} options", payload.options.len());
        return Ok(Json(json!({
            "success": false,
            "error": "Must have 2-5 options"
        })));
    }
    
    // Generate unique market ID (or use custom ID if provided)
    let market_id = match payload.id {
        Some(custom_id) if !custom_id.is_empty() => {
            // Use custom ID (e.g., Polymarket ID)
            println!("✅ [CREATE_MARKET DEBUG] Using custom ID: {}", custom_id);
            custom_id
        },
        _ => {
            // Generate default ID
            let generated_id = format!(
                "market_{}_{}",
                payload.title.to_lowercase().replace(" ", "_").chars().take(30).collect::<String>(),
                Uuid::new_v4().simple()
            );
            println!("✅ [CREATE_MARKET DEBUG] Generated ID: {}", generated_id);
            generated_id
        }
    };
    
    let mut app_state = state.lock().unwrap();
    
    // Check if market already exists
    if app_state.markets.contains_key(&market_id) {
        println!("⚠️ [CREATE_MARKET DEBUG] Market '{}' already exists", market_id);
        return Ok(Json(json!({
            "success": false,
            "error": format!("Market with ID '{}' already exists", market_id)
        })));
    }
    
    // Create new market
    let new_market = PredictionMarket::new(
        market_id.clone(),
        payload.title.clone(),
        payload.description.clone(),
        payload.category.clone(),
        payload.options.clone(),
    );
    
    app_state.markets.insert(market_id.clone(), new_market);
    
    println!("✅ [CREATE_MARKET DEBUG] Market '{}' successfully inserted into state", market_id);
    println!("   └─ Total markets in state now: {}", app_state.markets.len());
    
    // Log to terminal
    app_state.log_blockchain_activity(
        "🎯",
        "MARKET_CREATED",
        &format!("\"{}\" | Category: {} | Options: {:?} | ID: {}", 
            payload.title, payload.category, payload.options, market_id)
    );
    
    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "title": payload.title,
        "category": payload.category,
        "message": "✅ Market created! Start betting to reach the leaderboard."
    })))
}

/// Get markets (optionally filtered by category)
async fn get_markets(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let markets: Vec<_> = app_state.markets
        .values()
        .map(|m| json!({
            "id": m.id,
            "title": m.title,
            "category": m.category,
            "description": m.description,
            "options": m.options,
            "total_volume": m.total_volume,
            "unique_bettors": m.unique_bettors.len(),
            "bet_count": m.bet_count,
            "on_leaderboard": m.on_leaderboard,
            "is_resolved": m.is_resolved,
        }))
        .collect();
    
    Json(json!({
        "markets": markets,
        "count": markets.len()
    }))
}

/// Get a specific market by ID
async fn get_market(
    State(state): State<SharedState>,
    Path(market_id): Path<String>
) -> Result<Json<Value>, StatusCode> {
    let app_state = state.lock().unwrap();
    
    match app_state.markets.get(&market_id) {
        Some(market) => {
            Ok(Json(json!({
                "success": true,
                "market": {
                    "id": market.id,
                    "title": market.title,
                    "category": market.category,
                    "description": market.description,
                    "options": market.options,
                    "total_volume": market.total_volume,
                    "unique_bettors": market.unique_bettors.len(),
                    "bet_count": market.bet_count,
                    "on_leaderboard": market.on_leaderboard,
                    "is_resolved": market.is_resolved,
                    "winning_option": market.winning_option,
                    "created_at": market.created_at,
                }
            })))
        }
        None => Err(StatusCode::NOT_FOUND)
    }
}

/// Get leaderboard - Markets with 10+ bettors, sorted by volume
async fn get_leaderboard(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let mut featured: Vec<LeaderboardEntry> = app_state.markets
        .values()
        .filter(|m| m.on_leaderboard)  // Only markets with 10+ bettors
        .map(|m| LeaderboardEntry {
            market_id: m.id.clone(),
            title: m.title.clone(),
            category: m.category.clone(),
            total_volume: m.total_volume,
            unique_bettors: m.unique_bettors.len(),
            bet_count: m.bet_count,
        })
        .collect();
    
    // Sort by volume (descending)
    featured.sort_by(|a, b| b.total_volume.partial_cmp(&a.total_volume).unwrap());
    
    Json(json!({
        "leaderboard": featured,
        "count": featured.len(),
        "threshold": "Markets must have 10+ unique bettors to appear here"
    }))
}

/// Get leaderboard by category
async fn get_leaderboard_by_category(
    State(state): State<SharedState>,
    Path(category): Path<String>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let mut featured: Vec<LeaderboardEntry> = app_state.markets
        .values()
        .filter(|m| m.on_leaderboard && m.category.to_lowercase() == category.to_lowercase())
        .map(|m| LeaderboardEntry {
            market_id: m.id.clone(),
            title: m.title.clone(),
            category: m.category.clone(),
            total_volume: m.total_volume,
            unique_bettors: m.unique_bettors.len(),
            bet_count: m.bet_count,
        })
        .collect();
    
    // Sort by volume (descending)
    featured.sort_by(|a, b| b.total_volume.partial_cmp(&a.total_volume).unwrap());
    
    Json(json!({
        "category": category,
        "leaderboard": featured,
        "count": featured.len(),
    }))
}

// ===== SIMPLE SCRAPER HANDLER =====

/// Create a prediction market from user input
async fn scrape_and_create_market(
    State(state): State<SharedState>,
    Json(payload): Json<ScrapeRequest>,
) -> Result<Json<Value>, StatusCode> {
    // Create a market from the provided data
    let market_id = format!(
        "market_{}_{}",
        payload.title.to_lowercase().replace(" ", "_").chars().take(30).collect::<String>(),
        Uuid::new_v4().simple()
    );

    let market = PredictionMarket::new(
        market_id.clone(),
        payload.title.clone(),
        format!("Custom market: {}", payload.url),
        payload.category.clone(),
        vec!["Yes".to_string(), "No".to_string()],
    );

    let mut app_state = state.lock().unwrap();
    app_state.markets.insert(market_id.clone(), market);

    println!("✅ Created market: {}", market_id);

    Ok(Json(json!({
        "success": true,
        "market_id": market_id,
        "title": payload.title,
        "category": payload.category,
        "message": "Market created! Users can now bet on this event."
    })))
}

/// Get real-time Bitcoin price from CoinGecko
async fn get_bitcoin_price() -> Json<Value> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd";
    
    match reqwest::Client::new().get(url).send().await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    if let Some(price) = data["bitcoin"]["usd"].as_f64() {
                        Json(json!({
                            "success": true,
                            "asset": "Bitcoin",
                            "symbol": "BTC",
                            "price": price,
                            "change_24h": 0.0
                        }))
                    } else {
                        // Fallback to simulated price
                        eprintln!("⚠️  CoinGecko parse failed, using fallback BTC price");
                        Json(json!({
                            "success": true,
                            "asset": "Bitcoin",
                            "symbol": "BTC",
                            "price": 112524.00,
                            "change_24h": 0.0
                        }))
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  CoinGecko API failed: {}, using fallback BTC price", e);
                    // Return fallback price
                    Json(json!({
                        "success": true,
                        "asset": "Bitcoin",
                        "symbol": "BTC",
                        "price": 112524.00,
                        "change_24h": 0.0
                    }))
                }
            }
        }
        Err(e) => {
            eprintln!("⚠️  CoinGecko network error: {}, using fallback BTC price", e);
            // Return fallback price
            Json(json!({
                "success": true,
                "asset": "Bitcoin",
                "symbol": "BTC",
                "price": 112524.00,
                "change_24h": 0.0
            }))
        }
    }
}

/// Get real-time Solana price from CoinGecko
async fn get_solana_price() -> Json<Value> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";
    
    match reqwest::Client::new().get(url).send().await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    if let Some(price) = data["solana"]["usd"].as_f64() {
                        Json(json!({
                            "success": true,
                            "asset": "Solana",
                            "symbol": "SOL",
                            "price": price
                        }))
                    } else {
                        Json(json!({
                            "success": false,
                            "error": "Failed to parse Solana price"
                        }))
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to parse Solana price response: {}", e);
                    Json(json!({
                        "success": false,
                        "error": format!("Parse error: {}", e)
                    }))
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to fetch Solana price: {}", e);
            Json(json!({
                "success": false,
                "error": format!("API error: {}", e)
            }))
        }
    }
}

/// Place a live price bet on BTC or SOL for 1-min or 15-min timeframe
async fn place_live_price_bet(
    State(state): State<SharedState>,
    Json(payload): Json<LivePriceBetRequest>
) -> Result<Json<Value>, StatusCode> {
    println!("🎲 [BACKEND] Received live bet request");
    println!("🎲 [BACKEND] Payload: {:?}", payload);
    
    // Validate input
    println!("🔍 [BACKEND] Validating asset: {}", payload.asset);
    if !["BTC", "SOL"].contains(&payload.asset.as_str()) {
        println!("❌ [BACKEND] Invalid asset: {}", payload.asset);
        return Ok(Json(json!({
            "success": false,
            "error": "Asset must be BTC or SOL"
        })));
    }
    
    println!("🔍 [BACKEND] Validating direction: {}", payload.direction);
    if !["HIGHER", "LOWER"].contains(&payload.direction.as_str()) {
        println!("❌ [BACKEND] Invalid direction: {}", payload.direction);
        return Ok(Json(json!({
            "success": false,
            "error": "Direction must be HIGHER or LOWER"
        })));
    }
    
    println!("🔍 [BACKEND] Validating timeframe: {}", payload.timeframe);
    if !["1min", "15min"].contains(&payload.timeframe.as_str()) {
        println!("❌ [BACKEND] Invalid timeframe: {}", payload.timeframe);
        return Ok(Json(json!({
            "success": false,
            "error": "Timeframe must be 1min or 15min"
        })));
    }
    
    println!("🔍 [BACKEND] Validating amount: {}", payload.amount);
    if payload.amount <= 0.0 {
        println!("❌ [BACKEND] Invalid amount: {}", payload.amount);
        return Ok(Json(json!({
            "success": false,
            "error": "Bet amount must be positive"
        })));
    }
    
    // Get current price
    println!("💰 [BACKEND] Fetching current {} price...", payload.asset);
    let current_price = if payload.asset == "BTC" {
        get_btc_price().await
    } else {
        get_sol_price().await
    };
    
    if current_price.is_none() {
        println!("❌ [BACKEND] Failed to get current price for {}", payload.asset);
        return Ok(Json(json!({
            "success": false,
            "error": "Failed to get current price"
        })));
    }
    
    let entry_price = current_price.unwrap();
    println!("✅ [BACKEND] Current {} price: ${}", payload.asset, entry_price);
    
    let timeframe_seconds = if payload.timeframe == "1min" { 60 } else { 900 };
    println!("⏱️  [BACKEND] Timeframe: {} seconds", timeframe_seconds);
    
    let mut app_state = state.lock().unwrap();
    
    // Deduct bet amount from account
    let from_balance = app_state.ledger.get_balance(&payload.bettor);
    println!("💵 [BACKEND] Account {} balance: {} BB", payload.bettor, from_balance);
    
    if from_balance < payload.amount {
        println!("❌ [BACKEND] Insufficient balance: {} has {} BB but needs {}", 
            payload.bettor, from_balance, payload.amount);
        return Ok(Json(json!({
            "success": false,
            "error": format!("Insufficient balance: {} has {} BB but needs {}", 
                payload.bettor, from_balance, payload.amount)
        })));
    }
    
    // Create live bet
    println!("🎰 [BACKEND] Creating live bet...");
    let live_bet = LivePriceBet::new(
        payload.bettor.clone(),
        payload.asset.clone(),
        payload.direction.clone(),
        entry_price,
        payload.amount,
        timeframe_seconds
    );
    
    let bet_id = live_bet.id.clone();
    let expires_at = live_bet.expires_at;
    println!("✅ [BACKEND] Live bet created with ID: {}", bet_id);
    println!("⏰ [BACKEND] Bet expires at: {}", expires_at);
    
    // Record the bet in ledger
    println!("📝 [BACKEND] Recording bet in ledger...");
    match app_state.ledger.place_bet(&payload.bettor, &format!("live_bet_{}", payload.asset.to_lowercase()), payload.amount) {
        Ok(tx_id) => {
            println!("✅ [BACKEND] Bet recorded in ledger with transaction ID: {}", tx_id);
            
            // Add to live bets
            app_state.live_bets.push(live_bet);
            println!("✅ [BACKEND] Added to active live bets list");
            
            let response = json!({
                "success": true,
                "bet_id": bet_id,
                "asset": payload.asset,
                "direction": payload.direction,
                "entry_price": entry_price,
                "amount": payload.amount,
                "timeframe": payload.timeframe,
                "expires_at": expires_at,
                "message": format!("Bet placed on {} {}: {} {} for {} seconds", 
                    payload.asset, payload.direction, payload.amount, "BB", timeframe_seconds)
            });
            
            println!("✅ [BACKEND] Sending success response: {:?}", response);
            Ok(Json(response))
        },
        Err(error) => {
            println!("❌ [BACKEND] Ledger error: {}", error);
            Ok(Json(json!({
                "success": false,
                "error": error
            })))
        }
    }
}

/// Get all active live bets
async fn get_active_live_bets(
    State(state): State<SharedState>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let active_bets: Vec<Value> = app_state.live_bets
        .iter()
        .filter(|b| b.status == "ACTIVE" && !b.is_expired())
        .map(|b| json!({
            "id": b.id,
            "bettor": b.bettor,
            "asset": b.asset,
            "direction": b.direction,
            "entry_price": b.entry_price,
            "amount": b.bet_amount,
            "timeframe_seconds": b.timeframe_seconds,
            "created_at": b.created_at,
            "expires_at": b.expires_at,
            "status": b.status
        }))
        .collect();
    
    Json(json!({
        "success": true,
        "active_bets": active_bets,
        "count": active_bets.len()
    }))
}

/// Get live bet history for a specific bettor
async fn get_live_bet_history(
    State(state): State<SharedState>,
    Path(bettor): Path<String>
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let history: Vec<Value> = app_state.live_bets
        .iter()
        .filter(|b| b.bettor == bettor)
        .map(|b| json!({
            "id": b.id,
            "asset": b.asset,
            "direction": b.direction,
            "entry_price": b.entry_price,
            "final_price": b.final_price,
            "amount": b.bet_amount,
            "timeframe_seconds": b.timeframe_seconds,
            "created_at": b.created_at,
            "expires_at": b.expires_at,
            "status": b.status
        }))
        .collect();
    
    Json(json!({
        "success": true,
        "bettor": bettor,
        "bets": history,
        "count": history.len()
    }))
}

/// Check status of a specific live bet
async fn check_live_bet_status(
    State(state): State<SharedState>,
    Path(bet_id): Path<String>
) -> Result<Json<Value>, StatusCode> {
    let app_state = state.lock().unwrap();
    
    if let Some(bet) = app_state.live_bets.iter().find(|b| b.id == bet_id) {
        Ok(Json(json!({
            "success": true,
            "id": bet.id,
            "bettor": bet.bettor,
            "asset": bet.asset,
            "direction": bet.direction,
            "entry_price": bet.entry_price,
            "final_price": bet.final_price,
            "amount": bet.bet_amount,
            "status": bet.status,
            "is_expired": bet.is_expired(),
            "created_at": bet.created_at,
            "expires_at": bet.expires_at
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Helper to get BTC price
async fn get_btc_price() -> Option<f64> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd";
    
    match reqwest::Client::new().get(url).send().await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => data["bitcoin"]["usd"].as_f64(),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}

/// Helper to get SOL price
async fn get_sol_price() -> Option<f64> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";
    
    match reqwest::Client::new().get(url).send().await {
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Ok(data) => data["solana"]["usd"].as_f64(),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}

/*
// Hot Upgrade Handler Functions - TODO: Fix integration with hot_upgrades module

#[derive(Serialize, Deserialize)]
struct ProposeUpgradeRequest {
    proposer: String,
    new_code_hash: String,
    bytecode: Vec<u8>,
    description: String,
}

async fn propose_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<ProposeUpgradeRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.proxy_state.propose_upgrade(
        &payload.proposer,
        payload.new_code_hash,
        payload.bytecode,
        payload.description,
    ) {
        Ok(upgrade_id) => Ok(Json(json!({
            "success": true,
            "upgrade_id": upgrade_id,
            "message": "Upgrade proposal created"
        }))),
        Err(e) => {
            eprintln!("Upgrade proposal failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct VoteUpgradeRequest {
    voter: String,
    upgrade_id: String,
    approve: bool,
    signature: String,
}

async fn vote_on_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<VoteUpgradeRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.proxy_state.vote_on_upgrade(
        &payload.upgrade_id,
        &payload.voter,
        payload.approve,
        &payload.signature,
    ) {
        Ok(()) => Ok(Json(json!({
            "success": true,
            "message": "Vote recorded"
        }))),
        Err(e) => {
            eprintln!("Vote failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ExecuteUpgradeRequest {
    executor: String,
    upgrade_id: String,
    signature: String,
}

async fn execute_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<ExecuteUpgradeRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.proxy_state.execute_upgrade(
        &payload.upgrade_id,
        &payload.executor,
        &payload.signature,
    ) {
        Ok(()) => Ok(Json(json!({
            "success": true,
            "message": "Upgrade executed successfully"
        }))),
        Err(e) => {
            eprintln!("Upgrade execution failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RollbackUpgradeRequest {
    authority: String,
    target_version: String,
    reason: String,
    emergency_signature: String,
}

async fn rollback_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<RollbackUpgradeRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.proxy_state.emergency_rollback(
        &payload.authority,
        &payload.target_version,
        &payload.reason,
        &payload.emergency_signature,
    ) {
        Ok(()) => Ok(Json(json!({
            "success": true,
            "message": "Emergency rollback completed"
        }))),
        Err(e) => {
            eprintln!("Rollback failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn get_upgrade_status(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(version): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let app_state = state.lock().unwrap();
    
    match app_state.proxy_state.get_upgrade_status(&version) {
        Some(status) => Ok(Json(json!(status))),
        None => Err(StatusCode::NOT_FOUND)
    }
}

async fn get_upgrade_history(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let history = app_state.proxy_state.get_upgrade_history();
    Json(json!({
        "upgrade_history": history,
        "current_version": app_state.proxy_state.current_version
    }))
}

#[derive(Serialize, Deserialize)]
struct DelegateCallRequest {
    caller: String,
    function_name: String,
    args: Vec<String>,
    signature: String,
}

async fn delegate_call(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<DelegateCallRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    match app_state.proxy_state.delegate_call(
        &payload.caller,
        &payload.function_name,
        payload.args,
        &payload.signature,
    ) {
        Ok(result) => Ok(Json(json!({
            "success": true,
            "result": result
        }))),
        Err(e) => {
            eprintln!("Delegate call failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ValidateCodeRequest {
    bytecode: Vec<u8>,
    validator: String,
}

async fn validate_code(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(payload): Json<ValidateCodeRequest>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    let hash = app_state.proxy_state.validate_code_hash(&payload.bytecode);
    
    Json(json!({
        "valid": true,
        "code_hash": hash,
        "validator": payload.validator,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}
*/

// ==================== AI EVENT HANDLERS ====================

/// POST /ai/events - Accept AI-generated events and add to RSS feed + ledger if confidence > 0.555
async fn create_ai_event(
    State(state): State<SharedState>,
    Json(payload): Json<AiEventRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut app_state = state.lock().unwrap();
    
    // Validate the event data
    if payload.event.title.is_empty() || payload.event.options.len() < 2 {
        return Ok(Json(json!({
            "success": false,
            "error": "Event must have a title and at least 2 options"
        })));
    }
    
    if payload.event.confidence < 0.0 || payload.event.confidence > 1.0 {
        return Ok(Json(json!({
            "success": false,
            "error": "Confidence must be between 0.0 and 1.0"
        })));
    }
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let event_id = format!("ai_event_{}", Uuid::new_v4().simple());
    let mut market_id: Option<String> = None;
    let mut added_to_ledger = false;
    
    // If confidence > 0.555, create a prediction market
    if payload.event.confidence > 0.555 {
        let market_id_str = format!("ai_market_{}", Uuid::new_v4().simple());
        
        let market = PredictionMarket::new(
            market_id_str.clone(),
            payload.event.title.clone(),
            payload.event.description.clone(),
            payload.event.category.clone(),
            payload.event.options.clone(),
        );
        
        app_state.markets.insert(market_id_str.clone(), market);
        market_id = Some(market_id_str.clone());
        added_to_ledger = true;
        
        // Track activity
        app_state.track_activity(
            "ai_event_added".to_string(),
            Some(market_id_str.clone()),
            Some(payload.event.title.clone()),
            Some(format!("AI Agent: {}", payload.source.domain)),
            None,
            format!("AI-generated market created with {:.1}% confidence from {}", 
                payload.event.confidence * 100.0, payload.source.domain),
        );
        
        println!("🤖 AI Event added to ledger: {} (confidence: {:.2})", 
            payload.event.title, payload.event.confidence);
    } else {
        println!("📋 AI Event added to RSS only: {} (confidence: {:.2} - below 0.555 threshold)", 
            payload.event.title, payload.event.confidence);
    }
    
    // Create the AI event
    let ai_event = AiEvent {
        id: event_id.clone(),
        source: payload.source.clone(),
        event: payload.event.clone(),
        created_at: now,
        added_to_ledger,
        market_id: market_id.clone(),
    };
    
    // Add to AI events list (for RSS feed)
    app_state.ai_events.push(ai_event);
    
    // Write to RSS file
    if let Err(e) = update_rss_feed(&app_state.ai_events) {
        eprintln!("⚠️  Failed to update RSS feed: {}", e);
    }
    
    Ok(Json(json!({
        "success": true,
        "event_id": event_id,
        "added_to_ledger": added_to_ledger,
        "market_id": market_id,
        "confidence": payload.event.confidence,
        "threshold": 0.555,
        "message": if added_to_ledger {
            "Event added to RSS feed and created as prediction market"
        } else {
            "Event added to RSS feed only (confidence below threshold)"
        }
    })))
}

/// GET /ai/events/recent - Get recent AI events
async fn get_recent_ai_events(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    // Get last 50 events
    let recent_events: Vec<&AiEvent> = app_state.ai_events
        .iter()
        .rev()
        .take(50)
        .collect();
    
    Json(json!({
        "success": true,
        "count": recent_events.len(),
        "events": recent_events
    }))
}

/// GET /ai/events/feed.rss - Get RSS feed of AI events
async fn get_ai_events_rss(
    State(state): State<SharedState>,
) -> Result<([(axum::http::HeaderName, &'static str); 1], String), StatusCode> {
    let app_state = state.lock().unwrap();
    
    // Read the RSS file
    match std::fs::read_to_string("src/event.rss") {
        Ok(content) => {
            Ok((
                [(axum::http::header::CONTENT_TYPE, "application/rss+xml")],
                content
            ))
        },
        Err(e) => {
            eprintln!("Failed to read RSS feed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Helper function to update the RSS feed file
fn update_rss_feed(ai_events: &[AiEvent]) -> std::io::Result<()> {
    use std::io::Write;
    
    let mut rss_content = String::new();
    rss_content.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>BlackBook AI Prediction Events</title>
    <description>AI-generated prediction market events from various sources</description>
    <link>http://localhost:3000/ai/events/feed.rss</link>
    <atom:link href="http://localhost:3000/ai/events/feed.rss" rel="self" type="application/rss+xml" />
    <language>en-us</language>
    <managingEditor>ai-agent@blackbook.market</managingEditor>
    <webMaster>admin@blackbook.market</webMaster>
"#);
    
    // Add last build date
    let now = chrono::Utc::now();
    rss_content.push_str(&format!("    <lastBuildDate>{}</lastBuildDate>\n", 
        now.to_rfc2822()));
    
    rss_content.push_str(r#"    <category>Prediction Markets</category>
    <generator>BlackBook AI Event Generator</generator>
    <ttl>60</ttl>
    
"#);
    
    // Add each AI event as an RSS item (most recent first)
    for event in ai_events.iter().rev().take(100) {
        let event_date = chrono::DateTime::from_timestamp(event.created_at as i64, 0)
            .unwrap_or(chrono::Utc::now());
        
        let status_badge = if event.added_to_ledger {
            "✅ ACTIVE MARKET"
        } else {
            "📋 RSS ONLY"
        };
        
        rss_content.push_str("    <item>\n");
        rss_content.push_str(&format!("      <title><![CDATA[{} - {}]]></title>\n", 
            status_badge, event.event.title));
        rss_content.push_str(&format!("      <description><![CDATA[{}]]></description>\n", 
            event.event.description));
        rss_content.push_str(&format!("      <link>{}</link>\n", event.event.source_url));
        rss_content.push_str(&format!("      <guid isPermaLink=\"false\">{}</guid>\n", event.id));
        rss_content.push_str(&format!("      <pubDate>{}</pubDate>\n", event_date.to_rfc2822()));
        rss_content.push_str(&format!("      <category>{}</category>\n", event.event.category));
        rss_content.push_str(&format!("      <source url=\"{}\"><![CDATA[{}]]></source>\n", 
            event.source.url, event.source.domain));
        
        // Add custom elements for prediction market data
        rss_content.push_str(&format!("      <confidence>{:.3}</confidence>\n", event.event.confidence));
        rss_content.push_str(&format!("      <addedToLedger>{}</addedToLedger>\n", event.added_to_ledger));
        
        if let Some(ref market_id) = event.market_id {
            rss_content.push_str(&format!("      <marketId>{}</marketId>\n", market_id));
            rss_content.push_str(&format!("      <marketUrl>http://localhost:3000/markets/{}</marketUrl>\n", market_id));
        }
        
        rss_content.push_str("      <options>\n");
        for option in &event.event.options {
            rss_content.push_str(&format!("        <option><![CDATA[{}]]></option>\n", option));
        }
        rss_content.push_str("      </options>\n");
        
        rss_content.push_str("    </item>\n");
    }
    
    rss_content.push_str("  </channel>\n</rss>");
    
    // Write to file
    let mut file = std::fs::File::create("src/event.rss")?;
    file.write_all(rss_content.as_bytes())?;
    
    Ok(())
}

/// GET /activities - Get all market activities (receipts)
async fn get_market_activities(
    State(state): State<SharedState>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    // Get activities sorted by newest first
    let mut activities = app_state.market_activities.clone();
    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    Json(json!({
        "count": activities.len(),
        "activities": activities
    }))

}
