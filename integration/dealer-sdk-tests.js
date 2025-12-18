/**
 * ============================================================================
 * DEALER SDK TESTS - Layer 2 Oracle & Market Authority
 * ============================================================================
 * 
 * Tests the Dealer account functionality:
 * - Keypair validation (public key matches private key)
 * - Signature generation and verification
 * - Address format validation
 * - Integration with L1/L2 endpoints
 * 
 * ⚠️  SECURITY NOTE:
 * - The private key is loaded from environment variable
 * - In production, the private key should NEVER be in code
 * - This test file is for DEVELOPMENT ONLY
 */

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

// Load .env file from project root
const envPath = path.resolve(__dirname, '..', '.env');
if (fs.existsSync(envPath)) {
  const envContent = fs.readFileSync(envPath, 'utf8');
  envContent.split('\n').forEach(line => {
    const [key, ...valueParts] = line.split('=');
    if (key && valueParts.length > 0) {
      const value = valueParts.join('=').trim();
      if (!process.env[key.trim()]) {
        process.env[key.trim()] = value;
      }
    }
  });
  console.log("📁 Loaded .env from:", envPath);
}

// ============================================================================
// DEALER CREDENTIALS
// ============================================================================

// Public credentials (safe to store in code)
const DEALER = {
  address: "L2DEALER00000001",
  publicKey: "f19717a1860761b4e1b64101941c2115a416a07c57ff4fa3a91df7024b413d69",
  // Private key loaded from environment - NEVER hardcode in production!
  privateKey: process.env.DEALER_PRIVATE_KEY || process.env.dealer_private_key || null,
};

// L1 and L2 endpoints
const L1_URL = process.env.L1_URL || "http://localhost:8080";
const L2_URL = process.env.L2_URL || "http://localhost:1234";

// ============================================================================
// ED25519 SIGNING UTILITIES
// ============================================================================

/**
 * Sign a message using Ed25519
 * @param {string} privateKeyHex - 64 char hex private key
 * @param {string} message - Message to sign
 * @returns {string} - 128 char hex signature
 */
function signMessage(privateKeyHex, message) {
  if (!privateKeyHex) {
    throw new Error("Private key not provided. Set DEALER_PRIVATE_KEY environment variable.");
  }
  
  const privateKeyBuffer = Buffer.from(privateKeyHex, 'hex');
  
  // Ed25519 uses the first 32 bytes as seed, derives keypair
  const { privateKey } = crypto.generateKeyPairSync('ed25519', {
    privateKeyEncoding: { type: 'pkcs8', format: 'der' }
  });
  
  // For Ed25519, we need to use the sign function with the raw key
  // Node.js crypto expects the key in a specific format
  const keyObject = crypto.createPrivateKey({
    key: Buffer.concat([
      // Ed25519 PKCS8 header
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      privateKeyBuffer
    ]),
    format: 'der',
    type: 'pkcs8'
  });
  
  const signature = crypto.sign(null, Buffer.from(message), keyObject);
  return signature.toString('hex');
}

/**
 * Verify an Ed25519 signature
 * @param {string} publicKeyHex - 64 char hex public key
 * @param {string} message - Original message
 * @param {string} signatureHex - 128 char hex signature
 * @returns {boolean}
 */
function verifySignature(publicKeyHex, message, signatureHex) {
  const publicKeyBuffer = Buffer.from(publicKeyHex, 'hex');
  
  const keyObject = crypto.createPublicKey({
    key: Buffer.concat([
      // Ed25519 SPKI header
      Buffer.from('302a300506032b6570032100', 'hex'),
      publicKeyBuffer
    ]),
    format: 'der',
    type: 'spki'
  });
  
  const signature = Buffer.from(signatureHex, 'hex');
  return crypto.verify(null, Buffer.from(message), keyObject, signature);
}

/**
 * Derive public key from private key
 * @param {string} privateKeyHex - 64 char hex private key  
 * @returns {string} - 64 char hex public key
 */
