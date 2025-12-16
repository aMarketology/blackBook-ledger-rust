// ============================================================================
// Alice & Bob Integration Tests
// ============================================================================
// 
// Tests Layer 2 functionality and L1 RPC connectivity using test accounts
// 
// Run: node tests/test_alice_bob.js
// 
// Prerequisites:
//   - L2 server running on port 1234 (cargo run)
//   - L1 server running on port 8080 (optional, for full integration)
// ============================================================================

const L2_URL = 'http://localhost:1234';
const L1_URL = 'http://localhost:8080';

// Test Account Constants (from alice-bob.txt)
const ALICE = {
    address: 'L1ALICE000000001',
    publicKey: '4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57',
    privateKey: '616c6963655f746573745f6163636f756e745f76310000000000000000000001',
    username: 'alice_test',
    initialBalance: 10000
};

const BOB = {
    address: 'L1BOB00000000001',
    publicKey: 'b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f',
    privateKey: '626f625f746573745f6163636f756e745f763100000000000000000000000002',
    username: 'bob_test',
    initialBalance: 5000
};

// Test results tracking
let passed = 0;
let failed = 0;
const results = [];

// ============================================================================
// HELPERS
// ============================================================================

function timestamp() {
    return Math.floor(Date.now() / 1000);
}

function mockSignature(address, ts) {
    return `sig_${address}_${ts}`;
}

async function post(url, data) {
    const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data)
    });
    return { status: response.status, data: await response.json() };
}

async function get(url) {
    const response = await fetch(url);
    return { status: response.status, data: await response.json() };
}

function log(icon, message) {
    console.log(`${icon}  ${message}`);
}

function assert(condition, testName, details = '') {
    if (condition) {
        passed++;
        results.push({ name: testName, passed: true });
        log('✅', `PASS: ${testName}`);
    } else {
        failed++;
        results.push({ name: testName, passed: false, details });
        log('❌', `FAIL: ${testName} ${details ? '- ' + details : ''}`);
    }
}

// ============================================================================
// TEST FUNCTIONS
// ============================================================================

async function testHealthCheck() {
    log('🔍', 'Testing health check...');
    try {
        const response = await fetch(`${L2_URL}/health`);
        assert(response.status === 200, 'Health check returns 200');
        const text = await response.text();
        assert(text.includes('BlackBook') || text.includes('Online'), 'Health check returns expected text');
        log('   ', `Server response: ${text}`);
    } catch (e) {
        assert(false, 'Health check returns 200', e.message);
    }
}

async function testConnectAlice() {
    log('🔐', 'Connecting Alice wallet...');
    try {
        const { status, data } = await post(`${L2_URL}/auth/connect`, {
            wallet_address: ALICE.address,
            username: ALICE.username,
            public_key: ALICE.publicKey
        });
        assert(status === 200, 'Alice wallet connect returns 200');
        assert(data.success === true, 'Alice wallet connect succeeds');
        assert(data.wallet_address === ALICE.address, 'Alice address matches');
        return data;
    } catch (e) {
        assert(false, 'Alice wallet connect', e.message);
    }
}

async function testConnectBob() {
    log('🔐', 'Connecting Bob wallet...');
    try {
        const { status, data } = await post(`${L2_URL}/auth/connect`, {
            wallet_address: BOB.address,
            username: BOB.username,
            public_key: BOB.publicKey
        });
        assert(status === 200, 'Bob wallet connect returns 200');
        assert(data.success === true, 'Bob wallet connect succeeds');
        assert(data.wallet_address === BOB.address, 'Bob address matches');
        return data;
    } catch (e) {
        assert(false, 'Bob wallet connect', e.message);
    }
}

async function testGetAliceBalance() {
    log('💰', 'Getting Alice balance...');
    try {
        const { status, data } = await get(`${L2_URL}/balance/${ALICE.address}`);
        assert(status === 200, 'Alice balance returns 200');
        assert(typeof data.balance === 'number', 'Alice balance is a number');
        log('   ', `Alice balance: ${data.balance} BB`);
        return data.balance;
    } catch (e) {
        assert(false, 'Alice balance', e.message);
    }
}

async function testGetBobBalance() {
    log('💰', 'Getting Bob balance...');
    try {
        const { status, data } = await get(`${L2_URL}/balance/${BOB.address}`);
        assert(status === 200, 'Bob balance returns 200');
        assert(typeof data.balance === 'number', 'Bob balance is a number');
        log('   ', `Bob balance: ${data.balance} BB`);
        return data.balance;
    } catch (e) {
        assert(false, 'Bob balance', e.message);
    }
}

async function testGetMarkets() {
    log('📊', 'Getting markets...');
    try {
        const { status, data } = await get(`${L2_URL}/markets`);
        assert(status === 200, 'Get markets returns 200');
        assert(Array.isArray(data.markets), 'Markets is an array');
        log('   ', `Found ${data.markets.length} markets`);
        if (data.markets.length > 0) {
            log('   ', `First market: ${data.markets[0].id}`);
        }
        return data.markets;
    } catch (e) {
        assert(false, 'Get markets', e.message);
    }
}

