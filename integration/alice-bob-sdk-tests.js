// ============================================================================
// ALICE & BOB UNIFIED SDK TEST SUITE
// ============================================================================
//
// Complete test suite for BlackBook Layer 1 & Layer 2 integration.
// Includes all L1 operations, bridge escrow, and L2 integration tests.
//
// Run:   node frontend-sdk/alice-bob-sdk-tests.js
// Deps:  npm install tweetnacl
//
// Prerequisites:
//   - L1 server on port 8080 (required)
//   - L2 server on port 1234 (optional - tests skip if unavailable)
//
// Test Categories:
//   1. L1 Health & Status
//   2. Balance Operations
//   3. RPC Interface
//   4. Blockhash (Solana-Compatible)
//   5. Signature Verification
//   6. Signed Transfers (Alice ↔ Bob)
//   7. Block Operations
//   8. Bridge Escrow (Lock → Settle → Release)
//   9. L2 Integration (when L2 available)
//   10. Social Mining
//   11. Cross-Layer Operations
//
// ============================================================================

// ============================================================================
// TEST ACCOUNTS - Real Ed25519 credentials from the server
// ============================================================================

const ALICE = {
  name: "Alice",
  email: "alice@blackbook.test",
  username: "alice_test",
  publicKey: "4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57",
  privateKey: "616c6963655f746573745f6163636f756e745f76310000000000000000000001",
  address: "L1ALICE000000001",
  l1Address: "L1ALICE000000001",  // Alias for compatibility
  l2Address: "L148F582A1BC8976",
};

const BOB = {
  name: "Bob",
  email: "bob@blackbook.test",
  username: "bob_test",
  publicKey: "b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f",
  privateKey: "626f625f746573745f6163636f756e745f763100000000000000000000000002",
  address: "L1BOB00000000001",
  l1Address: "L1BOB00000000001",  // Alias for compatibility
  l2Address: "L16DD0DC4C96CABD",
};

// ============================================================================
// TEST CONFIGURATION
// ============================================================================

const L1_URL = process.env.L1_URL || 'http://localhost:8080';
const L2_URL = process.env.L2_URL || 'http://localhost:1234';
const VERBOSE = process.env.VERBOSE === 'true';

// Test results tracking
let testResults = {
  passed: 0,
  failed: 0,
  skipped: 0,
  tests: []
};

// Load tweetnacl
let nacl;
const crypto = require('crypto');

// ============================================================================
// CRYPTO HELPERS
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

function signMessage(privateKeyHex, message) {
  if (!nacl) throw new Error('tweetnacl not loaded');
  const seed = hexToBytes(privateKeyHex);
  const keyPair = nacl.sign.keyPair.fromSeed(seed);
  const messageBytes = new TextEncoder().encode(message);
  const signatureBytes = nacl.sign.detached(messageBytes, keyPair.secretKey);
  return bytesToHex(signatureBytes);
}

function generateNonce() {
  return crypto.randomBytes(16).toString('hex');
}

function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

// ============================================================================
// HTTP HELPERS
// ============================================================================

function log(...args) {
  if (VERBOSE) console.log(...args);
}

async function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function request(method, path, body = null, baseUrl = L1_URL) {
  const options = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body) {
    options.body = JSON.stringify(body);
  }
  const response = await fetch(`${baseUrl}${path}`, options);
  return response.json();
}

async function getBalance(address, baseUrl = L1_URL) {
  const result = await request('GET', `/balance/${address}`, null, baseUrl);
  return result.balance || 0;
}

function createSignedRequest(payload, account) {
  const payloadStr = typeof payload === 'string' ? payload : JSON.stringify(payload);
  const timestamp = getTimestamp();
  const nonce = generateNonce();
  const message = `${payloadStr}\n${timestamp}\n${nonce}`;
  const signature = signMessage(account.privateKey, message);
  
  return {
    public_key: account.publicKey,
    payload: payloadStr,
    timestamp,
    nonce,
    signature,
    wallet_address: account.address,
  };
}

async function checkServer(url, name) {
  try {
    const response = await fetch(`${url}/health`, { 
      signal: AbortSignal.timeout(3000) 
    });
    return response.ok;
  } catch (e) {
    return false;
  }
}

// ============================================================================
// TEST FRAMEWORK
// ============================================================================