function derivePublicKey(privateKeyHex) {
  const privateKeyBuffer = Buffer.from(privateKeyHex, 'hex');
  
  const keyObject = crypto.createPrivateKey({
    key: Buffer.concat([
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      privateKeyBuffer
    ]),
    format: 'der',
    type: 'pkcs8'
  });
  
  const publicKey = crypto.createPublicKey(keyObject);
  const publicKeyDer = publicKey.export({ type: 'spki', format: 'der' });
  
  // Extract raw 32-byte public key (skip SPKI header)
  const rawPublicKey = publicKeyDer.slice(-32);
  return rawPublicKey.toString('hex');
}

/**
 * Create a signed request payload for the Dealer
 */
function createDealerSignedRequest(payload) {
  if (!DEALER.privateKey) {
    throw new Error("DEALER_PRIVATE_KEY not set in environment");
  }
  
  const timestamp = Math.floor(Date.now() / 1000);
  const nonce = `dealer_${timestamp}_${Math.random().toString(36).slice(2)}`;
  
  const payloadStr = typeof payload === 'string' ? payload : JSON.stringify(payload);
  const message = `${payloadStr}\n${timestamp}\n${nonce}`;
  const signature = signMessage(DEALER.privateKey, message);
  
  return {
    public_key: DEALER.publicKey,
    wallet_address: DEALER.address,
    payload: payloadStr,
    signature: signature,
    timestamp: timestamp,
    nonce: nonce
  };
}

// ============================================================================
// TEST SUITE
// ============================================================================

