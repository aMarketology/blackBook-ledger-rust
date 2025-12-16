// ============================================================================
// ALICE & BOB COMPREHENSIVE L1 + L2 INTEGRATION TESTS
// ============================================================================
//
// Complete test suite testing Alice and Bob's wallets across the entire
// BlackBook ecosystem - L1 blockchain, L2 prediction market, and cross-layer
// operations (bridging, settlement, signatures).
//
// Run: node integration/alice-bob-comprehensive-tests.js
// Prerequisites: 
//   - L1 running on port 8080
//   - L2 running on port 1234
//
// ============================================================================

import nacl from 'tweetnacl';

// ============================================================================
// TEST ACCOUNTS - Alice & Bob
// ============================================================================

const ALICE = {
  name: "Alice",
  email: "alice@blackbook.test",
  username: "alice_test",
  publicKey: "4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57",
  privateKey: "616c6963655f746573745f6163636f756e745f76310000000000000000000001",
  l1Address: "L1ALICE000000001",
};

const BOB = {
  name: "Bob",
  email: "bob@blackbook.test",
  username: "bob_test",
  publicKey: "b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f",
  privateKey: "626f625f746573745f6163636f756e745f763100000000000000000000000002",
  l1Address: "L1BOB00000000001",
};

// ============================================================================
// CONFIGURATION
// ============================================================================

const L1_URL = process.env.L1_URL || 'http://localhost:8080';
const L2_URL = process.env.L2_URL || 'http://localhost:1234';
const VERBOSE = process.env.VERBOSE === 'true' || true;

// Test market ID (will be created during tests)
let TEST_MARKET_ID = null;

// Test results
const results = {
  passed: 0,
  failed: 0,
  skipped: 0,
  tests: []
};

// ============================================================================
// CRYPTO UTILITIES
// ============================================================================

function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return bytes;
}

