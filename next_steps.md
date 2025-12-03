# BlackBook L2 Prediction Market - Implementation Plan

Based on the manifesto and **L1 RPC Bridge Architecture**, here's the updated step-by-step integration plan.

## 🏗️ Architecture Context

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          L1 BLOCKCHAIN (Settlement Layer)                    │
│   • Wallet generation & private key management                               │
│   • Signature verification (Ed25519/secp256k1)                               │
│   • Consensus & finality                                                     │
│   • Authoritative balance state                                              │
│   • RPC Server: submit_transaction, get_balance, get_merkle_proof           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ RPC Bridge (JSON-RPC / HTTP)
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     L2 PREDICTION MARKET LEDGER (This Codebase)              │
│   blackBook-ledger-rust (Port 8080)                                          │
│                                                                              │
│   • RPC Client: Forwards signed transactions to L1                          │
│   • CPMM Engine: Market math & price discovery                               │
│   • Shadow State: Fast reads, optimistic execution                           │
│   • Event Lifecycle: Pending → Provisional → Active → Resolved              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Principle**: This L2 is an *application client* of the L1 blockchain. All state-changing operations must be:
1. **Signed** by the user's private key (client-side)
2. **Submitted** to L1 via RPC for verification
3. **Confirmed** by L1 before being considered final

---

## ✅ Completed Phases

### 🔴 Phase 1: Core CPMM Engine (Foundation) ✅ COMPLETE
| Step | Task | Status |
|------|------|--------|
| 1.1 | Create `src/cpmm.rs` module | ✅ Done (11 tests) |
| 1.2 | Add `EventStatus` enum | ✅ Done |
| 1.3 | Test Phase 1 | ✅ All tests passing |

### 🟡 Phase 2: Data Structure Updates ✅ COMPLETE
| Step | Task | Status |
|------|------|--------|
| 2.1 | Create `PendingEvent` struct | ✅ Done |
| 2.2 | Update `PredictionMarket` struct | ✅ Done |
| 2.3 | Test existing endpoints | ✅ All working |

### 🟠 Phase 3: New Endpoints (Partial)
| Step | Task | Status |
|------|------|--------|
| 3.1 | `GET /events/pending` | ✅ Done |
| 3.2 | `POST /events/:id/launch` | 🔄 Handler written, needs L1 integration |
| 3.3 | `POST /markets/:id/trade` | ⬜ Pending |
| 3.4 | Integration test | ⬜ Pending |

---

## 🚧 NEW: Phase 3.5 - L1 RPC Bridge Integration

Before continuing with trading endpoints, we need to implement the L1 RPC Bridge for trustless authentication.

### Step 3.5.1: Create `src/rpc_client.rs` - L1 RPC Client
**What**: Module to communicate with L1 blockchain via RPC
**Risk**: 🟡 Medium - new dependency on L1 API
**Test**: Mock L1 responses for unit tests

```rust
□ Create L1RpcClient struct with:
  - endpoint_url: String
  - timeout: Duration
  - retry_policy: RetryConfig

□ Implement core methods:
  - submit_transaction(signed_tx: SignedTransaction) -> Result<TxHash>
  - get_balance(address: &str) -> Result<f64>
  - verify_signature(payload: &[u8], sig: &str, addr: &str) -> Result<bool>
  - get_merkle_proof(tx_hash: &str) -> Result<MerkleProof>
  - get_nonce(address: &str) -> Result<u64>

□ Add configuration:
  - L1_RPC_URL environment variable
  - Fallback to mock mode for testing
```

### Step 3.5.2: Create `src/signed_transaction.rs` - Transaction Envelope
**What**: Signed transaction structure that L1 can verify
**Risk**: 🟢 Low - pure data structures
**Test**: Serialization tests

```rust
□ Create SignedTransaction struct:
  - tx_type: TransactionType (PlaceBet, LaunchMarket, AddLiquidity, etc.)
  - payload: serde_json::Value
  - sender_address: String
  - nonce: u64
  - timestamp: u64
  - signature: String (hex-encoded)

□ Create TransactionType enum:
  - PlaceBet
  - LaunchMarket
  - AddLiquidity
  - RemoveLiquidity
  - Trade
  - Redeem
  - Transfer

□ Implement validation:
  - is_expired() - check timestamp window
  - validate_format() - check required fields
```

### Step 3.5.3: Add `POST /tx/submit` - Generic Transaction Endpoint
**What**: Single endpoint for all signed transactions
**Risk**: 🟡 Medium - replaces multiple endpoints
**Test**: Submit mock transactions

```rust
□ Handler: submit_transaction()
  - Accept SignedTransaction in body
  - Forward to L1 via rpc_client.submit_transaction()
  - Wait for L1 confirmation (or use optimistic mode)
  - Execute local state change on success
  - Return tx_hash and status

□ Error handling:
  - L1 unreachable → return 503 with retry hint
  - Invalid signature → return 401
  - Insufficient balance → return 400
  - Nonce mismatch → return 409
```