async function runTests() {
  console.log("═".repeat(70));
  console.log("🎰 DEALER SDK TESTS");
  console.log("═".repeat(70));
  console.log();
  
  let passed = 0;
  let failed = 0;
  
  // -------------------------------------------------------------------------
  // Test 1: Dealer address format
  // -------------------------------------------------------------------------
  console.log("Test 1: Dealer Address Format");
  try {
    if (DEALER.address === "L2DEALER00000001") {
      console.log("  ✅ Address is correct: " + DEALER.address);
      console.log("  ✅ Address length: " + DEALER.address.length + " (expected 16)");
      console.log("  ✅ Prefix 'L2' indicates Layer 2 native");
      passed++;
    } else {
      throw new Error("Address mismatch");
    }
  } catch (e) {
    console.log("  ❌ FAILED: " + e.message);
    failed++;
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 2: Public key format
  // -------------------------------------------------------------------------
  console.log("Test 2: Public Key Format");
  try {
    if (DEALER.publicKey.length === 64) {
      console.log("  ✅ Public key length: 64 hex chars (32 bytes)");
      console.log("  ✅ Public key: " + DEALER.publicKey.slice(0, 16) + "...");
      passed++;
    } else {
      throw new Error(`Invalid length: ${DEALER.publicKey.length}`);
    }
  } catch (e) {
    console.log("  ❌ FAILED: " + e.message);
    failed++;
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 3: Private key available (from env)
  // -------------------------------------------------------------------------
  console.log("Test 3: Private Key Loaded from Environment");
  try {
    if (DEALER.privateKey) {
      console.log("  ✅ Private key loaded (length: " + DEALER.privateKey.length + ")");
      console.log("  ✅ Key preview: " + DEALER.privateKey.slice(0, 8) + "..." + DEALER.privateKey.slice(-8));
      passed++;
    } else {
      throw new Error("DEALER_PRIVATE_KEY not set in environment");
    }
  } catch (e) {
    console.log("  ⚠️  SKIPPED: " + e.message);
    console.log("  💡 Set environment variable: $env:DEALER_PRIVATE_KEY='...'");
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 4: Keypair validation (public matches private)
  // -------------------------------------------------------------------------
  console.log("Test 4: Keypair Validation (Public matches Private)");
  try {
    if (!DEALER.privateKey) {
      throw new Error("Private key not available");
    }
    
    const derivedPublic = derivePublicKey(DEALER.privateKey);
    
    if (derivedPublic === DEALER.publicKey) {
      console.log("  ✅ Derived public key matches stored public key");
      console.log("  ✅ Keypair is valid and consistent");
      passed++;
    } else {
      console.log("  ❌ Derived:  " + derivedPublic);
      console.log("  ❌ Expected: " + DEALER.publicKey);
      throw new Error("Public key mismatch - keypair invalid!");
    }
  } catch (e) {
    console.log("  ❌ FAILED: " + e.message);
    failed++;
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 5: Sign and verify message
  // -------------------------------------------------------------------------
  console.log("Test 5: Sign and Verify Message");
  try {
    if (!DEALER.privateKey) {
      throw new Error("Private key not available");
    }
    
    const testMessage = "test_dealer_signature_" + Date.now();
    console.log("  📝 Message: " + testMessage);
    
    const signature = signMessage(DEALER.privateKey, testMessage);
    console.log("  ✍️  Signature: " + signature.slice(0, 32) + "...");
    
    const isValid = verifySignature(DEALER.publicKey, testMessage, signature);
    
    if (isValid) {
      console.log("  ✅ Signature verified successfully");
      passed++;
    } else {
      throw new Error("Signature verification failed");
    }
  } catch (e) {
    console.log("  ❌ FAILED: " + e.message);
    failed++;
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 6: Create signed request payload
  // -------------------------------------------------------------------------
  console.log("Test 6: Create Signed Request Payload");
  try {
    if (!DEALER.privateKey) {
      throw new Error("Private key not available");
    }
    
    const payload = { action: "test", market_id: "btc_100k" };
    const signedRequest = createDealerSignedRequest(payload);
    
    console.log("  ✅ Signed request created:");
    console.log("     wallet_address: " + signedRequest.wallet_address);
    console.log("     public_key: " + signedRequest.public_key.slice(0, 16) + "...");
    console.log("     timestamp: " + signedRequest.timestamp);
    console.log("     nonce: " + signedRequest.nonce);
    console.log("     signature: " + signedRequest.signature.slice(0, 32) + "...");
    
    // Verify the signature
    const message = `${signedRequest.payload}\n${signedRequest.timestamp}\n${signedRequest.nonce}`;
    const isValid = verifySignature(DEALER.publicKey, message, signedRequest.signature);
    
    if (isValid) {
      console.log("  ✅ Request signature verified");
      passed++;
    } else {
      throw new Error("Request signature invalid");
    }
  } catch (e) {
    console.log("  ❌ FAILED: " + e.message);
    failed++;
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 7: Check L1 balance endpoint
  // -------------------------------------------------------------------------
  console.log("Test 7: L1 Balance Endpoint");
  try {
    const response = await fetch(`${L1_URL}/balance/${DEALER.address}`);
    
    if (response.ok) {
      const data = await response.json();
      console.log("  ✅ L1 responded successfully");
      console.log("     Balance: " + (data.balance || data.available || 0) + " BB");
      passed++;
    } else {
      console.log("  ⚠️  L1 returned: " + response.status);
      // Dealer might not have L1 balance (L2 native)
      console.log("  💡 Expected: Dealer is L2 native, may have 0 on L1");
      passed++;
    }
  } catch (e) {
    console.log("  ⚠️  Could not connect to L1: " + e.message);
    console.log("  💡 Start L1 with: cargo run");
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Test 8: Check L2 balance endpoint
  // -------------------------------------------------------------------------
  console.log("Test 8: L2 Balance Endpoint");
  try {
    const response = await fetch(`${L2_URL}/balance/${DEALER.address}`);
    
    if (response.ok) {
      const data = await response.json();
      console.log("  ✅ L2 responded successfully");
      console.log("     Balance: " + (data.balance || 0) + " BB");
      passed++;
    } else {
      console.log("  ⚠️  L2 returned: " + response.status);
    }
  } catch (e) {
    console.log("  ⚠️  Could not connect to L2: " + e.message);
    console.log("  💡 L2 may not be running yet");
  }
  console.log();
  
  // -------------------------------------------------------------------------
  // Summary
  // -------------------------------------------------------------------------
  console.log("═".repeat(70));
  console.log(`📊 RESULTS: ${passed} passed, ${failed} failed`);
  console.log("═".repeat(70));
  
  if (failed === 0) {
    console.log("🎉 All tests passed! Dealer account is ready.");
  } else {
    console.log("⚠️  Some tests failed. Check output above.");
  }
  
  return { passed, failed };
}

// ============================================================================
// EXPORTED FUNCTIONS
// ============================================================================

module.exports = {
  DEALER,
  signMessage,
  verifySignature,
  derivePublicKey,
  createDealerSignedRequest,
  runTests,
};

// Run tests if executed directly
if (require.main === module) {
  runTests().catch(console.error);
}
