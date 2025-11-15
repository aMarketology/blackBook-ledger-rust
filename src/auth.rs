use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique L1 wallet address
/// Format: L1_[32 hex characters]
pub fn generate_wallet_address(user_id: &str, username: &str) -> String {
    let mut hasher = Sha256::new();
    
    // Combine user_id, username, and timestamp for uniqueness
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let unique_string = format!("{}{}{}", user_id, username, timestamp);
    hasher.update(unique_string.as_bytes());
    
    let result = hasher.finalize();
    let hex_hash = format!("{:x}", result);
    
    // Take first 32 characters and convert to uppercase
    format!("L1_{}", hex_hash[..32].to_uppercase())
}

/// User account with blockchain wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,              // Supabase user ID
    pub username: String,         // Display name
    pub wallet_address: String,   // L1 blockchain address
    pub created_at: u64,         // Unix timestamp
    pub is_test_account: bool,   // False for real users, true for GOD MODE accounts
}

impl User {
    /// Create a new real user with generated wallet
    pub fn new(id: String, username: String) -> Self {
        let wallet_address = generate_wallet_address(&id, &username);
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            username,
            wallet_address,
            created_at,
            is_test_account: false,
        }
    }

    /// Create a test account (GOD MODE)
    pub fn new_test_account(username: String, wallet_address: String) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: format!("test_{}", username.to_lowercase()),
            username,
            wallet_address,
            created_at,
            is_test_account: true,
        }
    }
}

/// In-memory user registry
pub struct UserRegistry {
    users: Arc<Mutex<Vec<User>>>,
}

impl UserRegistry {
    pub fn new() -> Self {
        let users = Arc::new(Mutex::new(Vec::<User>::new()));

        // Initialize 8 test accounts (GOD MODE)
        let test_accounts = vec![
            ("ALICE", "L1_C3B4954FC9A54D8281181665A7B9CAD3"),
            ("BOB", "L1_3C63CF73361546CE83B9F063E3ED7CB2"),
            ("CHARLIE", "L1_CEBFA3BED0594FA697EA44697815D362"),
            ("DIANA", "L1_742B496474934DF4A1370D26361F6288"),
            ("ETHAN", "L1_8E8B5E5C4E06486D96198B3825D8BA30"),
            ("FIONA", "L1_ABBBA7408B6045B3A88339FE76FF7143"),
            ("GEORGE", "L1_10AAA1CAEBA14CEB8ACA7E5EABEEAE40"),
            ("HANNAH", "L1_BF7DAF95D06246039EF29F048D193646"),
        ];

        let mut users_vec = Vec::new();
        for (name, address) in test_accounts {
            users_vec.push(User::new_test_account(name.to_string(), address.to_string()));
        }

        Self {
            users: Arc::new(Mutex::new(users_vec)),
        }
    }

    /// Add a new real user
    pub async fn add_user(&self, user: User) -> Result<(), String> {
        let mut users = self.users.lock().await;
        
        // Check if user already exists
        if users.iter().any(|u| u.id == user.id) {
            return Err("User already exists".to_string());
        }

        users.push(user);
        Ok(())
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, id: &str) -> Option<User> {
        let users = self.users.lock().await;
        users.iter().find(|u| u.id == id).cloned()
    }

    /// Get user by wallet address
    pub async fn get_user_by_wallet(&self, wallet_address: &str) -> Option<User> {
        let users = self.users.lock().await;
        users.iter().find(|u| u.wallet_address == wallet_address).cloned()
    }

    /// Get all users
    pub async fn get_all_users(&self) -> Vec<User> {
        let users = self.users.lock().await;
        users.clone()
    }

    /// Get only real users (not test accounts)
    pub async fn get_real_users(&self) -> Vec<User> {
        let users = self.users.lock().await;
        users.iter().filter(|u| !u.is_test_account).cloned().collect()
    }

    /// Get only test accounts (GOD MODE)
    pub async fn get_test_accounts(&self) -> Vec<User> {
        let users = self.users.lock().await;
        users.iter().filter(|u| u.is_test_account).cloned().collect()
    }
}

/// Supabase authentication helper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseConfig {
    pub url: String,
    pub anon_key: String,
}

impl SupabaseConfig {
    pub fn from_env() -> Result<Self, String> {
        dotenv::dotenv().ok();
        
        let url = std::env::var("NEXT_PUBLIC_SUPABASE_URL")
            .map_err(|_| "NEXT_PUBLIC_SUPABASE_URL not set".to_string())?;
        
        let anon_key = std::env::var("NEXT_PUBLIC_SUPABASE_ANON_KEY")
            .map_err(|_| "NEXT_PUBLIC_SUPABASE_ANON_KEY not set".to_string())?;

        Ok(Self { url, anon_key })
    }

    /// Verify a Supabase JWT token
    pub async fn verify_token(&self, token: &str) -> Result<String, String> {
        let client = reqwest::Client::new();
        
        let response = client
            .get(format!("{}/auth/v1/user", self.url))
            .header("Authorization", format!("Bearer {}", token))
            .header("apikey", &self.anon_key)
            .send()
            .await
            .map_err(|e| format!("Failed to verify token: {}", e))?;

        if !response.status().is_success() {
            return Err("Invalid token".to_string());
        }

        let user_data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse user data: {}", e))?;

        let user_id = user_data["id"]
            .as_str()
            .ok_or("User ID not found")?
            .to_string();

        Ok(user_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user: User,
    pub token: String,
    pub wallet_address: String,
    pub initial_balance: f64,
}
