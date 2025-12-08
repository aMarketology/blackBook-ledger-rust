# Layer 1 ↔ Layer 2 Integration Guide

## Architecture Overview

BlackBook operates a two-layer blockchain architecture:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LAYER 1 (Consensus)                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │ Proof of    │  │ Ed25519     │  │ Cross-Layer │  │ Token       │ │
│  │ History     │  │ Signatures  │  │ RPC Bridge  │  │ Minting     │ │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘ │
│                              ▲                                        │
│                              │ Signature Verification                 │
│                              │ Account Lookup                         │
│                              │ Bridge Settlement                      │
│                              ▼                                        │
├─────────────────────────────────────────────────────────────────────┤
│                        LAYER 2 (Prediction Market)                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
│  │ Markets     │  │ Bets &      │  │ Liquidity   │  │ User        │ │
│  │ Engine      │  │ Settlements │  │ Pools       │  │ Wallets     │ │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

## Network Details

| Layer | Purpose | Port | Address Prefix | Derivation Path |
|-------|---------|------|----------------|-----------------|
| L1 | Consensus & Validation | 8080/Railway | `bb1_` | `m/44'/9000'/1'/0'` |
| L2 | Prediction Markets | 1234 | `bb2_` | `m/44'/9000'/2'/0'` |

**Key Design Principle**: Same mnemonic → Different derivation paths → Linked L1/L2 addresses

---

## What's Already Coded

### ✅ 1. Dual Address Derivation (`src/wallet.rs`)

```rust
// Constants
pub const BLACKBOOK_L1_PATH: &str = "m/44'/9000'/1'/0'";
pub const BLACKBOOK_L2_PATH: &str = "m/44'/9000'/2'/0'";
pub const BLACKBOOK_L1_PREFIX: &str = "bb1_";
pub const BLACKBOOK_L2_PREFIX: &str = "bb2_";

// Keypair structure
pub struct BlackBookKeypairs {
    pub l1: Keypair,  // Ed25519 keypair for L1
    pub l2: Keypair,  // Ed25519 keypair for L2
}

// Derivation function
pub fn derive_blackbook_keypairs(mnemonic: &str, passphrase: &str) 
    -> Result<BlackBookKeypairs, WalletError>
```

### ✅ 2. Cross-Layer RPC Service (`src/cross_layer_rpc.rs`)

The `CrossLayerRPC` service provides:

| Method | Purpose | Status |
|--------|---------|--------|
| `verify_l1_signature()` | L2 verifies L1 transaction signatures | ✅ Implemented |
| `get_account()` | Query account info across layers | ✅ Implemented |
| `initiate_bridge()` | Start token bridge L1→L2 or L2→L1 | ✅ Implemented |
| `get_bridge_status()` | Check pending bridge status | ✅ Implemented |

**Request/Response Types**:
```rust
// L2 calls this to verify an L1 signature
pub struct VerifyL1SignatureRequest {
    pub transaction: SignedTransaction,
    pub expected_sender: Option<String>,
}

pub struct VerifyL1SignatureResponse {
    pub valid: bool,
    pub tx_hash: String,
    pub sender: String,
    pub verified_at: u64,
    pub l1_block_height: u64,
    pub l1_slot: u64,
    pub error: Option<String>,
}

// Bridge tokens between layers
pub struct BridgeRequest {
    pub signed_tx: SignedTransaction,
    pub target_layer: String,
    pub target_address: String,
}

pub struct BridgeResponse {
    pub success: bool,
    pub bridge_id: String,
    pub amount: f64,
    pub from_layer: String,
    pub to_layer: String,
    pub estimated_completion_slot: u64,
}
```

### ✅ 3. Signed Transaction Envelope (`src/signed_transaction.rs`)

Transactions can be verified by either layer:

```rust
pub struct SignedTransaction {
    pub sender_pubkey: String,     // 64 hex chars
    pub nonce: u64,                // Replay protection
    pub timestamp: u64,            // Unix timestamp
    pub tx_type: SignedTxType,     // Transfer, Bridge, Bet, etc.
    pub payload: TransactionPayload,
    pub signature: String,         // ed25519 sig (128 hex chars)
}

pub enum SignedTxType {
    Transfer = 0,
    SocialAction = 1,
    Stake = 2,
    Unstake = 3,
    Bridge = 4,       // ← Cross-layer bridge
    Contract = 5,
    System = 6,
    BetPlacement = 7, // ← L2 prediction market
    BetResolution = 8,
}

// Verify on either layer
pub fn verify_cross_layer(tx: &SignedTransaction, expected_layer: &str) 
    -> VerificationResult
```