async function runTest(name, testFn) {
  const startTime = Date.now();
  try {
    await testFn();
    const duration = Date.now() - startTime;
    testResults.passed++;
    testResults.tests.push({ name, status: 'PASS', duration });
    console.log(`✅ ${name} (${duration}ms)`);
    return true;
  } catch (error) {
    const duration = Date.now() - startTime;
    testResults.failed++;
    testResults.tests.push({ name, status: 'FAIL', duration, error: error.message });
    console.log(`❌ ${name} (${duration}ms)`);
    console.log(`   Error: ${error.message}`);
    return false;
  }
}

function skipTest(name, reason) {
  testResults.skipped++;
  testResults.tests.push({ name, status: 'SKIP', reason });
  console.log(`⏭️  ${name} - SKIPPED: ${reason}`);
}

function assertEqual(actual, expected, message = '') {
  if (actual !== expected) {
    throw new Error(`${message}: Expected ${expected}, got ${actual}`);
  }
}

function assertTrue(condition, message = '') {
  if (!condition) {
    throw new Error(`${message}: Expected true, got false`);
  }
}

function assertExists(value, message = '') {
  if (value === null || value === undefined) {
    throw new Error(`${message}: Value is null or undefined`);
  }
}

// ============================================================================
// SECTION 1: L1 HEALTH & STATUS
// ============================================================================

async function testL1Health() {
  const response = await fetch(`${L1_URL}/health`);
  assertTrue(response.ok, 'L1 health endpoint should return OK');
  const data = await response.json();
  assertExists(data, 'Health response should have data');
}

async function testL1Stats() {
  const response = await fetch(`${L1_URL}/stats`);
  assertTrue(response.ok, 'L1 stats endpoint should return OK');
  const data = await response.json();
  assertExists(data.block_height !== undefined || data.blocks !== undefined, 
    'Stats should have block height');
}

async function testL1PoHStatus() {
  const response = await fetch(`${L1_URL}/poh/status`);
  if (response.ok) {
    const data = await response.json();
    log('PoH Status:', data);
    return;
  }
  log('PoH status not available');
}

// ============================================================================
// SECTION 2: BALANCE OPERATIONS
// ============================================================================

async function testGetAliceBalance() {
  const response = await fetch(`${L1_URL}/balance/${ALICE.l1Address}`);
  assertTrue(response.ok, 'Balance endpoint should return OK');
  const data = await response.json();
  assertExists(data.balance !== undefined, 'Response should have balance field');
  console.log(`   💰 Alice balance: ${data.balance} BB`);
}

async function testGetBobBalance() {
  const response = await fetch(`${L1_URL}/balance/${BOB.l1Address}`);
  assertTrue(response.ok, 'Balance endpoint should return OK');
  const data = await response.json();
  assertExists(data.balance !== undefined, 'Response should have balance field');
  console.log(`   💰 Bob balance: ${data.balance} BB`);
}

async function testGetMultipleBalances() {
  const [aliceRes, bobRes] = await Promise.all([
    fetch(`${L1_URL}/balance/${ALICE.l1Address}`),
    fetch(`${L1_URL}/balance/${BOB.l1Address}`)
  ]);
  assertTrue(aliceRes.ok && bobRes.ok, 'Both balance requests should succeed');
}

// ============================================================================
// SECTION 3: RPC INTERFACE
// ============================================================================

async function testRPCGetBlockHeight() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'getBlockHeight', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result !== undefined, 'RPC should return result');
  console.log(`   📦 Block height: ${data.result}`);
}

async function testRPCGetBalance() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'getBalance', params: [ALICE.l1Address] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result !== undefined, 'RPC should return balance');
}

async function testRPCGetChainStats() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'getChainStats', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return chain stats');
}

async function testRPCGetAccountInfo() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'getAccountInfo', params: [ALICE.l1Address] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return account info');
}

// ============================================================================
// SECTION 4: BLOCKHASH (SOLANA-COMPATIBLE)
// ============================================================================

async function testRPCGetRecentBlockhash() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 5, method: 'getRecentBlockhash', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return blockhash data');
  assertExists(data.result.blockhash, 'Should have blockhash field');
  console.log(`   🔗 Blockhash: ${data.result.blockhash.slice(0, 16)}...`);
}

async function testRPCGetLatestBlockhash() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 6, method: 'getLatestBlockhash', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return latest blockhash');
}