### Step 3.5.4: Update AppState with RPC Client
**What**: Add L1 client to shared state
**Risk**: 🟡 Medium - modifying core state
**Test**: Server starts with RPC client

```rust
□ Add to AppState:
  - l1_client: L1RpcClient
  - l1_connected: bool
  - pending_txs: HashMap<String, PendingTransaction>

□ Add initialization:
  - Read L1_RPC_URL from env
  - Test connection on startup
  - Log L1 connection status
```

### Step 3.5.5: Add L1 Query Endpoints
**What**: Proxy endpoints to query L1 state
**Risk**: 🟢 Low - read-only
**Test**: Query mock L1

```rust
□ GET /l1/balance/:address
  - Proxy to L1 get_balance()
  - Return authoritative balance

□ GET /l1/nonce/:address
  - Proxy to L1 get_nonce()
  - Return current nonce for signing

□ GET /l1/proof/:tx_hash
  - Proxy to L1 get_merkle_proof()
  - Return cryptographic proof of transaction
```

---

## 🔄 Updated Phase 3: Endpoints (With L1 Integration)

### Step 3.2 (Updated): POST /events/:id/launch
**What**: Launch market with L1 signature verification
**Requires**: Phase 3.5 complete

```rust
□ Accept SignedTransaction with:
  - tx_type: LaunchMarket
  - payload: { event_id, liquidity_amount, betting_closes_at }
  
□ Flow:
  1. Validate SignedTransaction format
  2. Forward to L1 via submit_transaction()
  3. On L1 confirmation:
     - Deduct tokens (L1 handles this)
     - Create local PredictionMarket
     - Initialize CPMMPool
     - Set provisional status
  4. Return market details
```

### Step 3.3 (Updated): POST /markets/:id/trade
**What**: CPMM trading with L1 signature verification
**Requires**: Phase 3.5 complete

```rust
□ Accept SignedTransaction with:
  - tx_type: Trade
  - payload: { market_id, outcome, amount, max_cost }
  
□ Flow:
  1. Validate SignedTransaction format
  2. Forward to L1 via submit_transaction()
  3. On L1 confirmation:
     - Execute CPMM swap locally
     - Update pool reserves
     - Credit outcome tokens
     - Collect LP fees
  4. Return trade result + new prices
```

---

## 🔵 Phase 4: Lifecycle Management (Updated)

### Step 4.1: Viability Checker (Background Task)
**What**: Background job to check provisional markets
**L1 Integration**: Query L1 for confirmed TVL

```rust
□ tokio::spawn a loop that runs every hour
□ Find markets where status == Provisional
□ Query L1 for confirmed balances/deposits
□ If deadline passed:
  - TVL >= 10,000 BB → promote to Active
  - TVL < 10,000 BB → submit RefundMarket tx to L1
□ Log state transitions to L1
```

### Step 4.2: POST /markets/:id/add-liquidity
**What**: Add liquidity with L1 verification

```rust
□ Accept SignedTransaction with:
  - tx_type: AddLiquidity
  - payload: { market_id, amount }
  
□ Forward to L1, then:
  - Calculate proportional deposit
  - Mint LP shares
  - Update pool reserves
```

### Step 4.3: POST /markets/:id/remove-liquidity
**What**: Remove liquidity with L1 verification

```rust
□ Accept SignedTransaction with:
  - tx_type: RemoveLiquidity
  - payload: { market_id, lp_shares }
  
□ Forward to L1, then:
  - Calculate proportional withdrawal
  - Burn LP shares
  - Return tokens via L1 transfer
```

---

## 🟣 Phase 5: Resolution & Settlement (Updated)

### Step 5.1: POST /markets/:id/resolve
**What**: Oracle resolution with L1 finality

```rust
□ Accept SignedTransaction with:
  - tx_type: ResolveMarket
  - payload: { market_id, winning_outcome }
  - Must be from authorized oracle address
  
□ Forward to L1, then:
  - Set market.winning_outcome
  - Set market.status = Resolved
  - L1 handles escrow release
```

### Step 5.2: POST /markets/:id/redeem
**What**: Redeem winning tokens via L1

```rust
□ Accept SignedTransaction with:
  - tx_type: Redeem
  - payload: { market_id, outcome_tokens }
  
□ Forward to L1, then:
  - Verify user holds winning tokens
  - L1 executes 1:1 token exchange
  - Burn redeemed outcome tokens
```

---

## 📋 Updated Implementation Order