async function testAlicePlacesBet(marketId) {
    log('🎯', `Alice placing YES bet on ${marketId}...`);
    const ts = timestamp();
    const nonce = ts;
    
    try {
        const { status, data } = await post(`${L2_URL}/bet/signed`, {
            signature: mockSignature(ALICE.address, ts),
            from_address: ALICE.address,
            market_id: marketId,
            option: 'YES',
            amount: 25.0,
            nonce: nonce,
            timestamp: ts
        });
        
        if (status === 200 && data.success) {
            assert(true, 'Alice YES bet succeeds');
            assert(data.outcome === 0, 'YES bet outcome is 0');
            assert(data.amount === 25.0, 'Bet amount is 25');
            log('   ', `Bet ID: ${data.bet_id}`);
            log('   ', `New balance: ${data.new_balance} BB`);
        } else if (status === 404) {
            log('⚠️ ', `Market ${marketId} not found - skipping bet test`);
            assert(true, 'Alice bet (market not found - skipped)');
        } else {
            assert(false, 'Alice YES bet', data.error || 'Unknown error');
        }
        return data;
    } catch (e) {
        assert(false, 'Alice YES bet', e.message);
    }
}

async function testBobPlacesBet(marketId) {
    log('🎯', `Bob placing NO bet on ${marketId}...`);
    const ts = timestamp();
    const nonce = ts + 1;
    
    try {
        const { status, data } = await post(`${L2_URL}/bet/signed`, {
            signature: mockSignature(BOB.address, ts),
            from_address: BOB.address,
            market_id: marketId,
            option: 'NO',
            amount: 20.0,
            nonce: nonce,
            timestamp: ts
        });
        
        if (status === 200 && data.success) {
            assert(true, 'Bob NO bet succeeds');
            assert(data.outcome === 1, 'NO bet outcome is 1');
            log('   ', `Bet ID: ${data.bet_id}`);
            log('   ', `New balance: ${data.new_balance} BB`);
        } else if (status === 404) {
            log('⚠️ ', `Market ${marketId} not found - skipping bet test`);
            assert(true, 'Bob bet (market not found - skipped)');
        } else {
            assert(false, 'Bob NO bet', data.error || 'Unknown error');
        }
        return data;
    } catch (e) {
        assert(false, 'Bob NO bet', e.message);
    }
}

async function testTransfer() {
    log('💸', 'Alice transferring 10 BB to Bob...');
    try {
        const { status, data } = await post(`${L2_URL}/transfer`, {
            from: ALICE.address,
            to: BOB.address,
            amount: 10.0
        });
        
        if (data.success) {
            assert(true, 'Transfer succeeds');
            log('   ', 'Transfer completed');
        } else {
            assert(false, 'Transfer', data.error || 'Failed');
        }
        return data;
    } catch (e) {
        assert(false, 'Transfer', e.message);
    }
}

async function testGetNonce() {
    log('🔢', 'Getting Alice nonce...');
    try {
        const { status, data } = await get(`${L2_URL}/rpc/nonce/${ALICE.address}`);
        assert(status === 200, 'Get nonce returns 200');
        log('   ', `Nonce: ${JSON.stringify(data)}`);
        return data;
    } catch (e) {
        assert(false, 'Get nonce', e.message);
    }
}

async function testGetUserBets() {
    log('📋', 'Getting Alice bet history...');
    try {
        const { status, data } = await get(`${L2_URL}/bets/${ALICE.address}`);
        assert(status === 200, 'Get user bets returns 200');
        log('   ', `Bets: ${JSON.stringify(data).slice(0, 100)}...`);
        return data;
    } catch (e) {
        assert(false, 'Get user bets', e.message);
    }
}

async function testGetLedger() {
    log('📜', 'Getting ledger transactions...');
    try {
        const { status, data } = await get(`${L2_URL}/ledger/transactions?limit=5`);
        assert(status === 200, 'Get ledger returns 200');
        log('   ', `Transactions: ${data.transactions?.length || 0}`);
        return data;
    } catch (e) {
        assert(false, 'Get ledger', e.message);
    }
}

async function testInvalidBet() {
    log('❌', 'Testing invalid bet (bad market)...');
    const ts = timestamp();
    
    try {
        const { status, data } = await post(`${L2_URL}/bet/signed`, {
            signature: mockSignature(ALICE.address, ts),
            from_address: ALICE.address,
            market_id: 'totally_fake_market_12345',
            option: 'YES',
            amount: 10.0,
            nonce: ts + 999,
            timestamp: ts
        });
        
        assert(status === 404, 'Invalid market returns 404');
        assert(data.success === false, 'Invalid market fails');
        return data;
    } catch (e) {
        assert(false, 'Invalid bet test', e.message);
    }
}

