# BlackBook L1/L2 Integration - Next Steps

## Architecture Overview

BlackBook operates as a **unified ledger** where L1 (blockchain core) and L2 (prediction market) share the same state. There is no bridging - they are one seamless system.

```
┌─────────────────────────────────────────────────────────────┐
│                    UNIFIED BLACKBOOK LEDGER                 │
│                                                             │
│   L1 (Blockchain Core)  ←────────→  L2 (Prediction Market)  │
│                                                             │
│   • Wallet creation                • Bet placement          │
│   • Token minting                  • Market resolution      │
│   • Core transfers                 • Payouts                │
│                                                             │
│   Same accounts, same balances, same transaction history    │
└─────────────────────────────────────────────────────────────┘
```

---

## 1. Add User Activity Endpoint

**Goal:** Expose filtered view of ledger for individual user's "📜 Recent Activity"

### Create `src/routes/activity.rs`

```rust
use axum::{
    extract::{Path, State, Query},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::app_state::SharedState;

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub tx_type: Option<String>,
}

/// GET /activity/:account
/// 
/// Returns user's activity filtered from the global ledger.
/// This is NOT separate storage - just a query view.
pub async fn get_user_activity(
    State(state): State<SharedState>,
    Path(account): Path<String>,
    Query(params): Query<ActivityQuery>,
) -> Json<Value> {
    let app_state = state.lock().unwrap();
    
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    
    // Get user's recipes (activity receipts) from unified ledger
    let mut recipes = app_state.ledger.get_account_recipes(&account);
    
    // Filter by tx_type if specified
    if let Some(ref tx_type) = params.tx_type {
        recipes.retain(|r| r.recipe_type == *tx_type);
    }
    
    // Sort by timestamp (newest first)
    recipes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    // Apply pagination
    let total = recipes.len();
    let paginated: Vec<_> = recipes.into_iter()
        .skip(offset)
        .take(limit)
        .map(|r| json!({
            "id": r.id,
            "type": r.recipe_type,
            "amount": r.amount,
            "description": r.description,
            "related_id": r.related_id,
            "timestamp": r.timestamp,
        }))
        .collect();
    
    Json(json!({
        "account": account,
        "activity": paginated,
        "total": total,
        "limit": limit,
        "offset": offset
    }))
}
```

### Update `src/routes/mod.rs`

```rust
pub mod auth;
pub mod activity;

pub use auth::*;
pub use activity::*;
```

### Update `src/main.rs` - Add Route

```rust
use routes::{login, get_user, get_user_activity};

// In router:
.route("/activity/:account", get(get_user_activity))
```

---

## 2. Transaction Types on Unified Ledger

All activity is recorded in the global ledger with these `tx_type` / `recipe_type` values:

| Type | Description | Example |
|------|-------------|---------|
| `transfer` | Token transfer between accounts | Alice sends 50 BB to Bob |
| `bet` | Bet placed on prediction market | Alice bets 100 BB on "Yes" |
| `payout` | Winnings from resolved market | Alice wins 180 BB |
| `admin_deposit` | Admin minted tokens | New user gets 100 BB welcome bonus |
| `market_created` | New market created | "Will X happen?" market launched |
| `market_resolved` | Market outcome determined | "Yes" wins, payouts distributed |

---

## 3. Frontend Integration

### User Profile Page - Recent Activity

```tsx
// Fetch user's personal activity (filtered view of ledger)
const { data } = await fetch(`/activity/${walletAddress}?limit=20`);

// Display in "📜 Recent Activity" section
{data.activity.map(item => (
  <ActivityItem 
    type={item.type}
    amount={item.amount}
    description={item.description}
    timestamp={item.timestamp}
  />
))}
```

### Global Ledger Monitor (Admin/Debug)

```tsx
// Fetch ALL ledger activity (unfiltered)
const { data } = await fetch('/ledger');

// Display in "📡 Monitoring all ledger actions" view
{data.activity.map(entry => (
  <LedgerEntry entry={entry} />
))}
```

---

## 4. API Endpoints Summary

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/auth/login` | POST | Login with Supabase JWT, create wallet if new |
| `/auth/user` | GET | Get authenticated user info |
| `/markets` | GET | List all prediction markets |
| `/markets` | POST | Create new market |
| `/markets/:id` | GET | Get market details |
| `/bet/signed` | POST | Place bet (cryptographic signature required) |
| `/bets/:account` | GET | Get user's bet history |
| `/balance/:account` | GET | Get account balance |
| `/transfer` | POST | Transfer tokens between accounts |
| `/ledger` | GET | **Global ledger activity** (all transactions) |
| `/activity/:account` | GET | **User's activity** (filtered from ledger) |
| `/rpc/nonce/:address` | GET | Get nonce for transaction signing |

---

## 5. Key Principles

### ✅ Single Source of Truth
- All transactions stored in `ledger.transactions` and `ledger.recipes`
- No duplicate storage for L1 vs L2

### ✅ Unified Wallet
- One wallet address (`L1_ABC123...`) works everywhere
- No bridging needed between layers

### ✅ Immutable History
- Every action creates a `Recipe` (activity receipt)
- Transactions cannot be modified after creation

### ✅ Filtered Views, Not Separate Data
- User activity = `ledger.get_account_recipes(account)`
- Global feed = `ledger.get_all_recipes()`
- Same data, different query

---

## 6. Implementation Checklist

- [ ] Create `src/routes/activity.rs` with `get_user_activity` handler
- [ ] Update `src/routes/mod.rs` to export activity module
- [ ] Add `/activity/:account` route to `main.rs`
- [ ] Update console endpoint list in `main.rs`
- [ ] Test with `curl http://localhost:1234/activity/ALICE`
- [ ] Frontend: Update profile page to call `/activity/:account`
- [ ] Frontend: Display "📜 Recent Activity" from response

---

## 7. Example API Responses

### GET /activity/ALICE?limit=3

```json
{
  "account": "ALICE",
  "activity": [
    {
      "id": "recipe_abc123",
      "type": "bet",
      "amount": 50.0,
      "description": "Placed 50 BB bet on market meta_ai_chip_migration",
      "related_id": "meta_ai_chip_migration",
      "timestamp": 1733900000
    },
    {
      "id": "recipe_xyz789",
      "type": "admin_deposit",
      "amount": 100.0,
      "description": "Admin minted 100 BB to wallet",
      "related_id": null,
      "timestamp": 1733899000
    },
    {
      "id": "recipe_def456",
      "type": "transfer",
      "amount": 25.0,
      "description": "Transferred 25 BB to BOB",
      "related_id": null,
      "timestamp": 1733898000
    }
  ],
  "total": 3,
  "limit": 3,
  "offset": 0
}
```

### GET /ledger (Global - shows everything)

```json
{
  "activity": [
    "[12:34:56] 🎯 BET | ALICE bet 50 BB on meta_ai_chip_migration",
    "[12:33:21] 💸 TRANSFER | BOB sent 100 BB to CHARLIE",
    "[12:32:00] 🆕 NEW_USER | david registered | Wallet: L1_DEF456...",
    ...
  ]
}
```
