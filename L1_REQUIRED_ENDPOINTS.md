# L1 Required Endpoints for L2 Integration

This document lists the L1 blockchain endpoints that must be implemented for full L1↔L2 integration.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    L2 Prediction Market (Port 1234)             │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Supabase Auth Endpoints (IMPLEMENTED)                    │  │
│  │  • POST /auth/login    - Login with JWT                   │  │
│  │  • GET  /auth/user     - Get user info                    │  │
│  │  • POST /bet/auth      - Place authenticated bet          │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  L1 RPC Client (IMPLEMENTED)                              │  │
│  │  • verify_signature_sync() - Local ed25519 verification   │  │
│  │  • record_settlement_sync() - Settlement recording        │  │
│  │  • get_balance() - Query L1 balance                       │  │
│  └──────────────────────────────────────────────────────────┘  │
└──────────────────────────┬───────────────────────────────────────┘
                           │ HTTP Requests
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    L1 Blockchain (Port 8080)                    │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  REQUIRED ENDPOINTS (TO BE IMPLEMENTED)                   │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Required L1 Endpoints

### 1. **GET /auth/wallet/:user_id**

**Purpose**: L2 queries L1 to get the wallet address for a Supabase user

**Request**:
```
GET /auth/wallet/{user_id}
```

**Response**:
```json
{
  "success": true,
  "wallet_address": "L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA",
  "username": "Alice",
  "balance": 1000.0,
  "created_at": 1702345678
}
```

**Error Response**:
```json
{
  "success": false,
  "error": "User not found. Register on L1 first."
}
```

**Implementation Notes**:
- L1 stores mapping: `supabase_user_id → wallet_address`
- Created during L1 registration: `POST /auth/register`
- Used by L2's `/auth/login` endpoint

---

### 2. **POST /rpc/verify**

**Purpose**: Verify an ed25519 signature (for signed transactions)

**Request**:
```json
{
  "pubkey": "4013e5a935e9873a57879c471d5da83845ed5fc4d7bf4ce6dca53d51f30e7ad2",
  "message": "bet:market-123:outcome-0:amount-50",
  "signature": "abc123def456789..."
}
```

**Response**:
```json
{
  "valid": true,
  "pubkey": "4013e5a935e9873a57879c471d5da83845ed5fc4d7bf4ce6dca53d51f30e7ad2"
}
```

**Implementation Notes**:
- Uses `ed25519-dalek` crate for verification
- L2 uses this for `/bet/signed` endpoint
- Can be cached/optimized on L2 side

---

### 3. **GET /rpc/nonce/:address**

**Purpose**: Get current nonce for an address (replay protection)

**Request**:
```
GET /rpc/nonce/L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA
```

**Response**:
```json
{
  "address": "L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA",
  "current_nonce": 5,
  "next_valid_nonce": 6
}
```

**Implementation Notes**:
- L1 tracks nonces in `HashMap<String, u64>`
- Incremented after each signed transaction
- Used by L2 for replay protection

---

### 4. **POST /rpc/settlement**

**Purpose**: Record market settlement on L1 for audit trail

**Request**:
```json
{
  "market_id": "btc-100k-2025",
  "outcome": 0,
  "winners": [
    ["L1_48F58216...", 150.0],
    ["L1_6DD0DC4C...", 50.0]
  ],
  "l2_block_height": 12345,
  "l2_signature": "signed_by_l2_validator"
}
```

**Response**:
```json
{
  "recorded": true,
  "l1_tx_hash": "0xabc123def456...",
  "l1_slot": 98765,
  "error": null
}
```

**Implementation Notes**:
- Called by L2 after resolving a market
- Stores settlement proof on L1 for finality
- Can be queried later for verification

---

### 5. **GET /health**

**Purpose**: Health check for L1 blockchain

**Response**:
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "network": "BlackBook L1",
  "block_height": 12345,
  "timestamp": 1702345678
}
```

---

### 6. **GET /balance/:address**

**Purpose**: Get L1 balance (L2 can verify balances)

**Request**:
```
GET /balance/L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA
```

**Response**:
```json
{
  "address": "L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA",
  "balance": 1000.0,
  "token": "BB"
}
```

---

### 7. **GET /poh/status** (Optional)

**Purpose**: Get Proof of History status

**Response**:
```json
{
  "enabled": true,
  "current_slot": 12345,
  "epoch": 42,
  "leader": "validator-abc123"
}
```

---

## L1 Backend Implementation Checklist

### Core Infrastructure
- [ ] Add `supabase_users: HashMap<String, User>` to L1 AppState
- [ ] Add `wallet_to_user: HashMap<String, String>` mapping
- [ ] Add Supabase config to L1

### Authentication Endpoints
- [ ] `POST /auth/register` - Register Supabase user, create wallet
- [ ] `GET /auth/wallet/:user_id` - Get wallet by Supabase user ID

### RPC Endpoints  
- [ ] `POST /rpc/verify` - Verify ed25519 signature
- [ ] `GET /rpc/nonce/:address` - Get nonce for address
- [ ] `POST /rpc/settlement` - Record market settlement

### Existing Endpoints (Verify)
- [ ] `GET /balance/:address` - Get account balance
- [ ] `GET /health` - Health check
- [ ] `GET /poh/status` - Proof of History status (optional)

---

## Testing the Integration

### 1. Start L1 Blockchain
```bash
cd blackbook-l1
cargo run
# L1 running on http://localhost:8080
```

### 2. Start L2 Prediction Market
```bash
cd blackbook-ledger-rust
cargo run
# L2 running on http://localhost:1234
```

### 3. Test Connection
```javascript
import { BlackBookL2SDK } from './integration/blackbook-l2-prediction-sdk.js';

const sdk = new BlackBookL2SDK();
const connection = await sdk.checkL1L2Connection();
console.log(connection);
// Should show: { connected: true, ... }
```

### 4. Test Supabase Flow
```javascript
// User registers on L1
const registerRes = await fetch('http://localhost:8080/auth/register', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${supabaseJWT}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ username: 'Alice' })
});

// User logs in on L2 (L2 queries L1 for wallet)
const sdk = new BlackBookL2SDK();
await sdk.loginWithSupabase(supabaseJWT);

// User places bet on L2
await sdk.placeAuthenticatedBet('market-123', 0, 50);
```

---

## Environment Variables

### L1 (.env)
```bash
# Supabase
NEXT_PUBLIC_SUPABASE_URL=https://your-project.supabase.co
NEXT_PUBLIC_SUPABASE_ANON_KEY=your-anon-key

# L1 Server
L1_PORT=8080
```

### L2 (.env)
```bash
# L1 Connection
L1_RPC_URL=http://localhost:8080

# Supabase (for JWT verification)
NEXT_PUBLIC_SUPABASE_URL=https://your-project.supabase.co
NEXT_PUBLIC_SUPABASE_ANON_KEY=your-anon-key

# L2 Server
L2_PORT=1234
```

---

## Next Steps

1. **Implement L1 endpoints** (see checklist above)
2. **Test L1↔L2 connection** using `sdk.checkL1L2Connection()`
3. **Test Supabase auth flow** end-to-end
4. **Deploy both layers** to production

---

## Status

- ✅ **L2 Backend**: Fully implemented
  - Supabase auth endpoints
  - L1 RPC client
  - Bridge endpoints
  - Settlement recording
  
- ⏳ **L1 Backend**: Needs implementation
  - Auth endpoints for Supabase
  - RPC endpoints for L2 queries
  
- ✅ **SDK**: Fully implemented
  - L1 RPC integration methods
  - Supabase authentication
  - Test account support