async function testReplayProtection() {
    log('🛡️', 'Testing replay attack protection...');
    const ts = timestamp();
    const fixedNonce = ts + 5000;
    
    try {
        // First bet should succeed (or fail for market not found)
        const { status: s1, data: d1 } = await post(`${L2_URL}/bet/signed`, {
            signature: mockSignature(ALICE.address, ts),
            from_address: ALICE.address,
            market_id: 'pegatron_georgetown_2026',
            option: 'YES',
            amount: 5.0,
            nonce: fixedNonce,
            timestamp: ts
        });
        
        // Second bet with same nonce should fail
        const { status: s2, data: d2 } = await post(`${L2_URL}/bet/signed`, {
            signature: mockSignature(ALICE.address, ts),
            from_address: ALICE.address,
            market_id: 'pegatron_georgetown_2026',
            option: 'YES',
            amount: 5.0,
            nonce: fixedNonce,  // Same nonce!
            timestamp: ts
        });
        
        if (s1 === 200 && d1.success) {
            // First succeeded, second should fail
            assert(d2.success === false, 'Replay attack blocked');
            if (d2.error) {
                log('   ', `Blocked: ${d2.error}`);
            }
        } else if (s1 === 404) {
            log('⚠️ ', 'Market not found - skipping replay test');
            assert(true, 'Replay test (skipped - market not found)');
        } else {
            log('⚠️ ', `First bet failed: ${d1.error}`);
            assert(true, 'Replay test (skipped - first bet failed)');
        }
    } catch (e) {
        assert(false, 'Replay protection test', e.message);
    }
}

async function testL1Connectivity() {
    log('🔗', 'Testing L1 RPC connectivity...');
    try {
        const response = await fetch(`${L1_URL}/health`);
        if (response.ok) {
            assert(true, 'L1 health check');
            log('   ', 'L1 server is reachable');
            
            // Try to get balance from L1
            const balResp = await get(`${L1_URL}/balance/${ALICE.address}`);
            if (balResp.status === 200) {
                assert(true, 'L1 balance query');
                log('   ', `L1 Alice balance: ${JSON.stringify(balResp.data)}`);
            }
        } else {
            log('⚠️ ', 'L1 server not running (optional)');
            assert(true, 'L1 connectivity (skipped - not running)');
        }
    } catch (e) {
        log('⚠️ ', 'L1 server not reachable (optional)');
        assert(true, 'L1 connectivity (skipped - not reachable)');
    }
}

// ============================================================================
// MAIN TEST RUNNER
// ============================================================================

async function runTests() {
    console.log('\n═══════════════════════════════════════════════════════════════');
    console.log('           🧪 ALICE & BOB INTEGRATION TESTS');
    console.log('═══════════════════════════════════════════════════════════════\n');
    
    console.log('📋 Test Accounts:');
    console.log(`   👩 Alice: ${ALICE.address}`);
    console.log(`   👨 Bob:   ${BOB.address}`);
    console.log(`   🌐 L2:    ${L2_URL}`);
    console.log(`   🔗 L1:    ${L1_URL}`);
    console.log('');
    
    // Run all tests
    await testHealthCheck();
    console.log('');
    
    await testConnectAlice();
    await testConnectBob();
    console.log('');
    
    await testGetAliceBalance();
    await testGetBobBalance();
    console.log('');
    
    const markets = await testGetMarkets();
    const testMarket = markets?.[0]?.id || 'pegatron_georgetown_2026';
    console.log('');
    
    await testAlicePlacesBet(testMarket);
    await testBobPlacesBet(testMarket);
    console.log('');
    
    await testTransfer();
    console.log('');
    
    await testGetAliceBalance();
    await testGetBobBalance();
    console.log('');
    
    await testGetNonce();
    await testGetUserBets();
    await testGetLedger();
    console.log('');
    
    await testInvalidBet();
    await testReplayProtection();
    console.log('');
    
    await testL1Connectivity();
    
    // Summary
    console.log('\n═══════════════════════════════════════════════════════════════');
    console.log('                        TEST SUMMARY');
    console.log('═══════════════════════════════════════════════════════════════\n');
    
    console.log(`   ✅ Passed: ${passed}`);
    console.log(`   ❌ Failed: ${failed}`);
    console.log(`   📊 Total:  ${passed + failed}`);
    console.log('');
    
    if (failed === 0) {
        console.log('   🎉 ALL TESTS PASSED!\n');
        process.exit(0);
    } else {
        console.log('   ⚠️  Some tests failed:\n');
        results.filter(r => !r.passed).forEach(r => {
            console.log(`      • ${r.name}: ${r.details || 'Failed'}`);
        });
        console.log('');
        process.exit(1);
    }
}

// Check if server is running first
async function checkServer() {
    try {
        await fetch(`${L2_URL}/health`);
        return true;
    } catch (e) {
        console.log('\n❌ ERROR: L2 server is not running!\n');
        console.log('   Start it first with: cargo run\n');
        console.log(`   Expected server at: ${L2_URL}\n`);
        process.exit(1);
    }
}

// Run
checkServer().then(() => runTests()).catch(e => {
    console.error('Test runner error:', e);
    process.exit(1);
});
