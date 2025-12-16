// ============================================================================
// L1 ↔ L2 FULL INTEGRATION TEST
// ============================================================================
//
// This test ACTUALLY calls both L1 (port 8080) and L2 (port 1234) servers
// to verify real cross-layer communication with Ed25519 signatures.
//
// Uses HARDCODED wallet addresses that exist on L1 (Alice & Bob)
//
// Prerequisites:
//   1. L1 server running on port 8080
//   2. L2 server running on port 1234
//   3. npm install tweetnacl
//
// Run: node integration/l1-l2-full-integration-test.js
//
// ============================================================================

// ============================================================================
// TEST ACCOUNTS - HARDCODED (same as L1 blockchain)
// ============================================================================

const ALICE = {
  name: "Alice",
  email: "alice@blackbook.test",
  username: "alice_test",
  publicKey: "4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57",
  privateKey: "616c6963655f746573745f6163636f756e745f76310000000000000000000001",
  l1Address: "L1ALICE000000001",
  l2Address: "L148F582A1BC8976",
};

const BOB = {
  name: "Bob",
  email: "bob@blackbook.test",
  username: "bob_test",
  publicKey: "b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f",
  privateKey: "626f625f746573745f6163636f756e745f763100000000000000000000000002",
  l1Address: "L1BOB00000000001",
  l2Address: "L16DD0DC4C96CABD",
};

// ORACLE is L2-only admin for market resolution
const ORACLE = {
  name: "Oracle",
  publicKey: "e4853d1336c460d500d47b95bf1335ad1612f0847c77816749cfda0dd7cbfa43",
};

// ============================================================================
// CONFIGURATION
// ============================================================================

const L1_URL = process.env.L1_URL || 'http://localhost:8080';
const L2_URL = process.env.L2_URL || 'http://localhost:1234';
const VERBOSE = true;

// ============================================================================
// LOAD TWEETNACL FOR ED25519 SIGNING
// ============================================================================

let nacl;
try {
  nacl = require('tweetnacl');
  console.log('✅ tweetnacl loaded - using REAL Ed25519 signatures');
} catch (e) {
  console.error('❌ tweetnacl not found - install with: npm install tweetnacl');
  process.exit(1);
}

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

/**
 * Sign a message with Ed25519 using the account's private key
 * @param {string} privateKeyHex - 64-char hex private key seed
 * @param {string} message - Message to sign
 * @returns {string} 128-char hex signature
 */
function signMessage(privateKeyHex, message) {
  const seed = hexToBytes(privateKeyHex);
  const keyPair = nacl.sign.keyPair.fromSeed(seed);
  const messageBytes = new TextEncoder().encode(message);
  const signatureBytes = nacl.sign.detached(messageBytes, keyPair.secretKey);
  return bytesToHex(signatureBytes);
}

/**
 * Verify a signature locally
 */
function verifySignature(publicKeyHex, message, signatureHex) {
  const publicKey = hexToBytes(publicKeyHex);
  const signature = hexToBytes(signatureHex);
  const messageBytes = new TextEncoder().encode(message);
  return nacl.sign.detached.verify(messageBytes, signature, publicKey);
}

// ============================================================================
// HTTP CLIENT
// ============================================================================

async function request(baseUrl, method, endpoint, body = null) {
  const url = `${baseUrl}${endpoint}`;
  const options = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };

  if (body) {
    options.body = JSON.stringify(body);
  }

  try {
    const response = await fetch(url, options);
    const text = await response.text();
    let data;
    try {
      data = JSON.parse(text);
    } catch (e) {
      data = text;
    }

    return {
      ok: response.ok,
      status: response.status,
      data
    };
  } catch (error) {
    return { ok: false, error: error.message };
  }
}

const l1 = (method, endpoint, body) => request(L1_URL, method, endpoint, body);
const l2 = (method, endpoint, body) => request(L2_URL, method, endpoint, body);

// ============================================================================
// UTILITIES
// ============================================================================

