use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Represents a locked escrow account for a specific market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowAccount {
    pub id: String,
    pub market_id: String,
    pub total_locked: u64,
    pub user_deposits: HashMap<String, u64>, // account -> locked amount
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub status: EscrowStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EscrowStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "settled")]
    Settled,
}

/// Manages all escrow accounts for the prediction market
#[derive(Debug)]
pub struct EscrowManager {
    accounts: HashMap<String, EscrowAccount>, // escrow_id -> EscrowAccount
    market_escrows: HashMap<String, Vec<String>>, // market_id -> [escrow_ids]
}

impl EscrowManager {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            market_escrows: HashMap::new(),
        }
    }

    /// Create a new escrow account for a market
    pub fn create_escrow(&mut self, market_id: &str) -> EscrowAccount {
        let escrow_id = Uuid::new_v4().to_string();
        let escrow = EscrowAccount {
            id: escrow_id.clone(),
            market_id: market_id.to_string(),
            total_locked: 0,
            user_deposits: HashMap::new(),
            created_at: Utc::now(),
            resolved_at: None,
            status: EscrowStatus::Active,
        };

        self.accounts.insert(escrow_id.clone(), escrow.clone());
        
        self.market_escrows
            .entry(market_id.to_string())
            .or_insert_with(Vec::new)
            .push(escrow_id);

        escrow
    }

    /// Lock funds when a bet is placed
    /// Returns the escrow account after deposit
    pub fn lock_funds(
        &mut self,
        market_id: &str,
        account: &str,
        amount: u64,
    ) -> Result<EscrowAccount, String> {
        // Find escrow for this market (most recent)
        let escrow_ids = self
            .market_escrows
            .get(market_id)
            .ok_or(format!("No escrow found for market {}", market_id))?;

        if escrow_ids.is_empty() {
            return Err(format!("No escrow found for market {}", market_id));
        }

        let escrow_id = escrow_ids.last().unwrap().clone();
        let escrow = self
            .accounts
            .get_mut(&escrow_id)
            .ok_or("Escrow not found".to_string())?;

        if escrow.status != EscrowStatus::Active {
            return Err("Escrow is not active".to_string());
        }

        // Add to user's locked balance
        let current = escrow.user_deposits.get(account).copied().unwrap_or(0);
        escrow.user_deposits.insert(account.to_string(), current + amount);
        escrow.total_locked += amount;

        Ok(escrow.clone())
    }

    /// Release funds after market resolution
    /// Returns amount released for the winning account
    pub fn release_funds(
        &mut self,
        market_id: &str,
        account: &str,
        payout_amount: u64,
    ) -> Result<u64, String> {
        // Find escrow for this market
        let escrow_ids = self
            .market_escrows
            .get(market_id)
            .ok_or(format!("No escrow found for market {}", market_id))?;

        if escrow_ids.is_empty() {
            return Err(format!("No escrow found for market {}", market_id));
        }

        let escrow_id = escrow_ids.last().unwrap().clone();
        let escrow = self
            .accounts
            .get_mut(&escrow_id)
            .ok_or("Escrow not found".to_string())?;

        // Verify user has locked funds
        let locked = escrow
            .user_deposits
            .get(account)
            .copied()
            .ok_or(format!("No locked funds for account {}", account))?;

        if locked == 0 {
            return Err("No locked funds to release".to_string());
        }

        // Reduce locked amount (remove original bet, keep only profit)
        escrow.user_deposits.insert(account.to_string(), 0);
        escrow.total_locked -= locked;

        Ok(payout_amount)
    }

    /// Refund all funds for a market (if market is cancelled)
    pub fn refund_market(&mut self, market_id: &str) -> Result<HashMap<String, u64>, String> {
        let escrow_ids = self
            .market_escrows
            .get(market_id)
            .ok_or(format!("No escrow found for market {}", market_id))?
            .clone();

        if escrow_ids.is_empty() {
            return Err(format!("No escrow found for market {}", market_id));
        }

        let escrow_id = escrow_ids.last().unwrap().clone();
        let escrow = self
            .accounts
            .get_mut(&escrow_id)
            .ok_or("Escrow not found".to_string())?;

        let refunds = escrow.user_deposits.clone();

        // Clear all deposits
        escrow.user_deposits.clear();
        escrow.total_locked = 0;
        escrow.status = EscrowStatus::Settled;
        escrow.resolved_at = Some(Utc::now());

        Ok(refunds)
    }

    /// Get escrow account details
    pub fn get_escrow(&self, market_id: &str) -> Option<EscrowAccount> {
        let escrow_ids = self.market_escrows.get(market_id)?;
        escrow_ids.last().and_then(|id| self.accounts.get(id).cloned())
    }

    /// Get user's locked balance in a specific market
    pub fn get_locked_balance(&self, market_id: &str, account: &str) -> u64 {
        self.get_escrow(market_id)
            .and_then(|escrow| escrow.user_deposits.get(account).copied())
            .unwrap_or(0)
    }

    /// Mark escrow as resolved (market has closed)
    pub fn mark_resolved(&mut self, market_id: &str) -> Result<(), String> {
        let escrow_ids = self
            .market_escrows
            .get(market_id)
            .ok_or(format!("No escrow found for market {}", market_id))?
            .clone();

        if let Some(escrow_id) = escrow_ids.last() {
            if let Some(escrow) = self.accounts.get_mut(escrow_id) {
                escrow.status = EscrowStatus::Resolved;
                escrow.resolved_at = Some(Utc::now());
                return Ok(());
            }
        }

        Err("Failed to mark escrow as resolved".to_string())
    }

    /// Get all escrows
    pub fn list_escrows(&self) -> Vec<EscrowAccount> {
        self.accounts.values().cloned().collect()
    }

    /// Get total locked across all markets
    pub fn total_locked(&self) -> u64 {
        self.accounts.values().map(|e| e.total_locked).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_escrow() {
        let mut manager = EscrowManager::new();
        let escrow = manager.create_escrow("market_123");

        assert_eq!(escrow.market_id, "market_123");
        assert_eq!(escrow.total_locked, 0);
        assert_eq!(escrow.status, EscrowStatus::Active);
    }

    #[test]
    fn test_lock_funds() {
        let mut manager = EscrowManager::new();
        manager.create_escrow("market_123");

        let result = manager.lock_funds("market_123", "alice", 1000);
        assert!(result.is_ok());

        let escrow = manager.get_escrow("market_123").unwrap();
        assert_eq!(escrow.total_locked, 1000);
        assert_eq!(escrow.user_deposits.get("alice").copied(), Some(1000));
    }

    #[test]
    fn test_release_funds() {
        let mut manager = EscrowManager::new();
        manager.create_escrow("market_123");
        manager.lock_funds("market_123", "alice", 1000).unwrap();

        let payout = manager.release_funds("market_123", "alice", 2000).unwrap();
        assert_eq!(payout, 2000);

        let escrow = manager.get_escrow("market_123").unwrap();
        assert_eq!(escrow.user_deposits.get("alice").copied(), Some(0));
    }

    #[test]
    fn test_refund_market() {
        let mut manager = EscrowManager::new();
        manager.create_escrow("market_123");
        manager.lock_funds("market_123", "alice", 1000).unwrap();
        manager.lock_funds("market_123", "bob", 500).unwrap();

        let refunds = manager.refund_market("market_123").unwrap();
        assert_eq!(refunds.get("alice").copied(), Some(1000));
        assert_eq!(refunds.get("bob").copied(), Some(500));

        let escrow = manager.get_escrow("market_123").unwrap();
        assert_eq!(escrow.total_locked, 0);
    }
}