### ✅ 4. Supabase Dual Address Storage (`src/supabase_connector.rs`)

User profiles store both L1 and L2 addresses:

```rust
// Profile fields (stored in Supabase)
{
    "blackbook_l1_address": "bb1_abc123...",  // L1 address
    "blackbook_l2_address": "bb2_def456...",  // L2 address
    "encrypted_wallet": "...",                 // AES-256-GCM vault
}
```

### ✅ 5. Wallet Endpoints (`src/routes/wallet.rs`)

| Endpoint | Purpose | L2 Can Call? |
|----------|---------|--------------|
| `POST /wallet/create` | Generate new mnemonic + keypairs | Yes |
| `POST /wallet/initialize` | Create/import wallet | Yes |
| `POST /wallet/connect` | Link external wallet | Yes |
| `DELETE /wallet/delete` | Remove wallet from profile | Yes |

---

## What Needs to Be Coded

### 🔧 1. HTTP Routes for Cross-Layer RPC

The `CrossLayerRPC` service exists but **no HTTP endpoints** expose it. L2 needs to call L1 via HTTP.

**Required Routes** (add to `src/main.rs`):

```rust
// POST /rpc/verify-l1-signature
// L2 calls this to verify an L1 signature
fn verify_l1_signature_route() {
    warp::path!("rpc" / "verify-l1-signature")
        .and(warp::post())
        .and(warp::body::json::<VerifyL1SignatureRequest>())
        .and_then(|request| async move {
            let rpc = CrossLayerRPC::new();
            let response = rpc.verify_l1_signature(&request);
            Ok::<_, Rejection>(warp::reply::json(&response))
        })
}

// POST /rpc/bridge
// Initiate L1↔L2 token bridge
fn bridge_route() {
    warp::path!("rpc" / "bridge")
        .and(warp::post())
        .and(warp::body::json::<BridgeRequest>())
        .and_then(|request| async move {
            let rpc = CrossLayerRPC::new();
            let response = rpc.initiate_bridge(&request);
            Ok::<_, Rejection>(warp::reply::json(&response))
        })
}

// GET /rpc/bridge/{bridge_id}
// Check bridge status
fn bridge_status_route() {
    warp::path!("rpc" / "bridge" / String)
        .and(warp::get())
        .and_then(|bridge_id| async move {
            let rpc = CrossLayerRPC::new();
            match rpc.get_bridge_status(&bridge_id) {
                Some(status) => Ok(warp::reply::json(&status)),
                None => Err(warp::reject::not_found()),
            }
        })
}

// GET /rpc/account/{address}
// Get account info (callable by L2)
fn account_route() {
    warp::path!("rpc" / "account" / String)
        .and(warp::get())
        .and_then(|address| async move {
            let rpc = CrossLayerRPC::new();
            let request = GetAccountRequest { 
                address, 
                layer: None 
            };
            let response = rpc.get_account(&request);
            Ok::<_, Rejection>(warp::reply::json(&response))
        })
}
```

### 🔧 2. L2 Settlement Finalization on L1

When L2 resolves a bet, L1 needs to record the settlement for:
- Audit trail
- Cross-layer balance sync
- Dispute resolution

**Required**: Settlement recording endpoint

```rust
// POST /rpc/settlement
// L2 calls this after resolving a bet
pub struct SettlementRequest {
    pub market_id: String,
    pub outcome: String,
    pub winners: Vec<(String, f64)>,   // (bb2_address, payout)
    pub l2_block_height: u64,
    pub l2_signature: String,          // L2 validator signature
}

pub struct SettlementResponse {
    pub recorded: bool,
    pub l1_tx_hash: String,
    pub l1_slot: u64,
}
```

### 🔧 3. L1→L2 Callback (Bridge Completion)

When a bridge from L1→L2 completes, L1 needs to notify L2:

```rust
// L1 calls L2's endpoint after bridge confirmation
// L2 endpoint: POST http://localhost:1234/bridge/complete
pub struct BridgeCompleteNotification {
    pub bridge_id: String,
    pub from_address: String,   // bb1_...
    pub to_address: String,     // bb2_...
    pub amount: f64,
    pub l1_tx_hash: String,
    pub l1_slot: u64,
}
```

### 🔧 4. Unified Balance Query

L1 should be able to aggregate balances across both layers:

```rust
// GET /balance/unified/{user_id}
// Returns L1 + L2 balances
pub struct UnifiedBalance {
    pub user_id: String,
    pub l1_address: String,
    pub l2_address: String,
    pub l1_balance: f64,
    pub l2_balance: f64,
    pub total_bb: f64,
    pub pending_bridges: Vec<PendingBridge>,
}
```

### 🔧 5. Cross-Layer Nonce Sync

Both layers maintain nonces. For cross-layer transactions, nonces must be coordinated:

```rust
// GET /rpc/nonce/{address}
// L2 queries L1 for the next valid cross-layer nonce
pub struct NonceResponse {
    pub address: String,
    pub l1_nonce: u64,
    pub cross_layer_nonce: u64,
    pub last_l1_activity_slot: u64,
}
```

---

## Integration Flow: L2 Prediction Market → L1 Validation

### Flow 1: User Places a Bet on L2

```
User                L2 (port 1234)              L1 (port 8080)
  │                      │                            │
  ├──POST /bet/place────►│                            │
  │  {market_id, amount, │                            │
  │   prediction, sig}   │                            │
  │                      │                            │
  │                      ├──POST /rpc/verify-l1-sig──►│
  │                      │  {signed_tx, expected}     │
  │                      │                            │
  │                      │◄─────{valid: true}─────────│
  │                      │                            │
  │                      │  [Process bet locally]     │
  │                      │                            │
  │◄──{bet_id, status}───│                            │
```

### Flow 2: Bridge Tokens L1→L2

```
User                L1 (port 8080)              L2 (port 1234)
  │                      │                            │
  ├──POST /rpc/bridge───►│                            │
  │  {signed_tx, L2,     │                            │
  │   target_address}    │                            │
  │                      │                            │
  │                      │  [Lock tokens on L1]       │
  │                      │  [Generate bridge_id]      │
  │                      │                            │
  │◄──{bridge_id, est}───│                            │
  │                      │                            │
  │                      │  [After confirmation]      │
  │                      │                            │
  │                      ├──POST /bridge/complete────►│
  │                      │  {bridge_id, amount}       │
  │                      │                            │
  │                      │◄────{minted: true}─────────│
```

### Flow 3: Market Settlement (L2→L1 Record)

```
L2 Oracle           L2 (port 1234)              L1 (port 8080)
  │                      │                            │
  ├──POST /admin/resolve►│                            │
  │  {market_id, outcome}│                            │
  │                      │                            │
  │                      │  [Calculate winners]       │
  │                      │  [Distribute payouts]      │
  │                      │                            │
  │                      ├──POST /rpc/settlement─────►│
  │                      │  {market_id, winners}      │
  │                      │                            │
  │                      │◄────{recorded, l1_hash}────│
  │                      │                            │
  │◄──{resolved, hash}───│                            │
```

---

## L2 Endpoints Reference

### HTTP Endpoints (port 1234)

L2 exposes these endpoints. L1 may need to call some of these:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/health` | GET | Health check |
| `/markets` | GET | List all markets |
| `/markets/{id}` | GET | Get market details |
| `/bet/place` | POST | Place a bet |
| `/bet/{id}` | GET | Get bet status |
| `/wallet/balance/{addr}` | GET | Get L2 balance |
| `/wallet/transfer` | POST | Transfer on L2 |
| `/bridge/complete` | POST | ← L1 calls this |
| `/admin/resolve` | POST | Resolve market |

### IPC Commands (internal)

L2 also supports 27 IPC commands for internal operations. These are not typically called cross-layer.

---

## Shared Authentication

Both layers use the same authentication system:

### Fork Authentication (Password Split)

```
User Password
      │
      ├── SHA256 → Login_Pass → Supabase auth
      │
      └── Argon2id → Wallet_Key → Decrypt mnemonic vault
```

### JWT Tokens

L1 issues JWTs that are valid on L2:
- Same JWT secret (configured in environment)
- Same user_id claim
- L2 should verify JWT with L1's public key (or shared secret)

### Recommended: Cross-Layer Token Validation

```rust
// L2 should validate L1-issued JWTs before processing requests
// Add header: Authorization: Bearer <L1_JWT>

// L2 can call L1 to validate:
// GET /auth/validate?token=<jwt>
// Returns: { valid: true, user_id: "...", expires_at: ... }
```

---

## Environment Configuration

### L1 Environment Variables

```env
# L1 Server
PORT=8080
L1_RPC_URL=https://your-l1-node.railway.app

# L2 Connection (for callbacks)
L2_RPC_URL=http://localhost:1234

# Supabase (shared)
SUPABASE_URL=https://xxx.supabase.co
SUPABASE_KEY=your_key