| Order | Task | Risk | Est. Time | Depends On |
|-------|------|------|-----------|------------|
| ✅ 1 | Step 1.1: Create `src/cpmm.rs` | 🟢 None | ✅ Done | - |
| ✅ 2 | Step 1.2: Add `EventStatus` enum | 🟢 None | ✅ Done | - |
| ✅ 3 | Step 1.3: Test CPMM math | 🟢 None | ✅ Done | 1, 2 |
| ✅ 4 | Step 2.1: Add `PendingEvent` struct | 🟡 Low | ✅ Done | - |
| ✅ 5 | Step 2.2: Update `PredictionMarket` | 🟡 Medium | ✅ Done | 4 |
| ✅ 6 | Step 2.3: Test existing endpoints | 🟢 None | ✅ Done | 5 |
| ✅ 7 | Step 3.1: `GET /events/pending` | 🟢 None | ✅ Done | 4 |
| **8** | **Step 3.5.1: Create `src/rpc_client.rs`** | 🟡 Medium | 2 hours | L1 API docs |
| **9** | **Step 3.5.2: Create `src/signed_transaction.rs`** | 🟢 Low | 1 hour | - |
| **10** | **Step 3.5.3: Add `POST /tx/submit`** | 🟡 Medium | 1.5 hours | 8, 9 |
| **11** | **Step 3.5.4: Update AppState** | 🟡 Medium | 30 min | 8 |
| **12** | **Step 3.5.5: Add L1 query endpoints** | 🟢 Low | 45 min | 8 |
| 13 | Step 3.2: `POST /events/:id/launch` (updated) | 🟡 Medium | 1 hour | 10 |
| 14 | Step 3.3: `POST /markets/:id/trade` | 🟡 Medium | 1 hour | 10 |
| 15 | Step 3.4: Integration test | 🟢 None | 30 min | 13, 14 |

---

## ❓ Questions to Resolve Before Step 3.5.1

1. **L1 RPC Endpoint URL**: What is the L1 RPC server address?
   - Example: `http://l1-node.blackbook.io:8545`
   - Or: `https://rpc.blackbook.network`

2. **L1 RPC Format**: What protocol does L1 use?
   - [ ] JSON-RPC 2.0 (like Ethereum)
   - [ ] REST API
   - [ ] gRPC
   - [ ] Custom

3. **L1 Signature Scheme**: What crypto does L1 use?
   - [ ] Ed25519 (like Solana)
   - [ ] secp256k1 (like Ethereum/Bitcoin)
   - [ ] Custom

4. **Execution Model**: How should L2 handle L1 confirmation?
   - [ ] **Pessimistic**: Wait for L1 confirmation before updating L2 state (slower, always consistent)
   - [ ] **Optimistic**: Update L2 immediately, rollback if L1 rejects (faster UX, needs rollback logic)

5. **Mock Mode**: Should we build a mock L1 for testing first?
   - [ ] Yes - allows development without L1 dependency
   - [ ] No - connect to real L1 testnet immediately

---

## 🎯 Next Action

**Answer the 5 questions above**, then we can implement Step 3.5.1: Create `src/rpc_client.rs`



Based on your manifesto, here's a careful step-by-step integration plan. We'll go slow, test at each stage, and ensure nothing breaks.

🔴 Phase 1: Core CPMM Engine (Foundation)
Step 1.1: Create src/cpmm.rs module
What: Pure math module with no dependencies on existing code
Risk: 🟢 None - new file, doesn't touch existing code
Test: Unit tests for math calculations

□ Create CPMMPool struct { x: f64, y: f64, k: f64 }
□ Implement new() - initialize pool with 50/50 split
□ Implement calculate_price() - return prices for each outcome
□ Implement calculate_cost() - cost to buy X tokens
□ Implement swap() - execute trade, return cost + fee
□ Add LP_FEE_RATE = 0.02 constant
□ Write unit tests

Step 1.2: Add EventStatus enum
What: Add status enum to track event lifecycle
Risk: 🟢 Low - additive only
Test: Compile check

□ Add to main.rs or new types.rs:
  enum EventStatus { Pending, Provisional, Active, Closed, Resolved, Refunded }
□ Implement Display, Clone, Serialize, Deserialize

Step 1.3: Test Phase 1 in isolation
What: Run cargo test and verify CPMM math
Risk: 🟢 None


🟡 Phase 2: Data Structure Updates (Careful!)
Step 2.1: Create PendingEvent struct (separate from existing)
What: New struct for events that haven't been launched
Risk: 🟡 Medium - need to migrate existing ai_events
Test: Ensure existing /ai/events still works

□ Add PendingEvent struct with:
  - id, title, category, options, confidence
  - source_url, source_domain
  - created_at, expires_at
  - status: EventStatus (always Pending)