function bytesToHex(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

function generateNonce() {
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return bytesToHex(array);
}

function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

function sign(privateKeyHex, message) {
  const seed = hexToBytes(privateKeyHex);
  const keyPair = nacl.sign.keyPair.fromSeed(seed);
  const messageBytes = new TextEncoder().encode(message);
  const signature = nacl.sign.detached(messageBytes, keyPair.secretKey);
  return bytesToHex(signature);
}

function verify(publicKeyHex, message, signatureHex) {
  const pubKey = hexToBytes(publicKeyHex);
  const signature = hexToBytes(signatureHex);
  const messageBytes = new TextEncoder().encode(message);
  return nacl.sign.detached.verify(messageBytes, signature, pubKey);
}

// ============================================================================
// HTTP HELPERS
// ============================================================================

async function get(url) {
  const response = await fetch(url);
  const text = await response.text();
  try {
    return { ok: response.ok, status: response.status, data: JSON.parse(text) };
  } catch {
    return { ok: response.ok, status: response.status, data: text };
  }
}

async function post(url, body) {
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  try {
    return { ok: response.ok, status: response.status, data: JSON.parse(text) };
  } catch {
    return { ok: response.ok, status: response.status, data: text };
  }
}

// ============================================================================
// TEST FRAMEWORK
// ============================================================================

function log(...args) {
  if (VERBOSE) console.log(...args);
}

async function test(name, fn) {
  const start = Date.now();
  try {
    await fn();
    const elapsed = Date.now() - start;
    results.passed++;
    results.tests.push({ name, status: 'PASS', elapsed });
    console.log(`✅ ${name} (${elapsed}ms)`);
  } catch (error) {
    const elapsed = Date.now() - start;
    results.failed++;
    results.tests.push({ name, status: 'FAIL', error: error.message, elapsed });
    console.log(`❌ ${name} - ${error.message}`);
    if (VERBOSE) console.error(error);
  }
}

function skip(name, reason) {
  results.skipped++;
  results.tests.push({ name, status: 'SKIP', reason });
  console.log(`⏭️  ${name} - SKIPPED: ${reason}`);
}

function assert(condition, message) {
  if (!condition) throw new Error(message || 'Assertion failed');
}

function assertEqual(actual, expected, message) {
  if (actual !== expected) {
    throw new Error(`${message || 'Not equal'}: expected ${expected}, got ${actual}`);
  }
}

function assertExists(value, message) {
  if (value === null || value === undefined) {
    throw new Error(message || 'Value is null/undefined');
  }
}

async function sleep(ms) {
  return new Promise(r => setTimeout(r, ms));
}

// ============================================================================
// SECTION 1: L1 HEALTH & STATUS
// ============================================================================

async function testL1Health() {
  const res = await get(`${L1_URL}/health`);
  assert(res.ok, 'L1 health check failed');
}

async function testL1Stats() {
  const res = await get(`${L1_URL}/stats`);
  assert(res.ok, 'L1 stats failed');
  assertExists(res.data, 'Stats data missing');
  log('  L1 Stats:', res.data);
}

// ============================================================================
// SECTION 2: L2 HEALTH & STATUS
// ============================================================================

async function testL2Health() {
  const res = await get(`${L2_URL}/health`);
  assert(res.ok, 'L2 health check failed');
}

async function testL2Stats() {
  const res = await get(`${L2_URL}/ledger/stats`);
  if (res.ok) {
    log('  L2 Ledger Stats:', res.data);
  }
}

// ============================================================================
// SECTION 3: ALICE L1 OPERATIONS
// ============================================================================

async function testAliceL1Balance() {
  const res = await get(`${L1_URL}/balance/${ALICE.l1Address}`);
  assert(res.ok, 'Failed to get Alice L1 balance');
  log(`  Alice L1 Balance: ${res.data.balance} BB`);
}

async function testAliceL1BalanceByPubKey() {
  const res = await get(`${L1_URL}/balance/${ALICE.publicKey}`);
  log(`  Alice balance by pubkey: ${res.data?.balance ?? 'N/A'}`);
}

async function testAliceL1Nonce() {
  const res = await get(`${L1_URL}/rpc/nonce/${ALICE.l1Address}`);
  if (res.ok) {
    log(`  Alice L1 Nonce: ${JSON.stringify(res.data)}`);
  }
}

async function testAliceSignatureVerification() {
  const message = `test_alice_${Date.now()}`;
  const signature = sign(ALICE.privateKey, message);
  
  // Verify locally first
  const localValid = verify(ALICE.publicKey, message, signature);
  assert(localValid, 'Local signature verification failed');
  
  // Verify via L1 RPC
  const res = await post(`${L1_URL}/rpc/verify-signature`, {
    public_key: ALICE.publicKey,
    message: message,
    signature: signature,
  });
  
  log(`  Signature verification: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 4: BOB L1 OPERATIONS
// ============================================================================

async function testBobL1Balance() {
  const res = await get(`${L1_URL}/balance/${BOB.l1Address}`);
  assert(res.ok, 'Failed to get Bob L1 balance');
  log(`  Bob L1 Balance: ${res.data.balance} BB`);
}

async function testBobSignatureVerification() {
  const message = `test_bob_${Date.now()}`;
  const signature = sign(BOB.privateKey, message);
  
  const localValid = verify(BOB.publicKey, message, signature);
  assert(localValid, 'Bob local signature verification failed');
  
  log(`  Bob signature verified locally`);
}

// ============================================================================
// SECTION 5: ALICE L2 OPERATIONS
// ============================================================================

async function testAliceL2Connect() {
  const res = await post(`${L2_URL}/auth/connect`, {
    address: ALICE.publicKey,
    public_key: ALICE.publicKey,
    timestamp: getTimestamp(),
  });
  
  if (!res.ok) {
    // Try deposit endpoint to create account
    const depositRes = await post(`${L2_URL}/deposit`, {
      address: ALICE.publicKey,
      amount: 0,
    });
    log(`  Alice L2 account created via deposit: ${depositRes.ok}`);
  } else {
    log(`  Alice L2 connected: ${JSON.stringify(res.data)}`);
  }
}

async function testAliceL2Balance() {
  const res = await get(`${L2_URL}/balance/${ALICE.publicKey}`);
  if (res.ok) {
    log(`  Alice L2 Balance: ${res.data.balance} BB`);
  } else {
    log(`  Alice L2 Balance: Not found (needs deposit)`);
  }
}

async function testAliceL2Nonce() {
  const res = await get(`${L2_URL}/rpc/nonce/${ALICE.publicKey}`);
  log(`  Alice L2 Nonce: ${JSON.stringify(res.data)}`);
}

async function testAliceL2Deposit() {
  // Admin mint for testing
  const res = await post(`${L2_URL}/admin/mint`, {
    address: ALICE.publicKey,
    amount: 10000,
  });
  
  if (res.ok) {
    log(`  Alice minted 10000 BB on L2`);
  } else {
    log(`  Alice mint result: ${JSON.stringify(res.data)}`);
  }
}

// ============================================================================
// SECTION 6: BOB L2 OPERATIONS
// ============================================================================

async function testBobL2Connect() {
  const res = await post(`${L2_URL}/auth/connect`, {
    address: BOB.publicKey,
    public_key: BOB.publicKey,
    timestamp: getTimestamp(),
  });
  
  if (!res.ok) {
    const depositRes = await post(`${L2_URL}/deposit`, {
      address: BOB.publicKey,
      amount: 0,
    });
    log(`  Bob L2 account created via deposit: ${depositRes.ok}`);
  } else {
    log(`  Bob L2 connected: ${JSON.stringify(res.data)}`);
  }
}

async function testBobL2Balance() {
  const res = await get(`${L2_URL}/balance/${BOB.publicKey}`);
  if (res.ok) {
    log(`  Bob L2 Balance: ${res.data.balance} BB`);
  } else {
    log(`  Bob L2 Balance: Not found (needs deposit)`);
  }
}

async function testBobL2Deposit() {
  const res = await post(`${L2_URL}/admin/mint`, {
    address: BOB.publicKey,
    amount: 10000,
  });
  
  if (res.ok) {
    log(`  Bob minted 10000 BB on L2`);
  } else {
    log(`  Bob mint result: ${JSON.stringify(res.data)}`);
  }
}

// ============================================================================
// SECTION 7: MARKET OPERATIONS
// ============================================================================

async function testCreateMarket() {
  const market = {
    title: `Alice vs Bob Test Market ${Date.now()}`,
    description: "Test market for Alice & Bob integration testing",
    category: "test",
    options: ["Alice Wins", "Bob Wins"],
    end_time: getTimestamp() + 86400, // 24 hours
    creator: ALICE.publicKey,
  };
  
  const res = await post(`${L2_URL}/markets`, market);
  
  if (res.ok && res.data.id) {
    TEST_MARKET_ID = res.data.id;
    log(`  Market created: ${TEST_MARKET_ID}`);
  } else if (res.data?.market_id) {
    TEST_MARKET_ID = res.data.market_id;
    log(`  Market created: ${TEST_MARKET_ID}`);
  } else {
    log(`  Market creation response: ${JSON.stringify(res.data)}`);
  }
}

async function testGetMarkets() {
  const res = await get(`${L2_URL}/markets`);
  assert(res.ok, 'Failed to get markets');
  
  const markets = res.data.markets || res.data || [];
  log(`  Found ${Array.isArray(markets) ? markets.length : 'N/A'} markets`);
  
  // Try to find a market to use for testing
  if (!TEST_MARKET_ID && Array.isArray(markets) && markets.length > 0) {
    TEST_MARKET_ID = markets[0].id || Object.keys(markets)[0];
    log(`  Using existing market: ${TEST_MARKET_ID}`);
  }
}

async function testGetMarketDetails() {
  if (!TEST_MARKET_ID) {
    skip('testGetMarketDetails', 'No market ID available');
    return;
  }
  
  const res = await get(`${L2_URL}/markets/${TEST_MARKET_ID}`);
  log(`  Market details: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 8: BETTING - ALICE BETS
// ============================================================================

async function testAlicePlaceBet() {
  if (!TEST_MARKET_ID) {
    skip('testAlicePlaceBet', 'No market ID available');
    return;
  }
  
  const nonceRes = await get(`${L2_URL}/rpc/nonce/${ALICE.publicKey}`);
  const currentNonce = typeof nonceRes.data === 'number' ? nonceRes.data : (nonceRes.data?.nonce || 0);
  const nonce = currentNonce + 1;
  const timestamp = getTimestamp();
  const amount = 100;
  const outcome = 0; // "Alice Wins"
  
  const message = `bet:${ALICE.publicKey}:${TEST_MARKET_ID}:${outcome}:${amount}:${timestamp}:${nonce}`;
  const signature = sign(ALICE.privateKey, message);
  
  const bet = {
    signature,
    public_key: ALICE.publicKey,
    from_address: ALICE.publicKey,
    market_id: TEST_MARKET_ID,
    option: String(outcome),
    amount: amount,
    nonce,
    timestamp,
    payload: message,
  };
  
  const res = await post(`${L2_URL}/bet/signed`, bet);
  log(`  Alice bet result: ${JSON.stringify(res.data)}`);
}

async function testAliceGetBets() {
  const res = await get(`${L2_URL}/bets/${ALICE.publicKey}`);
  log(`  Alice bets: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 9: BETTING - BOB BETS
// ============================================================================

async function testBobPlaceBet() {
  if (!TEST_MARKET_ID) {
    skip('testBobPlaceBet', 'No market ID available');
    return;
  }
  
  const nonceRes = await get(`${L2_URL}/rpc/nonce/${BOB.publicKey}`);
  const currentNonce = typeof nonceRes.data === 'number' ? nonceRes.data : (nonceRes.data?.nonce || 0);
  const nonce = currentNonce + 1;
  const timestamp = getTimestamp();
  const amount = 150;
  const outcome = 1; // "Bob Wins"
  
  const message = `bet:${BOB.publicKey}:${TEST_MARKET_ID}:${outcome}:${amount}:${timestamp}:${nonce}`;
  const signature = sign(BOB.privateKey, message);
  
  const bet = {
    signature,
    public_key: BOB.publicKey,
    from_address: BOB.publicKey,
    market_id: TEST_MARKET_ID,
    option: String(outcome),
    amount: amount,
    nonce,
    timestamp,
    payload: message,
  };
  
  const res = await post(`${L2_URL}/bet/signed`, bet);
  log(`  Bob bet result: ${JSON.stringify(res.data)}`);
}

async function testBobGetBets() {
  const res = await get(`${L2_URL}/bets/${BOB.publicKey}`);
  log(`  Bob bets: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 10: SHARES SYSTEM
// ============================================================================

async function testAliceMintShares() {
  if (!TEST_MARKET_ID) {
    skip('testAliceMintShares', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/shares/mint`, {
    wallet: ALICE.publicKey,
    market_id: TEST_MARKET_ID,
    amount: 50,
  });
  
  log(`  Alice mint shares: ${JSON.stringify(res.data)}`);
}

async function testAliceSharePosition() {
  if (!TEST_MARKET_ID) {
    skip('testAliceSharePosition', 'No market ID available');
    return;
  }
  
  const res = await get(`${L2_URL}/shares/position/${ALICE.publicKey}/${TEST_MARKET_ID}`);
  log(`  Alice share position: ${JSON.stringify(res.data)}`);
}

async function testBobMintShares() {
  if (!TEST_MARKET_ID) {
    skip('testBobMintShares', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/shares/mint`, {
    wallet: BOB.publicKey,
    market_id: TEST_MARKET_ID,
    amount: 75,
  });
  
  log(`  Bob mint shares: ${JSON.stringify(res.data)}`);
}

async function testBobSharePosition() {
  if (!TEST_MARKET_ID) {
    skip('testBobSharePosition', 'No market ID available');
    return;
  }
  
  const res = await get(`${L2_URL}/shares/position/${BOB.publicKey}/${TEST_MARKET_ID}`);
  log(`  Bob share position: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 11: ORDERBOOK (CLOB)
// ============================================================================

async function testAlicePlaceLimitOrder() {
  if (!TEST_MARKET_ID) {
    skip('testAlicePlaceLimitOrder', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/orderbook/order`, {
    wallet: ALICE.publicKey,
    market_id: TEST_MARKET_ID,
    side: 'buy',
    outcome: 'yes',
    price: 0.65,
    quantity: 25,
  });
  
  log(`  Alice limit order: ${JSON.stringify(res.data)}`);
}

async function testBobPlaceLimitOrder() {
  if (!TEST_MARKET_ID) {
    skip('testBobPlaceLimitOrder', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/orderbook/order`, {
    wallet: BOB.publicKey,
    market_id: TEST_MARKET_ID,
    side: 'sell',
    outcome: 'yes',
    price: 0.70,
    quantity: 20,
  });
  
  log(`  Bob limit order: ${JSON.stringify(res.data)}`);
}

async function testGetOrderbook() {
  if (!TEST_MARKET_ID) {
    skip('testGetOrderbook', 'No market ID available');
    return;
  }
  
  const res = await get(`${L2_URL}/orderbook/${TEST_MARKET_ID}`);
  log(`  Orderbook: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 12: ORACLE & RESOLUTION
// ============================================================================

async function testListOracles() {
  const res = await get(`${L2_URL}/oracles`);
  log(`  Oracles: ${JSON.stringify(res.data)}`);
}

async function testAddAliceAsOracle() {
  // Use admin endpoint (Alice needs to be admin)
  const res = await post(`${L2_URL}/oracles/add`, {
    admin_address: ALICE.publicKey,
    oracle_address: ALICE.publicKey,
  });
  
  log(`  Add Alice as oracle: ${JSON.stringify(res.data)}`);
}

async function testMarketResolution() {
  if (!TEST_MARKET_ID) {
    skip('testMarketResolution', 'No market ID available');
    return;
  }
  
  // Alice resolves market (she wins!)
  const nonceRes = await get(`${L2_URL}/rpc/nonce/${ALICE.publicKey}`);
  const currentNonce = typeof nonceRes.data === 'number' ? nonceRes.data : (nonceRes.data?.nonce || 0);
  const nonce = currentNonce + 1;
  const timestamp = getTimestamp();
  const winningOutcome = 0; // "Alice Wins"
  
  const message = `resolve:${TEST_MARKET_ID}:${winningOutcome}:${timestamp}:${nonce}`;
  const signature = sign(ALICE.privateKey, message);
  
  const res = await post(`${L2_URL}/markets/${TEST_MARKET_ID}/resolve`, {
    resolver_address: ALICE.publicKey,
    winning_outcome: winningOutcome,
    resolution_reason: "Test resolution - Alice wins!",
    timestamp,
    nonce,
    signature,
  });
  
  log(`  Resolution result: ${JSON.stringify(res.data)}`);
}

async function testAdminResolveMarket() {
  if (!TEST_MARKET_ID) {
    skip('testAdminResolveMarket', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/admin/resolve/${TEST_MARKET_ID}/0`, {
    admin_address: ALICE.publicKey,
  });
  
  log(`  Admin resolve: ${JSON.stringify(res.data)}`);
}

async function testGetResolution() {
  if (!TEST_MARKET_ID) {
    skip('testGetResolution', 'No market ID available');
    return;
  }
  
  const res = await get(`${L2_URL}/markets/${TEST_MARKET_ID}/resolution`);
  log(`  Resolution details: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 13: CLAIM WINNINGS
// ============================================================================

async function testAliceClaimWinnings() {
  if (!TEST_MARKET_ID) {
    skip('testAliceClaimWinnings', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/shares/claim/${TEST_MARKET_ID}`, {
    wallet: ALICE.publicKey,
  });
  
  log(`  Alice claim: ${JSON.stringify(res.data)}`);
}

async function testBobClaimWinnings() {
  if (!TEST_MARKET_ID) {
    skip('testBobClaimWinnings', 'No market ID available');
    return;
  }
  
  const res = await post(`${L2_URL}/shares/claim/${TEST_MARKET_ID}`, {
    wallet: BOB.publicKey,
  });
  
  log(`  Bob claim: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 14: L1 SETTLEMENT
// ============================================================================

async function testSettleToL1() {
  const res = await post(`${L2_URL}/settle/l1`, {});
  log(`  Settlement to L1: ${JSON.stringify(res.data)}`);
}

async function testGetPendingSettlements() {
  const res = await get(`${L2_URL}/settle/pending`);
  log(`  Pending settlements: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 15: BRIDGE OPERATIONS
// ============================================================================

async function testBridgeStats() {
  const res = await get(`${L2_URL}/bridge/stats`);
  log(`  Bridge stats: ${JSON.stringify(res.data)}`);
}

async function testAliceBridgeWithdraw() {
  const nonceRes = await get(`${L2_URL}/rpc/nonce/${ALICE.publicKey}`);
  const currentNonce = typeof nonceRes.data === 'number' ? nonceRes.data : (nonceRes.data?.nonce || 0);
  const nonce = currentNonce + 1;
  const timestamp = getTimestamp();
  const amount = 50;
  
  const message = `withdraw:${ALICE.publicKey}:${ALICE.publicKey}:${amount}:${timestamp}:${nonce}`;
  const signature = sign(ALICE.privateKey, message);
  
  const res = await post(`${L2_URL}/bridge/withdraw`, {
    wallet: ALICE.publicKey,
    target_address: ALICE.publicKey,
    amount: amount,
    nonce,
    timestamp,
    signature,
  });
  
  log(`  Alice bridge withdraw: ${JSON.stringify(res.data)}`);
}

async function testAliceWalletBridges() {
  const res = await get(`${L2_URL}/bridge/wallet/${ALICE.publicKey}`);
  log(`  Alice bridges: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 16: CROSS-LAYER TRANSFER TEST
// ============================================================================

async function testAliceToBobL2Transfer() {
  const res = await post(`${L2_URL}/transfer`, {
    from: ALICE.publicKey,
    to: BOB.publicKey,
    amount: 25,
  });
  
  log(`  Alice→Bob L2 transfer: ${JSON.stringify(res.data)}`);
}

async function testBobToAliceL2Transfer() {
  const res = await post(`${L2_URL}/transfer`, {
    from: BOB.publicKey,
    to: ALICE.publicKey,
    amount: 10,
  });
  
  log(`  Bob→Alice L2 transfer: ${JSON.stringify(res.data)}`);
}

// ============================================================================
// SECTION 17: FINAL BALANCE CHECK
// ============================================================================

async function testFinalBalances() {
  const [aliceL1, bobL1, aliceL2, bobL2] = await Promise.all([
    get(`${L1_URL}/balance/${ALICE.l1Address}`),
    get(`${L1_URL}/balance/${BOB.l1Address}`),
    get(`${L2_URL}/balance/${ALICE.publicKey}`),
    get(`${L2_URL}/balance/${BOB.publicKey}`),
  ]);
  
  console.log('\n📊 FINAL BALANCES:');
  console.log(`   Alice L1: ${aliceL1.data?.balance ?? 'N/A'} BB`);
  console.log(`   Alice L2: ${aliceL2.data?.balance ?? 'N/A'} BB`);
  console.log(`   Bob L1: ${bobL1.data?.balance ?? 'N/A'} BB`);
  console.log(`   Bob L2: ${bobL2.data?.balance ?? 'N/A'} BB`);
}

async function testTransactionHistory() {
  const aliceTxRes = await get(`${L2_URL}/transactions/${ALICE.publicKey}`);
  const bobTxRes = await get(`${L2_URL}/transactions/${BOB.publicKey}`);
  
  log(`  Alice transactions: ${JSON.stringify(aliceTxRes.data)}`);
  log(`  Bob transactions: ${JSON.stringify(bobTxRes.data)}`);
}

// ============================================================================
// MAIN TEST RUNNER
// ============================================================================

async function runAllTests() {
  console.log('═'.repeat(70));
  console.log('  ALICE & BOB COMPREHENSIVE L1 + L2 INTEGRATION TESTS');
  console.log('═'.repeat(70));
  console.log(`  L1 URL: ${L1_URL}`);
  console.log(`  L2 URL: ${L2_URL}`);
  console.log(`  Alice: ${ALICE.publicKey.slice(0, 16)}...`);
  console.log(`  Bob: ${BOB.publicKey.slice(0, 16)}...`);
  console.log('═'.repeat(70));
  console.log();

  // SECTION 1: Health
  console.log('━━━ SECTION 1: L1 HEALTH ━━━');
  await test('L1 Health Check', testL1Health);
  await test('L1 Stats', testL1Stats);
  console.log();

  // SECTION 2: L2 Health
  console.log('━━━ SECTION 2: L2 HEALTH ━━━');
  await test('L2 Health Check', testL2Health);
  await test('L2 Stats', testL2Stats);
  console.log();

  // SECTION 3: Alice L1
  console.log('━━━ SECTION 3: ALICE L1 OPERATIONS ━━━');
  await test('Alice L1 Balance', testAliceL1Balance);
  await test('Alice L1 Balance (by pubkey)', testAliceL1BalanceByPubKey);
  await test('Alice L1 Nonce', testAliceL1Nonce);
  await test('Alice Signature Verification', testAliceSignatureVerification);
  console.log();

  // SECTION 4: Bob L1
  console.log('━━━ SECTION 4: BOB L1 OPERATIONS ━━━');
  await test('Bob L1 Balance', testBobL1Balance);
  await test('Bob Signature Verification', testBobSignatureVerification);
  console.log();

  // SECTION 5: Alice L2
  console.log('━━━ SECTION 5: ALICE L2 OPERATIONS ━━━');
  await test('Alice L2 Connect', testAliceL2Connect);
  await test('Alice L2 Deposit (Mint)', testAliceL2Deposit);
  await test('Alice L2 Balance', testAliceL2Balance);
  await test('Alice L2 Nonce', testAliceL2Nonce);
  console.log();

  // SECTION 6: Bob L2
  console.log('━━━ SECTION 6: BOB L2 OPERATIONS ━━━');
  await test('Bob L2 Connect', testBobL2Connect);
  await test('Bob L2 Deposit (Mint)', testBobL2Deposit);
  await test('Bob L2 Balance', testBobL2Balance);
  console.log();

  // SECTION 7: Markets
  console.log('━━━ SECTION 7: MARKET OPERATIONS ━━━');
  await test('Get Markets', testGetMarkets);
  await test('Create Market', testCreateMarket);
  await test('Get Market Details', testGetMarketDetails);
  console.log();

  // SECTION 8: Alice Betting
  console.log('━━━ SECTION 8: ALICE BETTING ━━━');
  await test('Alice Place Bet', testAlicePlaceBet);
  await test('Alice Get Bets', testAliceGetBets);
  console.log();

  // SECTION 9: Bob Betting
  console.log('━━━ SECTION 9: BOB BETTING ━━━');
  await test('Bob Place Bet', testBobPlaceBet);
  await test('Bob Get Bets', testBobGetBets);
  console.log();

  // SECTION 10: Shares
  console.log('━━━ SECTION 10: SHARES SYSTEM ━━━');
  await test('Alice Mint Shares', testAliceMintShares);
  await test('Alice Share Position', testAliceSharePosition);
  await test('Bob Mint Shares', testBobMintShares);
  await test('Bob Share Position', testBobSharePosition);
  console.log();

  // SECTION 11: Orderbook
  console.log('━━━ SECTION 11: ORDERBOOK (CLOB) ━━━');
  await test('Alice Place Limit Order', testAlicePlaceLimitOrder);
  await test('Bob Place Limit Order', testBobPlaceLimitOrder);
  await test('Get Orderbook', testGetOrderbook);
  console.log();

  // SECTION 12: Resolution
  console.log('━━━ SECTION 12: ORACLE & RESOLUTION ━━━');
  await test('List Oracles', testListOracles);
  await test('Add Alice as Oracle', testAddAliceAsOracle);
  await test('Market Resolution (Alice as Oracle)', testMarketResolution);
  await test('Admin Resolve Market', testAdminResolveMarket);
  await test('Get Resolution Details', testGetResolution);
  console.log();

  // SECTION 13: Claim
  console.log('━━━ SECTION 13: CLAIM WINNINGS ━━━');
  await test('Alice Claim Winnings', testAliceClaimWinnings);
  await test('Bob Claim Winnings', testBobClaimWinnings);
  console.log();

  // SECTION 14: Settlement
  console.log('━━━ SECTION 14: L1 SETTLEMENT ━━━');
  await test('Settle to L1', testSettleToL1);
  await test('Get Pending Settlements', testGetPendingSettlements);
  console.log();

  // SECTION 15: Bridge
  console.log('━━━ SECTION 15: BRIDGE OPERATIONS ━━━');
  await test('Bridge Stats', testBridgeStats);
  await test('Alice Bridge Withdraw', testAliceBridgeWithdraw);
  await test('Alice Wallet Bridges', testAliceWalletBridges);
  console.log();

  // SECTION 16: Cross-Layer
  console.log('━━━ SECTION 16: CROSS-LAYER TRANSFERS ━━━');
  await test('Alice → Bob L2 Transfer', testAliceToBobL2Transfer);
  await test('Bob → Alice L2 Transfer', testBobToAliceL2Transfer);
  console.log();

  // SECTION 17: Final Check
  console.log('━━━ SECTION 17: FINAL STATE ━━━');
  await test('Final Balances', testFinalBalances);
  await test('Transaction History', testTransactionHistory);
  console.log();

  // Summary
  console.log('═'.repeat(70));
  console.log('  TEST RESULTS SUMMARY');
  console.log('═'.repeat(70));
  console.log(`  ✅ Passed:  ${results.passed}`);
  console.log(`  ❌ Failed:  ${results.failed}`);
  console.log(`  ⏭️  Skipped: ${results.skipped}`);
  console.log(`  📊 Total:   ${results.passed + results.failed + results.skipped}`);
  console.log('═'.repeat(70));

  // List failed tests
  if (results.failed > 0) {
    console.log('\n❌ FAILED TESTS:');
    results.tests
      .filter(t => t.status === 'FAIL')
      .forEach(t => console.log(`   - ${t.name}: ${t.error}`));
  }

  // Exit with appropriate code
  process.exit(results.failed > 0 ? 1 : 0);
}

// Run
runAllTests().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