# JWT (shared secret)
JWT_SECRET=your_256_bit_secret
```

### L2 Environment Variables

```env
# L2 Server
PORT=1234
L2_RPC_URL=http://localhost:1234

# L1 Connection (for verification)
L1_RPC_URL=https://your-l1-node.railway.app

# Supabase (shared)
SUPABASE_URL=https://xxx.supabase.co
SUPABASE_KEY=your_key

# JWT (shared secret - same as L1)
JWT_SECRET=your_256_bit_secret
```

---

## Implementation Checklist

### Phase 1: Core Integration (Required)
- [ ] Add `/rpc/verify-l1-signature` HTTP route to L1
- [ ] Add `/rpc/bridge` HTTP route to L1
- [ ] Add `/rpc/account/{address}` HTTP route to L1
- [ ] L2: Call L1 verification before processing bets
- [ ] L2: Add `/bridge/complete` endpoint

### Phase 2: Settlement & Sync
- [ ] Add `/rpc/settlement` route to L1
- [ ] Add `/balance/unified/{user_id}` route to L1
- [ ] L2: Call L1 after market resolution
- [ ] Implement bridge completion callback (L1→L2)

### Phase 3: Advanced
- [ ] Cross-layer nonce synchronization
- [ ] Unified transaction history endpoint
- [ ] Cross-layer dispute resolution
- [ ] L2 validator signatures for settlements

---

## SDK Usage Examples

### JavaScript SDK: Place Bet on L2 with L1 Verification

```javascript
import { BlackBookSDK } from './blackbook-sdk.js';

const sdk = new BlackBookSDK('https://l1.blackbook.io', 'https://l2.blackbook.io');

// 1. Sign in with Fork auth (hits L1)
await sdk.signIn('email@example.com', 'password');

// 2. Place bet (L2 internally verifies signature with L1)
const result = await sdk.placeBet({
    marketId: 'super-bowl-2025',
    prediction: 'team_a_wins',
    amount: 100.0
});

console.log('Bet placed:', result.bet_id);
```

### Rust: L2 Verifies Transaction with L1

```rust
use reqwest::Client;
use layer1::{VerifyL1SignatureRequest, SignedTransaction};

async fn verify_with_l1(tx: SignedTransaction) -> bool {
    let client = Client::new();
    let l1_url = std::env::var("L1_RPC_URL").unwrap();
    
    let request = VerifyL1SignatureRequest {
        transaction: tx,
        expected_sender: None,
    };
    
    let response = client
        .post(format!("{}/rpc/verify-l1-signature", l1_url))
        .json(&request)
        .send()
        .await
        .unwrap();
    
    let result: VerifyL1SignatureResponse = response.json().await.unwrap();
    result.valid
}
```

---

## Testing Integration

### 1. Test L1 Verification Endpoint

```bash
# Start L1
cargo run

# Test verification (from another terminal)
curl -X POST http://localhost:8080/rpc/verify-l1-signature \
  -H "Content-Type: application/json" \
  -d '{
    "transaction": {
      "sender_pubkey": "abc123...",
      "nonce": 1,
      "timestamp": 1700000000,
      "tx_type": "Transfer",
      "payload": {"type": "Transfer", "to": "def456", "amount": 10.0},
      "signature": "sig_hex..."
    }
  }'
```

### 2. Test Bridge Flow

```bash
# Initiate bridge L1→L2
curl -X POST http://localhost:8080/rpc/bridge \
  -H "Content-Type: application/json" \
  -d '{
    "signed_tx": { ... },
    "target_layer": "L2",
    "target_address": "bb2_..."
  }'

# Check status
curl http://localhost:8080/rpc/bridge/bridge_12345_abc
```

---

## Summary

| Component | L1 Status | L2 Status | Integration |
|-----------|-----------|-----------|-------------|
| Address derivation | ✅ Done | ✅ Done | ✅ Linked |
| Signature verification | ✅ Done | 🔧 Needs HTTP | 🔧 Pending |
| Bridge logic | ✅ Done | 🔧 Needs endpoint | 🔧 Pending |
| Settlement recording | 🔧 Needs route | 🔧 Needs call | 🔧 Pending |
| Shared auth (JWT) | ✅ Done | 🔧 Verify | 🔧 Pending |
| Supabase dual address | ✅ Done | ✅ Done | ✅ Working |

**Next Steps**:
1. Add HTTP routes in L1 for cross-layer RPC
2. Implement L2's call to L1 for signature verification
3. Add bridge completion callback flow
4. Test end-to-end with prediction market bet flow