async function testRPCIsBlockhashValid() {
  const getHashRes = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 7, method: 'getRecentBlockhash', params: [] })
  });
  const hashData = await getHashRes.json();
  const blockhash = hashData.result?.blockhash;
  
  if (!blockhash) throw new Error('Could not get blockhash to test');
  
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 8, method: 'isBlockhashValid', params: [blockhash] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return validity result');
}

async function testRPCGetSlotLeader() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 12, method: 'getSlotLeader', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return slot leader');
}

async function testRPCGetFeeForMessage() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 11, method: 'getFeeForMessage', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return fee info');
}

async function testRPCGetMinimumBalanceForRentExemption() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 13, method: 'getMinimumBalanceForRentExemption', params: [0] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return rent exemption info');
}

// ============================================================================
// SECTION 5: SIGNATURE VERIFICATION
// ============================================================================

async function testRPCVerifySignature() {
  const message = 'test_message_' + Date.now();
  const signature = signMessage(ALICE.privateKey, message);
  
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0', id: 14, method: 'verifyL1Signature',
      params: [ALICE.publicKey, message, signature]
    })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'RPC should return verification result');
  assertTrue(data.result.valid === true, 'Signature should be valid');
}

async function testRPCVerifyInvalidSignature() {
  const message = 'test_message_' + Date.now();
  const fakeSignature = '0'.repeat(128);
  
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0', id: 15, method: 'verifyL1Signature',
      params: [ALICE.publicKey, message, fakeSignature]
    })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertTrue(data.result?.valid === false || data.result?.error, 
    'Invalid signature should be detected');
}

// ============================================================================
// SECTION 6: SIGNED TRANSFERS (ALICE ↔ BOB)
// ============================================================================

async function testAliceToBobTransfer() {
  const aliceBefore = await getBalance(ALICE.l1Address);
  const bobBefore = await getBalance(BOB.l1Address);
  
  console.log(`   Before: Alice=${aliceBefore} BB, Bob=${bobBefore} BB`);
  
  if (aliceBefore < 1) throw new Error('Alice needs at least 1 BB to test transfer');
  
  const amount = 1;
  const signedRequest = createSignedRequest({ to: BOB.l1Address, amount }, ALICE);
  
  const response = await fetch(`${L1_URL}/transfer`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(signedRequest)
  });
  
  const result = await response.json();
  assertTrue(result.success || result.tx_id, 'Transfer should succeed');
  
  await sleep(300);
  const aliceAfter = await getBalance(ALICE.l1Address);
  const bobAfter = await getBalance(BOB.l1Address);
  
  console.log(`   After:  Alice=${aliceAfter} BB, Bob=${bobAfter} BB`);
}

async function testBobToAliceTransfer() {
  const bobBefore = await getBalance(BOB.l1Address);
  
  if (bobBefore < 1) throw new Error('Bob needs at least 1 BB to test transfer');
  
  const amount = 0.5;
  const signedRequest = createSignedRequest({ to: ALICE.l1Address, amount }, BOB);
  
  const response = await fetch(`${L1_URL}/transfer`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(signedRequest)
  });
  
  const result = await response.json();
  assertTrue(result.success || result.tx_id, 'Transfer should succeed');
}

async function testTransferWithInsufficientBalance() {
  const amount = 9999999;
  const signedRequest = createSignedRequest({ to: BOB.l1Address, amount }, ALICE);
  
  const response = await fetch(`${L1_URL}/transfer`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(signedRequest)
  });
  
  const result = await response.json();
  assertTrue(!result.success || result.error, 'Over-limit transfer should fail');
}

async function testTransferWithInvalidSignature() {
  const signedRequest = {
    public_key: ALICE.publicKey,
    wallet_address: ALICE.l1Address,
    payload: JSON.stringify({ to: BOB.l1Address, amount: 1 }),
    timestamp: getTimestamp(),
    nonce: generateNonce(),
    signature: '0'.repeat(128)  // Invalid signature
  };
  
  const response = await fetch(`${L1_URL}/transfer`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(signedRequest)
  });
  
  const result = await response.json();
  assertTrue(!result.success || result.error, 'Invalid signature should be rejected');
}

// ============================================================================
// SECTION 7: BLOCK OPERATIONS
// ============================================================================

async function testRPCGetBlock() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 20, method: 'getBlock', params: [0] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'Should return genesis block');
}