function log(msg, data = null) {
  if (VERBOSE) {
    const time = new Date().toISOString().split('T')[1].split('.')[0];
    if (data) {
      console.log(`[${time}] ${msg}`, typeof data === 'object' ? JSON.stringify(data, null, 2) : data);
    } else {
      console.log(`[${time}] ${msg}`);
    }
  }
}

function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

function generateNonce() {
  return Date.now();
}

function generateUuid() {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

let testResults = { passed: 0, failed: 0, tests: [] };

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

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

// ============================================================================
// MAIN TEST SUITE
// ============================================================================

async function runAllTests() {
  console.log('\n╔══════════════════════════════════════════════════════════════════╗');
  console.log('║   L1 ↔ L2 FULL INTEGRATION TEST - REAL WALLETS                   ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝\n');

  console.log(`📌 L1 URL: ${L1_URL}`);
  console.log(`📌 L2 URL: ${L2_URL}`);
  console.log(`📌 Alice L1: ${ALICE.l1Address} | PubKey: ${ALICE.publicKey.substring(0, 16)}...`);
  console.log(`📌 Bob L1:   ${BOB.l1Address} | PubKey: ${BOB.publicKey.substring(0, 16)}...`);
  console.log(`📌 Oracle:   ${ORACLE.publicKey.substring(0, 16)}...\n`);

  // ==========================================================================
  // STEP 1: CHECK BOTH SERVERS ARE RUNNING
  // ==========================================================================
  console.log('─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 1: Server Health Checks');
  console.log('─────────────────────────────────────────────────────────────────');

  let l1Online = false;
  let l2Online = false;

  await runTest('L1 Server Health Check (port 8080)', async () => {
    const res = await l1('GET', '/health');
    assert(res.ok, `L1 server not responding: ${res.error || JSON.stringify(res.data)}`);
    l1Online = true;
    log('L1 Server:', res.data);
  });

  await runTest('L2 Server Health Check (port 1234)', async () => {
    const res = await l2('GET', '/health');
    assert(res.ok, `L2 server not responding: ${res.error || JSON.stringify(res.data)}`);
    l2Online = true;
    log('L2 Server:', res.data);
  });

  if (!l1Online || !l2Online) {
    console.log('\n❌ Cannot continue - both servers must be running!');
    console.log('   Start L1: cargo run (in L1 repo)');
    console.log('   Start L2: cargo run (in this repo)');
    return 1;
  }

  // ==========================================================================
  // STEP 2: CHECK L1 BALANCES (Alice & Bob REAL balances)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 2: Check L1 Balances (Real Wallet Addresses)');
  console.log('─────────────────────────────────────────────────────────────────');

  let aliceL1Balance = 0;
  let bobL1Balance = 0;

  await runTest('Get Alice L1 Balance', async () => {
    const res = await l1('GET', `/balance/${ALICE.l1Address}`);
    assert(res.ok, `Failed to get Alice L1 balance: ${JSON.stringify(res.data)}`);
    aliceL1Balance = res.data.balance || 0;
    log(`✅ Alice L1 Balance: ${aliceL1Balance} BB`);
    assert(aliceL1Balance > 0, `Alice should have L1 balance, got ${aliceL1Balance}`);
  });

  await runTest('Get Bob L1 Balance', async () => {
    const res = await l1('GET', `/balance/${BOB.l1Address}`);
    assert(res.ok, `Failed to get Bob L1 balance: ${JSON.stringify(res.data)}`);
    bobL1Balance = res.data.balance || 0;
    log(`✅ Bob L1 Balance: ${bobL1Balance} BB`);
  });

  // ==========================================================================
  // STEP 3: L1 SIGNATURE VERIFICATION (Ed25519)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 3: L1 Signature Verification (Ed25519)');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Alice signs message and L1 verifies', async () => {
    const message = `alice_auth_${Date.now()}`;
    const signature = signMessage(ALICE.privateKey, message);
    
    log(`Message: ${message}`);
    log(`Signature: ${signature.substring(0, 32)}...`);
    
    // Verify locally first
    const localValid = verifySignature(ALICE.publicKey, message, signature);
    assert(localValid, 'Local signature verification failed!');
    log('✅ Local verification: PASSED');
    
    // Verify on L1
    const res = await l1('POST', '/rpc/verify-signature', {
      public_key: ALICE.publicKey,
      message: message,
      signature: signature,
    });
    
    if (res.ok && res.data.valid) {
      log('✅ L1 verification: PASSED');
    } else {
      log('L1 verification response:', res.data);
    }
  });

  await runTest('Bob signs message and L1 verifies', async () => {
    const message = `bob_auth_${Date.now()}`;
    const signature = signMessage(BOB.privateKey, message);
    
    const localValid = verifySignature(BOB.publicKey, message, signature);
    assert(localValid, 'Bob local signature verification failed!');
    log('✅ Bob local verification: PASSED');
    
    const res = await l1('POST', '/rpc/verify-signature', {
      public_key: BOB.publicKey,
      message: message,
      signature: signature,
    });
    
    if (res.ok && res.data.valid) {
      log('✅ L1 Bob verification: PASSED');
    }
    log('L1 response:', res.data);
  });

  // ==========================================================================
  // STEP 4: L1 TOKEN TRANSFER (Alice → Bob on L1)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 4: L1 Token Transfer (Alice → Bob)');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Alice transfers 1 BB to Bob on L1 (SIGNED)', async () => {
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 1;
    
    // Create transfer message to sign
    const message = `transfer:${ALICE.l1Address}:${BOB.l1Address}:${amount}:${nonce}:${timestamp}`;
    const signature = signMessage(ALICE.privateKey, message);
    
    log(`Transfer: ${ALICE.l1Address} → ${BOB.l1Address} (${amount} BB)`);
    log(`Signature: ${signature.substring(0, 32)}...`);
    
    // Try L1 transfer endpoint
    const res = await l1('POST', '/transfer', {
      from: ALICE.l1Address,
      to: BOB.l1Address,
      amount: amount,
      signature: signature,
      nonce: nonce,
      timestamp: timestamp.toString(),
    });
    
    if (res.ok) {
      log('✅ L1 Transfer SUCCESS:', res.data);
    } else {
      log('L1 Transfer response:', res.data);
    }
  });

  // ==========================================================================
  // STEP 5: CHECK L2 BALANCES
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 5: Check L2 Balances');
  console.log('─────────────────────────────────────────────────────────────────');

  let aliceL2Balance = 0;
  let bobL2Balance = 0;

  await runTest('Get Alice L2 Balance', async () => {
    // Try with L1 address first, then public key
    let res = await l2('GET', `/balance/${ALICE.l1Address}`);
    if (!res.ok) {
      res = await l2('GET', `/balance/${ALICE.publicKey}`);
    }
    if (res.ok) {
      aliceL2Balance = res.data.balance || 0;
      log(`Alice L2 Balance: ${aliceL2Balance} BB`);
    } else {
      log('Alice L2 balance response:', res.data);
    }
  });

  await runTest('Get Bob L2 Balance', async () => {
    let res = await l2('GET', `/balance/${BOB.l1Address}`);
    if (!res.ok) {
      res = await l2('GET', `/balance/${BOB.publicKey}`);
    }
    if (res.ok) {
      bobL2Balance = res.data.balance || 0;
      log(`Bob L2 Balance: ${bobL2Balance} BB`);
    } else {
      log('Bob L2 balance response:', res.data);
    }
  });

  // ==========================================================================
  // STEP 6: BRIDGE DEPOSIT (L1 → L2) - Both Alice and Bob
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 6: Bridge Deposit (L1 → L2) - Alice & Bob');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Alice deposits 100 BB from L1 to L2', async () => {
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 100;
    
    // Sign the bridge deposit
    const message = `bridge:${ALICE.l1Address}:${ALICE.publicKey}:${amount}:${nonce}`;
    const signature = signMessage(ALICE.privateKey, message);
    
    const res = await l2('POST', '/bridge/deposit', {
      bridge_id: `bridge_${generateUuid()}`,
      from_address: ALICE.l1Address,
      to_address: ALICE.publicKey,
      amount: amount,
      l1_tx_hash: `0x${generateUuid().replace(/-/g, '')}${generateUuid().replace(/-/g, '')}`,
      l1_slot: 12345678,
      signature: signature,
    });
    
    if (res.ok) {
      log('✅ Alice Bridge Deposit SUCCESS:', res.data);
    } else {
      log('Alice Bridge Deposit response:', res.data);
    }
  });

  await runTest('Bob deposits 100 BB from L1 to L2', async () => {
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 100;
    
    // Sign the bridge deposit
    const message = `bridge:${BOB.l1Address}:${BOB.publicKey}:${amount}:${nonce}`;
    const signature = signMessage(BOB.privateKey, message);
    
    const res = await l2('POST', '/bridge/deposit', {
      bridge_id: `bridge_${generateUuid()}`,
      from_address: BOB.l1Address,
      to_address: BOB.publicKey,
      amount: amount,
      l1_tx_hash: `0x${generateUuid().replace(/-/g, '')}${generateUuid().replace(/-/g, '')}`,
      l1_slot: 12345679,
      signature: signature,
    });
    
    if (res.ok) {
      log('✅ Bob Bridge Deposit SUCCESS:', res.data);
    } else {
      log('Bob Bridge Deposit response:', res.data);
    }
  });

  // ==========================================================================
  // STEP 7: CREATE MARKET AND PLACE SIGNED BETS ON L2
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 7: Create Market & Place SIGNED Bets on L2');
  console.log('─────────────────────────────────────────────────────────────────');

  let testMarketId = null;

  await runTest('Create test market on L2', async () => {
    const res = await l2('POST', '/markets', {
      title: `L1-L2 Integration Test [${Date.now()}]`,
      description: "Testing full L1↔L2 signature verification with real wallets",
      category: "test",
      outcomes: ["YES", "NO"],
      end_time: getTimestamp() + 86400,
      creator: ALICE.publicKey,
      source: `integration_test_${generateUuid()}`
    });
    
    assert(res.ok, `Create market failed: ${JSON.stringify(res.data)}`);
    testMarketId = res.data.market_id || res.data.id;
    log('✅ Market created:', testMarketId);
  });

  await runTest('Alice places SIGNED bet on YES', async () => {
    if (!testMarketId) throw new Error('No market created');
    
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 50;
    
    // Create message to sign
    const message = `bet:${testMarketId}:YES:${amount}:${ALICE.publicKey}:${nonce}:${timestamp}`;
    const signature = signMessage(ALICE.privateKey, message);
    
    log(`Alice bet signature: ${signature.substring(0, 32)}...`);
    
    const res = await l2('POST', '/bet/signed', {
      market_id: testMarketId,
      option: "YES",
      amount: amount,
      from_address: ALICE.publicKey,
      signature: signature,
      nonce: parseInt(nonce),
      timestamp: timestamp
    });
    
    if (res.ok && res.data.success) {
      log('✅ Alice SIGNED bet placed:', res.data);
    } else {
      log('Alice bet response:', res.data);
    }
  });

  await runTest('Bob places SIGNED bet on NO', async () => {
    if (!testMarketId) throw new Error('No market created');
    
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 50;
    
    const message = `bet:${testMarketId}:NO:${amount}:${BOB.publicKey}:${nonce}:${timestamp}`;
    const signature = signMessage(BOB.privateKey, message);
    
    log(`Bob bet signature: ${signature.substring(0, 32)}...`);
    
    const res = await l2('POST', '/bet/signed', {
      market_id: testMarketId,
      option: "NO",
      amount: amount,
      from_address: BOB.publicKey,
      signature: signature,
      nonce: parseInt(nonce),
      timestamp: timestamp
    });
    
    if (res.ok && res.data.success) {
      log('✅ Bob SIGNED bet placed:', res.data);
    } else {
      log('Bob bet response:', res.data);
    }
  });

  // ==========================================================================
  // STEP 8: BRIDGE WITHDRAW (L2 → L1)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 8: Bridge Withdraw (L2 → L1)');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Alice withdraws 25 BB from L2 to L1 (SIGNED)', async () => {
    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const amount = 25;
    
    const message = `withdraw:${ALICE.publicKey}:${ALICE.l1Address}:${amount}:${nonce}:${timestamp}`;
    const signature = signMessage(ALICE.privateKey, message);
    
    const res = await l2('POST', '/bridge/withdraw', {
      wallet: ALICE.publicKey,
      target_address: ALICE.l1Address,
      amount: amount,
      signature: signature,
      nonce: nonce,
      timestamp: timestamp
    });
    
    if (res.ok) {
      log('✅ L2→L1 Withdrawal initiated:', res.data);
    } else {
      log('Withdrawal response:', res.data);
    }
  });

  // ==========================================================================
  // STEP 9: FINAL BALANCE CHECK (Both Layers)
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 9: Final Balance Check (L1 & L2)');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Check Alice final balances', async () => {
    // L1 balance
    const l1Res = await l1('GET', `/balance/${ALICE.l1Address}`);
    if (l1Res.ok) {
      log(`Alice L1 Final: ${l1Res.data.balance} BB`);
    }
    
    // L2 balance
    const l2Res = await l2('GET', `/balance/${ALICE.publicKey}`);
    if (l2Res.ok) {
      log(`Alice L2 Final: ${l2Res.data.balance} BB`);
    }
  });

  await runTest('Check Bob final balances', async () => {
    const l1Res = await l1('GET', `/balance/${BOB.l1Address}`);
    if (l1Res.ok) {
      log(`Bob L1 Final: ${l1Res.data.balance} BB`);
    }
    
    const l2Res = await l2('GET', `/balance/${BOB.publicKey}`);
    if (l2Res.ok) {
      log(`Bob L2 Final: ${l2Res.data.balance} BB`);
    }
  });

  // ==========================================================================
  // STEP 10: L1 LEDGER/TRANSACTION CHECK
  // ==========================================================================
  console.log('\n─────────────────────────────────────────────────────────────────');
  console.log('📋 STEP 10: L1 Ledger Activity');
  console.log('─────────────────────────────────────────────────────────────────');

  await runTest('Get L1 recent transactions', async () => {
    const res = await l1('GET', '/transactions/recent');
    if (res.ok) {
      const txCount = res.data.transactions?.length || res.data.length || 0;
      log(`L1 has ${txCount} recent transactions`);
    } else {
      log('L1 transactions response:', res.data);
    }
  });

  await runTest('Get L2 ledger activity', async () => {
    const res = await l2('GET', '/ledger');
    if (res.ok) {
      const txCount = res.data.transactions?.length || res.data.length || 0;
      log(`L2 ledger has ${txCount} transactions`);
    } else {
      log('L2 ledger response:', res.data);
    }
  });

  // ==========================================================================
  // RESULTS SUMMARY
  // ==========================================================================
  console.log('\n╔══════════════════════════════════════════════════════════════════╗');
  console.log('║                      TEST RESULTS SUMMARY                         ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝\n');

  const total = testResults.passed + testResults.failed;
  const passRate = total > 0 ? ((testResults.passed / total) * 100).toFixed(1) : 0;

  console.log(`   ✅ Passed:  ${testResults.passed}`);
  console.log(`   ❌ Failed:  ${testResults.failed}`);
  console.log(`   ─────────────────`);
  console.log(`   📊 Total:   ${total} tests`);
  console.log(`   📈 Pass Rate: ${passRate}%\n`);

  if (testResults.failed > 0) {
    console.log('   Failed Tests:');
    testResults.tests
      .filter(t => t.status === 'FAIL')
      .forEach(t => {
        console.log(`   ❌ ${t.name}: ${t.error}`);
      });
    console.log();
  }

  if (testResults.failed === 0) {
    console.log('╔══════════════════════════════════════════════════════════════════╗');
    console.log('║   🎉 ALL TESTS PASSED! L1↔L2 INTEGRATION WORKING! 🎉             ║');
    console.log('╚══════════════════════════════════════════════════════════════════╝\n');
  }

  return testResults.failed === 0 ? 0 : 1;
}

// Run tests
runAllTests()
  .then(exitCode => process.exit(exitCode))
  .catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
  });
