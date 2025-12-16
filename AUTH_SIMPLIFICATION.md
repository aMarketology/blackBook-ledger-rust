# Authentication Simplification - JWT Removed

## Summary
Removed JWT authentication from BlackBook Layer 2. Authentication now relies **entirely on cryptographic signatures**.

## What Changed

### ✅ Removed
- ❌ Supabase JWT verification
- ❌ `POST /auth/login` (Supabase token-based)
- ❌ `GET /auth/user` (JWT in Authorization header)
- ❌ JWT dependency in auth flow

### ✅ Added
- ✨ `POST /auth/connect` - Simple wallet connection
- ✨ `POST /rpc/submit` - Alias for `/bet/signed` (SDK compatibility)
- ✨ Automatic wallet creation & funding (100 BB initial balance)

## New Authentication Flow

### Before (JWT-based):
```
1. User logs in with Supabase → Gets JWT token
2. Frontend sends JWT in Authorization header
3. L2 verifies JWT with Supabase
4. L2 maps JWT to wallet address
5. User signs transaction with private key (using salt and encyptet blob)
6. L1 verifies signature
7. Bet is placed
```

### After (Signature-only):
```
1. User connects wallet → L2 creates account if new
2. User signs transaction with private key
3. L2 verifies cryptographic signature
4. Wallet address extracted from signature
5. Bet is placed immediately
```

## API Changes

### POST /auth/connect
**Purpose:** Connect wallet and create account if new

**Request:**
```json
{
  "wallet_address": "L1_ABC123...",
  "username": "optional_username"  // Optional
}
```

**Response (New Wallet):**
```json
{
  "success": true,
  "wallet_address": "L1_ABC123...",
  "username": "user_ABC123",
  "balance": 100.0,
  "is_new_account": true,
  "message": "Account created and funded with 100 BB"
}
```

**Response (Existing Wallet):**
```json
{
  "success": true,
  "wallet_address": "L1_ABC123...",
  "username": "user_ABC123",
  "balance": 234.56,
  "is_new_account": false
}
```

### POST /bet/signed (Unchanged)
**Purpose:** Place bet with cryptographic signature

**Request:**
```json
{
  "signature": "0xHEX_SIGNATURE",
  "from_address": "L1_ABC123...",
  "market_id": "tesla_robotaxi_safety",
  "option": "YES",
  "amount": 50.0,
  "nonce": 1,
  "timestamp": 1234567890
}
```

### POST /rpc/submit (NEW - Alias)
**Purpose:** SDK compatibility - same as `/bet/signed`

This endpoint is an **alias** for `/bet/signed` to support existing client SDKs without requiring updates.

## Why Remove JWT?

1. **Redundant Security:** Cryptographic signatures already provide authentication
2. **Complexity:** JWT verification requires Supabase config, network calls, and token management
3. **Dependency:** Removes external dependency on Supabase
4. **Performance:** One less network call per request
5. **Simplicity:** Direct wallet-to-L2 communication

## Security Model

### Cryptographic Signature Verification
- Every bet transaction is **signed with the user's private key**
- L2 verifies the signature matches the `from_address`
- Only the owner of the private key can sign valid transactions
- Nonce prevents replay attacks

### Benefits
- ✅ **Decentralized:** No reliance on centralized auth provider
- ✅ **Trustless:** Math-based security, not trust-based
- ✅ **Industry Standard:** Used by all major blockchains
- ✅ **Simple:** One authentication mechanism instead of two

## Files Modified

### src/routes/auth.rs
- Removed: `login()` handler with JWT verification
- Removed: `get_user()` handler with JWT in header
- Added: `connect_wallet()` handler (no JWT)
- Removed: JWT imports and dependencies

### src/main.rs
- Changed: `POST /auth/login` → `POST /auth/connect`
- Removed: `GET /auth/user`
- Added: `POST /rpc/submit` alias for SDK compatibility
- Updated: Endpoint descriptions in console output

## Client SDK Integration

### Before
```javascript
// Login first with JWT
const loginResponse = await fetch('http://localhost:1234/auth/login', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ token: supabaseToken })
});

// Then place bet
const betResponse = await fetch('http://localhost:1234/bet/signed', {
  method: 'POST',
  body: JSON.stringify({ signature, from_address, ... })
});
```

### After
```javascript
// Optional: Connect wallet (creates account if new)
const connectResponse = await fetch('http://localhost:1234/auth/connect', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ wallet_address: 'L1_ABC123...' })
});

// Place bet directly (no JWT needed)
const betResponse = await fetch('http://localhost:1234/rpc/submit', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ signature, from_address, ... })
});
```

## Testing

### Test Wallet Connection
```bash
curl -X POST http://localhost:1234/auth/connect \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_address": "L1_TEST123",
    "username": "test_user"
  }'
```

### Test Bet Placement
```bash
curl -X POST http://localhost:1234/rpc/submit \
  -H "Content-Type: application/json" \
  -d '{
    "signature": "0xSIGNED_TX",
    "from_address": "L1_TEST123",
    "market_id": "tesla_robotaxi_safety",
    "option": "YES",
    "amount": 50.0,
    "nonce": 1,
    "timestamp": 1234567890
  }'
```

## Migration Notes

### For Existing Users
- **No action required** - Existing wallet addresses continue to work
- First bet after update will automatically work without JWT

### For New Users
- First transaction creates wallet automatically
- Receives 100 BB initial balance
- No signup/login required

## Next Steps

1. ✅ Test bet placement with client SDK
2. ⏳ Update frontend to remove JWT login flow
3. ⏳ Add GET /activity/:account endpoint for user transaction history
4. ⏳ Update documentation

## Related Files
- `src/routes/auth.rs` - Simplified authentication handlers
- `src/main.rs` - Route configuration
- `src/handlers.rs` - Bet placement handler (unchanged)
- `NEXT_STEPS.md` - Integration guide