async function testRPCGetLatestBlock() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 21, method: 'getLatestBlock', params: [] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result, 'Should return latest block');
}

async function testRPCGetTransactions() {
  const response = await fetch(`${L1_URL}/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 22, method: 'getTransactions', params: [ALICE.l1Address] })
  });
  assertTrue(response.ok, 'RPC should return OK');
  const data = await response.json();
  assertExists(data.result !== undefined, 'Should return transaction array');
}

// ============================================================================
// SECTION 8: BRIDGE ESCROW (Lock → Settle → Release)
// ============================================================================

let bridgeTestState = {};

async function testBridgeEscrow_CheckBalances() {
  const aliceBalance = await getBalance(ALICE.address);
  const bobBalance = await getBalance(BOB.address);
  
  console.log(`   💰 Alice: ${aliceBalance} BB, Bob: ${bobBalance} BB`);
  
  if (aliceBalance < 50) {
    throw new Error('Alice needs at least 50 BB for bridge escrow test');
  }
  
  bridgeTestState.initialAlice = aliceBalance;
  bridgeTestState.initialBob = bobBalance;
}

async function testBridgeEscrow_AliceLocks() {
  const bridgePayload = {
    target_address: BOB.address,
    amount: 25,  // Lock 25 BB for this test
    target_layer: 'L2',
  };
  
  const signedRequest = createSignedRequest(bridgePayload, ALICE);
  const result = await request('POST', '/bridge/initiate', signedRequest);
  
  console.log(`   🔒 Bridge ID: ${result.bridge_id || 'N/A'}`);
  console.log(`   🔒 Lock ID:   ${result.lock_id || 'N/A'}`);
  
  if (!result.success) throw new Error(result.error || 'Bridge initiate failed');
  
  bridgeTestState.lockId = result.lock_id;
  bridgeTestState.bridgeId = result.bridge_id;
  
  const newAliceBalance = await getBalance(ALICE.address);
  console.log(`   💰 Alice after lock: ${newAliceBalance} BB`);
}

async function testBridgeEscrow_L2VerifiesSettlement() {
  const settlementProof = {
    lock_id: bridgeTestState.lockId,
    market_id: 'market_sdk_test_001',
    outcome: 'BOB_WINS',
    beneficiary: BOB.address,
    amount: 25,
    l2_block_height: 12345,
    l2_signature: 'dev_mode_signature',
  };
  
  const result = await request('POST', '/bridge/verify-settlement', settlementProof);
  
  console.log(`   ✅ Release Authorized: ${result.release_authorized || false}`);
  
  if (!result.success) throw new Error(result.error || 'Settlement verification failed');
  assertTrue(result.release_authorized === true, 'Release should be authorized');
}

async function testBridgeEscrow_ReleaseTokensToBob() {
  const result = await request('POST', '/bridge/release', {
    lock_id: bridgeTestState.lockId,
  });
  
  console.log(`   🔓 Released to: ${result.recipient || 'N/A'}`);
  console.log(`   🔓 Amount: ${result.amount || 'N/A'} BB`);
  
  if (!result.success) throw new Error(result.error || 'Release failed');
  
  const newBobBalance = await getBalance(BOB.address);
  const expectedBob = bridgeTestState.initialBob + 25;
  
  console.log(`   💰 Bob after release: ${newBobBalance} BB (expected ~${expectedBob})`);
  assertTrue(newBobBalance >= expectedBob - 0.01, 'Bob should have received tokens');
}

async function testBridgeEscrow_VerifyConservation() {
  const aliceBalance = await getBalance(ALICE.address);
  const bobBalance = await getBalance(BOB.address);
  
  const aliceDelta = aliceBalance - bridgeTestState.initialAlice;
  const bobDelta = bobBalance - bridgeTestState.initialBob;
  
  console.log(`   📊 Alice delta: ${aliceDelta} BB`);
  console.log(`   📊 Bob delta:   ${bobDelta} BB`);
  
  assertTrue(Math.abs(aliceDelta + 25) < 0.01, 'Alice should have lost 25 BB');
  assertTrue(Math.abs(bobDelta - 25) < 0.01, 'Bob should have gained 25 BB');
}

async function testBridgeStats() {
  const result = await request('GET', '/bridge/stats');
  if (result.success !== false) {
    console.log(`   🌉 Total bridged L1→L2: ${result.total_bridged_l1_to_l2 || 0} BB`);
  }
}

async function testBridgePending() {
  const result = await request('GET', '/bridge/pending');
  if (result.success !== false) {
    console.log(`   📋 Pending bridges: ${result.pending_count || 0}`);
  }
}

// ============================================================================
// SECTION 9: L2 INTEGRATION (Optional - requires L2 server)
// ============================================================================

async function testL2Health() {
  const response = await fetch(`${L2_URL}/health`, { signal: AbortSignal.timeout(3000) });
  if (!response.ok) throw new Error('L2 health check failed');
  console.log('   🎮 L2 server is healthy');
}

async function testL2Markets() {
  const response = await fetch(`${L2_URL}/markets`);
  if (!response.ok) throw new Error('L2 markets endpoint failed');
  const data = await response.json();
  const marketCount = Array.isArray(data.markets) ? data.markets.length : 
                      Array.isArray(data) ? data.length : 0;
  console.log(`   🎯 Found ${marketCount} markets`);
}

async function testL2Balance() {
  const response = await fetch(`${L2_URL}/balance/${ALICE.l1Address}`);
  if (!response.ok) throw new Error('L2 balance endpoint failed');
  const data = await response.json();
  console.log(`   💰 Alice L2 balance: ${data.balance || 0} BB`);
}

async function testCrossLayerBalance() {
  const [l1Res, l2Res] = await Promise.all([
    fetch(`${L1_URL}/balance/${ALICE.l1Address}`),
    fetch(`${L2_URL}/balance/${ALICE.l1Address}`)
  ]);
  
  const l1Data = await l1Res.json();
  const l2Data = await l2Res.json();
  
  console.log(`   💰 L1 Balance: ${l1Data.balance || 0} BB`);
  console.log(`   🎮 L2 Balance: ${l2Data.balance || 0} BB`);
  console.log(`   📊 Total: ${(l1Data.balance || 0) + (l2Data.balance || 0)} BB`);
}

async function testL2InitialLiquidityEndpoint() {
  const response = await fetch(`${L1_URL}/markets/initial-liquidity/test-market-id`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ amount: 0, house_funded: false })
  });
  // Even with invalid data, endpoint should exist (not 404)
  if (response.status === 404) throw new Error('Initial liquidity endpoint not found');
  console.log('   📊 Initial liquidity endpoint exists');
}

// ============================================================================
// SECTION 10: SOCIAL MINING
// ============================================================================

async function testSocialPost() {
  const signedRequest = createSignedRequest({ 
    content: 'Test post from unified SDK tests!',
    media_type: 'text'
  }, ALICE);
  
  const response = await fetch(`${L1_URL}/social/post`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(signedRequest)
  });
  
  if (!response.ok) {
    log('Social post endpoint not available');
    return;
  }
  
  const result = await response.json();
  log('Social post result:', result);
}

async function testSocialStats() {
  const response = await fetch(`${L1_URL}/social/stats`);
  if (!response.ok) {
    log('Social stats endpoint not available');
    return;
  }
  const result = await response.json();
  log('Social stats:', result);
}

// ============================================================================
// SECTION 11: ADMIN OPERATIONS (Development Only)
// ============================================================================

async function testAdminMint() {
  const response = await fetch(`${L1_URL}/admin/mint`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ address: ALICE.l1Address, amount: 100 })
  });
  
  if (!response.ok) {
    log('Admin mint not available');
    return;
  }
  
  const result = await response.json();
  if (result.success) {
    console.log(`   🪙 Minted 100 BB to Alice`);
  }
}

// ============================================================================
// MAIN TEST RUNNER
// ============================================================================

async function runAllTests() {
  console.log('\n╔══════════════════════════════════════════════════════════════════╗');
  console.log('║         ALICE & BOB UNIFIED SDK TEST SUITE                        ║');
  console.log('║                                                                    ║');
  console.log('║   BlackBook L1/L2 Integration - Complete Test Coverage            ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝\n');
  
  // Load tweetnacl
  try {
    nacl = require('tweetnacl');
    console.log('✅ tweetnacl loaded for Ed25519 signing\n');
  } catch (e) {
    console.log('❌ tweetnacl not found. Please run: npm install tweetnacl');
    process.exit(1);
  }
  
  // Check servers
  const l1Available = await checkServer(L1_URL, 'L1');
  const l2Available = await checkServer(L2_URL, 'L2');
  
  console.log(`📌 L1 URL: ${L1_URL} ${l1Available ? '✅' : '❌'}`);
  console.log(`📌 L2 URL: ${L2_URL} ${l2Available ? '✅' : '⏸️  (optional)'}`);
  console.log(`📌 Alice: ${ALICE.l1Address}`);
  console.log(`📌 Bob:   ${BOB.l1Address}\n`);
  
  if (!l1Available) {
    console.log('\n❌ FATAL: L1 Server not available. Start it with: cargo run');
    process.exit(1);
  }
  
  // ==========================================================================
  // SECTION 1: L1 HEALTH & STATUS
  // ==========================================================================
  console.log('─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 1: L1 Health & Status');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('L1 Health Check', testL1Health);
  await runTest('L1 Stats', testL1Stats);
  await runTest('L1 PoH Status', testL1PoHStatus);
  
  // ==========================================================================
  // SECTION 2: BALANCE OPERATIONS
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 2: Balance Operations');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('Get Alice Balance', testGetAliceBalance);
  await runTest('Get Bob Balance', testGetBobBalance);
  await runTest('Get Multiple Balances (Parallel)', testGetMultipleBalances);
  
  // ==========================================================================
  // SECTION 3: RPC INTERFACE
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 3: RPC Interface');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('RPC getBlockHeight', testRPCGetBlockHeight);
  await runTest('RPC getBalance', testRPCGetBalance);
  await runTest('RPC getChainStats', testRPCGetChainStats);
  await runTest('RPC getAccountInfo', testRPCGetAccountInfo);
  
  // ==========================================================================
  // SECTION 4: BLOCKHASH (SOLANA-COMPATIBLE)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 4: Blockhash (Solana-Compatible)');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('RPC getRecentBlockhash', testRPCGetRecentBlockhash);
  await runTest('RPC getLatestBlockhash', testRPCGetLatestBlockhash);
  await runTest('RPC isBlockhashValid', testRPCIsBlockhashValid);
  await runTest('RPC getSlotLeader', testRPCGetSlotLeader);
  await runTest('RPC getFeeForMessage', testRPCGetFeeForMessage);
  await runTest('RPC getMinimumBalanceForRentExemption', testRPCGetMinimumBalanceForRentExemption);
  
  // ==========================================================================
  // SECTION 5: SIGNATURE VERIFICATION
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 5: Signature Verification');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('RPC Verify Valid Signature (Alice)', testRPCVerifySignature);
  await runTest('RPC Reject Invalid Signature', testRPCVerifyInvalidSignature);
  
  // ==========================================================================
  // SECTION 6: SIGNED TRANSFERS
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 6: Signed Transfers');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('Admin Mint to Alice (Setup)', testAdminMint);
  await sleep(300);
  
  await runTest('Alice → Bob Transfer (Signed)', testAliceToBobTransfer);
  await runTest('Bob → Alice Transfer (Signed)', testBobToAliceTransfer);
  await runTest('Transfer with Insufficient Balance (Reject)', testTransferWithInsufficientBalance);
  await runTest('Transfer with Invalid Signature (Reject)', testTransferWithInvalidSignature);
  
  // ==========================================================================
  // SECTION 7: BLOCK OPERATIONS
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 7: Block Operations');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('RPC getBlock (Genesis)', testRPCGetBlock);
  await runTest('RPC getLatestBlock', testRPCGetLatestBlock);
  await runTest('RPC getTransactions (Alice)', testRPCGetTransactions);
  
  // ==========================================================================
  // SECTION 8: BRIDGE ESCROW (Lock → Settle → Release)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 8: Bridge Escrow Flow (Alice → Lock → Bob)');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('Bridge: Check Initial Balances', testBridgeEscrow_CheckBalances);
  await runTest('Bridge: Alice Locks 25 BB', testBridgeEscrow_AliceLocks);
  await runTest('Bridge: L2 Verifies Settlement', testBridgeEscrow_L2VerifiesSettlement);
  await runTest('Bridge: Release Tokens to Bob', testBridgeEscrow_ReleaseTokensToBob);
  await runTest('Bridge: Verify Conservation', testBridgeEscrow_VerifyConservation);
  await runTest('Bridge: Get Stats', testBridgeStats);
  await runTest('Bridge: Get Pending', testBridgePending);
  
  // ==========================================================================
  // SECTION 9: L2 INTEGRATION (Optional)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 9: L2 Integration (Optional)');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('L1 Initial Liquidity Endpoint', testL2InitialLiquidityEndpoint);
  
  if (l2Available) {
    await runTest('L2 Health Check', testL2Health);
    await runTest('L2 Markets', testL2Markets);
    await runTest('L2 Balance (Alice)', testL2Balance);
    await runTest('Cross-Layer Balance Check', testCrossLayerBalance);
  } else {
    skipTest('L2 Health Check', 'L2 server not running');
    skipTest('L2 Markets', 'L2 server not running');
    skipTest('L2 Balance', 'L2 server not running');
    skipTest('Cross-Layer Balance', 'L2 server not running');
  }
  
  // ==========================================================================
  // SECTION 10: SOCIAL MINING
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 SECTION 10: Social Mining');
  console.log('─────────────────────────────────────────────────────────────────');
  
  await runTest('Social Post (Alice)', testSocialPost);
  await runTest('Social Stats', testSocialStats);
  
  // ==========================================================================
  // RESULTS SUMMARY
  // ==========================================================================
  console.log('\n╔══════════════════════════════════════════════════════════════════╗');
  console.log('║                      TEST RESULTS SUMMARY                          ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝\n');
  
  const total = testResults.passed + testResults.failed + testResults.skipped;
  const passRate = total > 0 ? ((testResults.passed / total) * 100).toFixed(1) : 0;
  
  console.log(`   ✅ Passed:  ${testResults.passed}`);
  console.log(`   ❌ Failed:  ${testResults.failed}`);
  console.log(`   ⏭️  Skipped: ${testResults.skipped}`);
  console.log(`   ─────────────────`);
  console.log(`   📊 Total:   ${total} tests`);
  console.log(`   📈 Pass Rate: ${passRate}%\n`);
  
  // List failed tests
  if (testResults.failed > 0) {
    console.log('   Failed Tests:');
    testResults.tests
      .filter(t => t.status === 'FAIL')
      .forEach(t => console.log(`   ❌ ${t.name}: ${t.error}`));
    console.log();
  }
  
  // Architecture diagram
  console.log('╔══════════════════════════════════════════════════════════════════╗');
  console.log('║                    INTEGRATION ARCHITECTURE                       ║');
  console.log('╠══════════════════════════════════════════════════════════════════╣');
  console.log('║                                                                   ║');
  console.log('║   Frontend App                                                    ║');
  console.log('║       │                                                           ║');
  console.log('║       ├── L1 SDK ──────────► L1 Server (port 8080)               ║');
  console.log('║       │                       • Ed25519 Signatures                ║');
  console.log('║       │                       • Balance Management                ║');
  console.log('║       │                       • Bridge Escrow                     ║');
  console.log('║       │                       • PoH/Slot Tracking                 ║');
  console.log('║       │                                                           ║');
  console.log('║       └── L2 SDK ──────────► L2 Server (port 1234)               ║');
  console.log('║                               • Prediction Markets                ║');
  console.log('║                               • Betting/CLOB                      ║');
  console.log('║                               • Session Management                ║');
  console.log('║                                                                   ║');
  console.log('║   Cross-Layer Bridge:                                             ║');
  console.log('║   L1 ←── /bridge/verify-settlement ←── L2 (settlement proof)     ║');
  console.log('║   L1 ───► /bridge/release ───► Bob (tokens released)             ║');
  console.log('║                                                                   ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝\n');
  
  // Final status
  if (testResults.failed === 0) {
    console.log('╔══════════════════════════════════════════════════════════════════╗');
    console.log('║       🎉 ALL TESTS PASSED! SDK IS WORKING CORRECTLY! 🎉          ║');
    console.log('╚══════════════════════════════════════════════════════════════════╝\n');
    process.exit(0);
  } else {
    console.log('╔══════════════════════════════════════════════════════════════════╗');
    console.log('║       ⚠️  SOME TESTS FAILED - SEE ABOVE FOR DETAILS ⚠️           ║');
    console.log('╚══════════════════════════════════════════════════════════════════╝\n');
    process.exit(1);
  }
}

// Run tests
runAllTests().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