□ Add app_state.pending_events: HashMap<String, PendingEvent>
□ Keep ai_events as-is for now (don't break existing)

Step 2.2: Update PredictionMarket struct
What: Add CPMM fields to existing market struct
Risk: 🟡 Medium - modifying existing struct
Test: Ensure /markets endpoint still works

□ Add optional fields (so existing code doesn't break):
  - cpmm_pool: Option<CPMMPool>
  - lp_shares: HashMap<String, f64>
  - tvl: f64
  - status: EventStatus
  - provisional_deadline: Option<u64>
  - betting_closes_at: Option<u64>
□ Default old markets to status: Active with no CPMM

Step 2.3: Test Phase 2
What: Run server, hit all existing endpoints
Risk: 🟢 Verify nothing broke

□ cargo run
□ Test GET /markets
□ Test GET /ai/events/feed.rss
□ Test POST /ai/events
□ Test GET /balance/:address

🟠 Phase 3: New Endpoints (Additive)
Step 3.1: Add GET /events/pending
What: List all pending (un-launched) events
Risk: 🟢 New endpoint, no changes to existing

□ Handler: list_pending_events()
□ Filter ai_events where status == Pending
□ Return JSON array

Step 3.2: Add POST /events/:id/launch
What: Launch a pending event as a market with liquidity
Risk: 🟡 Medium - creates markets, moves tokens

□ Handler: launch_event()
□ Validate: event exists, is pending, launcher has funds
□ Deduct tokens from launcher
□ Initialize CPMMPool with 50/50 split
□ Create PredictionMarket with status: Provisional
□ Set provisional_deadline = now + 72 hours
□ Grant 100% LP shares to launcher
□ Log blockchain activity

Step 3.3: Add POST /markets/:id/trade
What: CPMM trading endpoint
Risk: 🟡 Medium - moves tokens, updates pools

□ Handler: trade_cpmm()
□ Validate: market is active/provisional, trading open
□ Call cpmm_pool.swap()
□ Deduct cost from trader
□ Credit outcome tokens to trader
□ Add fees to LP pool
□ Log blockchain activity


Step 3.4: Test Phase 3
What: Full integration test of new endpoints

□ POST /ai/events (create pending event)
□ GET /events/pending (see it listed)
□ POST /events/:id/launch (launch with liquidity)
□ GET /markets/:id (see CPMM pool)
□ POST /markets/:id/trade (buy YES tokens)
□ GET /markets/:id/price (see price moved)


🔵 Phase 4: Lifecycle Management
Step 4.1: Add viability checker (background task)
What: Background job to check provisional markets
Risk: 🟡 Medium - async task, timing sensitive

□ tokio::spawn a loop that runs every hour
□ Find markets where status == Provisional
□ If deadline passed:
  - TVL >= 10,000 → promote to Active
  - TVL < 10,000 → refund all positions
□ Log state transitions


Step 4.2: Add POST /markets/:id/add-liquidity
What: Allow users to become LPs
Risk: 🟡 Medium - LP share math



□ Handler: add_liquidity()
□ Calculate proportional deposit
□ Mint LP shares
□ Update pool reserves

Step 4.3: Add POST /markets/:id/remove-liquidity
What: Allow LPs to exit
Risk: 🟡 Medium - LP share math

□ Handler: remove_liquidity()
□ Calculate proportional withdrawal
□ Burn LP shares
□ Return tokens

🟣 Phase 5: Resolution & Settlement
Step 5.1: Update POST /markets/:id/resolve
What: Set winning outcome
Risk: 🟡 Medium - existing endpoint, add new logic

□ Set market.winning_outcome
□ Set market.status = Resolved
□ Calculate winning token value (1.00 BB each)
□ Mark losing tokens as worthless

Step 5.2: Add POST /markets/:id/redeem
What: Redeem winning tokens for BB
Risk: 🟡 Medium - token transfers

□ Handler: redeem_tokens()
□ Validate user holds winning tokens
□ Exchange 1:1 for BB tokens
□ Burn redeemed outcome tokens

Step 5.3: Add LP settlement
What: Pay LPs after resolution
Risk: 🟡 Medium - complex calculations

□ Calculate remaining pool value
□ Distribute to LPs by share percentage
□ Handle impermanent loss scenarios

Suggested Order of Implementation


Order	Task	Risk	Est. Time
1	Step 1.1: Create src/cpmm.rs	🟢 None	1 hour
2	Step 1.2: Add EventStatus enum	🟢 None	15 min
3	Step 1.3: Test CPMM math	🟢 None	30 min
4	Step 2.1: Add PendingEvent struct	🟡 Low	30 min
5	Step 2.2: Update PredictionMarket	🟡 Medium	45 min
6	Step 2.3: Test existing endpoints	🟢 None	15 min
7	Step 3.1: GET /events/pending	🟢 None	20 min
8	Step 3.2: POST /events/:id/launch	🟡 Medium	1 hour
9	Step 3.3: POST /markets/:id/trade	🟡 Medium	1 hour
10	Step 3.4: Integration test	🟢 None	30 min


