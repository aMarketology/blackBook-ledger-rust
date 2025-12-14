// ============================================================================
// Layer 2 Prediction Market - Integration SDK
// ============================================================================

import nacl from 'tweetnacl';

// Production-ready SDK for frontend betting integration.
// Supports 2 authentication modes:
//   1. Supabase JWT - Production auth via L1 wallet lookup
//   2. Signed Transactions - Full cryptographic auth with ed25519
//
// Quick Start:
//   import { PredictionMarket } from './layer2_integration_sdk.js';
//   const sdk = new PredictionMarket();
//   await sdk.loginWithSupabase(jwt);
//   await sdk.placeBet('market-id', 0, 50);
//
// Architecture:
//   L1 (Port 8080): Core blockchain - wallets, balances, signatures, sessions
//   L2 (Port 1234): Prediction market - betting, markets, hybrid settlement
//
// Hybrid Settlement Model (Optimistic Execution with Batch Settlement):
//   - L2 executes bets instantly (optimistic)
//   - L1 remains source of truth
//   - Balances have two parts: confirmed (L1) + pending (L2 changes)
//   - Periodic batch settlements sync L2 state to L1
//   - Users can trigger settlement to "claim" winnings to L1
//
// Token: BlackBook (BB) - Stable at $0.01
// ============================================================================


/**
 * Generate a random nonce for replay protection
 */
function generateNonce() {
  const array = new Uint8Array(16);
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(array);
  } else {
    for (let i = 0; i < 16; i++) {
      array[i] = Math.floor(Math.random() * 256);
    }
  }
  return Array.from(array, b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Get current Unix timestamp in seconds
 */
function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

/**
 * Convert hex string to Uint8Array
 */
function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return bytes;
}

/**
 * Convert Uint8Array to hex string
 */
function bytesToHex(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

// ============================================================================
// SDK CLASS
// ============================================================================

class PredictionMarket {
  /**
   * Initialize the SDK
   * @param {Object} config - Configuration options
   * @param {string} config.l1Url - L1 blockchain URL (default: http://localhost:8080)
   * @param {string} config.l2Url - L2 server URL (default: http://localhost:1234)
   */
  constructor(config = {}) {
    this.l1Url = config.l1Url || 'http://localhost:8080';
    this.l2Url = config.l2Url || 'http://localhost:1234';
    
    // Auth state
    this.jwt = null;
    this.connectedAccount = null;
    this.walletAddress = null;
    
    // For signed transactions (advanced)
    this.publicKey = null;
    this.privateKey = null;
  }

  // ==========================================================================
  // CRYPTOGRAPHY - Ed25519 Signing (Production)
  // ==========================================================================

  /**
   * Set private key from hex string and derive public key
   * @param {string} privateKeyHex - 64-char hex private key (32-byte seed)
   */
  setPrivateKey(privateKeyHex) {
    // Validate hex string (32 bytes = 64 hex chars)
    if (!/^[0-9a-fA-F]{64}$/.test(privateKeyHex)) {
      throw new Error('Invalid private key format. Must be 64-character hex string (32 bytes).');
    }

    // Convert hex to Uint8Array
    const seed = hexToBytes(privateKeyHex);

    // Derive keypair from seed
    // nacl.sign.keyPair.fromSeed takes a 32-byte seed and returns { publicKey, secretKey }
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    
    this.privateKey = keyPair.secretKey; // Full 64-byte secret key (seed + public)
    this.publicKey = bytesToHex(keyPair.publicKey); // 32-byte public key as hex
    
    console.log(`✅ Private key set. Public Key: ${this.publicKey}`);
    return this.publicKey;
  }

  /**
   * Generate a new random keypair
   * @returns {Object} { publicKey: hex, privateKey: hex (seed only) }
   */
  generateKeypair() {
    const keyPair = nacl.sign.keyPair();
    
    this.privateKey = keyPair.secretKey;
    this.publicKey = bytesToHex(keyPair.publicKey);
    
    // Return the seed (first 32 bytes of secretKey) for storage
    const seed = keyPair.secretKey.slice(0, 32);
    
    console.log(`✅ New keypair generated. Public Key: ${this.publicKey}`);
    return {
      publicKey: this.publicKey,
      privateKey: bytesToHex(seed) // Return seed for secure storage
    };
  }

  /**
   * Sign a message with the stored private key (Ed25519)
   * @param {string|Uint8Array} message - Message to sign
   * @returns {string} 128-character hex signature (64 bytes)
   */
  signMessage(message) {
    if (!this.privateKey) {
      throw new Error('Private key not set. Call setPrivateKey() or generateKeypair() first.');
    }

    let messageBytes;
    if (typeof message === 'string') {
      messageBytes = new TextEncoder().encode(message);
    } else if (message instanceof Uint8Array) {
      messageBytes = message;
    } else {
      throw new Error('Message must be string or Uint8Array');
    }

    // 🔍 DETAILED DEBUG OUTPUT
    console.log('========================================');
    console.log('[signMessage] 🔏 SIGNING DEBUG:');
    console.log('   Message to sign:', typeof message === 'string' ? message : '(Uint8Array)');
    console.log('   Message bytes length:', messageBytes.length);
    console.log('   Private key length:', this.privateKey.length, '(should be 64)');
    console.log('   Public key (hex):', this.publicKey || 'NOT SET');
    console.log('   Wallet address:', this.walletAddress || 'NOT SET');
    
    const signatureBytes = nacl.sign.detached(messageBytes, this.privateKey);
    const signatureHex = bytesToHex(signatureBytes);
    
    console.log('   Signature (hex):', signatureHex);
    console.log('   Signature length:', signatureHex.length, '(should be 128)');
    
    // 🧪 LOCAL VERIFICATION TEST
    const pubKeyBytes = hexToBytes(this.publicKey);
    const localVerify = nacl.sign.detached.verify(messageBytes, signatureBytes, pubKeyBytes);
    console.log('   🧪 LOCAL VERIFY:', localVerify ? '✅ PASS' : '❌ FAIL');
    
    if (!localVerify) {
      console.error('   ❌ KEY MISMATCH! The publicKey does not match the privateKey!');
      console.error('      This signature WILL BE REJECTED by any server.');
    }
    console.log('========================================');
    
    return signatureHex;
  }

  /**
   * Verify a signature (for testing)
   * @param {string} message - Original message
   * @param {string} signatureHex - 128-char hex signature
   * @param {string} publicKeyHex - 64-char hex public key
   * @returns {boolean} True if valid
   */
  verifySignature(message, signatureHex, publicKeyHex = null) {
    const pubKey = publicKeyHex ? hexToBytes(publicKeyHex) : hexToBytes(this.publicKey);
    const signature = hexToBytes(signatureHex);
    const messageBytes = typeof message === 'string' 
      ? new TextEncoder().encode(message) 
      : message;
    
    return nacl.sign.detached.verify(messageBytes, signature, pubKey);
  }

  // ==========================================================================
  // PATH B: SignedRequest Helpers (Decoupled Identity)
  // ==========================================================================
  //
  // Path B separates:
  // - wallet_address (L1...) = used for balance operations (stored in Supabase)
  // - public_key (64-char hex) = used for signature verification
  //
  // This allows the L1... address to be a short, human-readable identifier
  // while the public_key is used for cryptographic operations.
  // ==========================================================================

  /**
   * Create a SignedRequest object for L1 API calls
   * This is the standard format L1 expects for authenticated requests
   * 
   * Format:
   * {
   *   public_key: "64-char hex",      // For signature verification
   *   wallet_address: "L1...",        // For balance operations (Path B)
   *   payload: "JSON string",         // The actual request data
   *   timestamp: 1234567890,          // Unix timestamp
   *   nonce: "random-string",         // Replay protection
   *   signature: "128-char hex"       // Signs: payload\ntimestamp\nnonce
   * }
   * 
   * @param {Object} payload - The request payload object
   * @returns {Object} SignedRequest object ready to send to L1
   */
  createSignedRequest(payload) {
    if (!this.privateKey || !this.publicKey) {
      throw new Error('Private key not set. Call setPrivateKey() or generateKeypair() first.');
    }

    const timestamp = getTimestamp();
    const nonce = generateNonce();
    const payloadJson = JSON.stringify(payload);
    
    // Create the signed content: payload\ntimestamp\nnonce
    const signedContent = `${payloadJson}\n${timestamp}\n${nonce}`;
    const signature = this.signMessage(signedContent);
    
    return {
      public_key: this.publicKey,           // 64-char hex - for signature verification
      wallet_address: this.walletAddress,   // L1... address - for balance operations (Path B)
      payload: payloadJson,                 // JSON string of the actual request
      timestamp: timestamp,
      nonce: nonce,
      signature: signature                  // Signs: payload\ntimestamp\nnonce
    };
  }

  /**
   * Get the data needed to sync public_key to Supabase
   * Frontend should call this and update Supabase profiles table:
   *   UPDATE profiles SET public_key = ? WHERE blackbook_address = ?
   * 
   * @returns {Object} { wallet_address, public_key } to save to Supabase
   */
  getPublicKeyForSync() {
    if (!this.publicKey) {
      throw new Error('No public key available. Generate or load a keypair first.');
    }
    
    return {
      wallet_address: this.walletAddress,
      public_key: this.publicKey
    };
  }

  // ==========================================================================
  // AUTHENTICATION - Supabase JWT (Production)
  // ==========================================================================

  /**
   * Login with Supabase JWT token
   * Queries L1 for user's registered wallet address
   * @param {string} jwt - Supabase JWT token
   * @param {string} username - Optional username for lookup
   * @returns {Promise<Object>} Login response with wallet and balance
   */
  async loginWithSupabase(jwt, username = null) {
    this.jwt = jwt;
    
    const response = await fetch(`${this.l2Url}/auth/login`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
        // Don't send Authorization header for login - send token in body
      },
      body: JSON.stringify({
        token: jwt,           // Send JWT in body
        username: username    // Optional username for lookup
      })
    });

    // Handle non-OK responses properly
    if (!response.ok) {
      const errorText = await response.text();
      console.error(`❌ L2 login failed (${response.status}): ${errorText}`);
      return {
        success: false,
        error: `Server error ${response.status}: ${errorText}`
      };
    }

    const data = await response.json();

    if (data.success && data.wallet_address) {
      // ⚠️ CRITICAL FIX: Check for Identity Mismatch
      // If we have a local private key, we MUST use its address, otherwise signing fails.
      if (this.publicKey && this.publicKey !== data.wallet_address) {
        console.warn(`⚠️ WALLET MISMATCH DETECTED`);
        console.warn(`   Supabase Account: ${data.wallet_address}`);
        console.warn(`   Local Signing Key: ${this.publicKey}`);
        console.warn(`   -> Using LOCAL KEY to ensure transactions work.`);
        
        this.walletAddress = this.publicKey; // Override with the key we actually possess
      } else {
        this.walletAddress = data.wallet_address;
      }

      this.connectedAccount = {
        name: 'SUPABASE_USER',
        address: this.walletAddress,
        user_id: data.user_id
      };
      console.log(`✅ Logged in: ${data.wallet_address} (${data.balance} BB)`);

      // ✅ Auto-connect to L2 to ensure address is registered
      console.log('🔗 Auto-connecting wallet to L2...');
      const connectResult = await this.connectToL2(data.wallet_address);
      if (connectResult.success) {
        console.log('✅ Wallet registered in L2');
      } else {
        console.warn('⚠️ L2 connect warning (may already exist):', connectResult.error);
      }
    }

    return data;
  }

  /**
   * Login with Supabase JWT and auto-deposit L1 funds to L2 ("Session Deposit" flow)
   * 
   * This provides a seamless UX where:
   * 1. User logs in
   * 2. SDK checks L1 balance
   * 3. If L1 > 0: Automatically bridges all funds to L2
   * 4. User can bet instantly with zero friction
   * 
   * @param {string} jwt - Supabase JWT token
   * @param {string} username - Optional username for lookup
   * @param {Object} options - Options for auto-deposit
   * @param {boolean} options.autoDeposit - Whether to auto-bridge L1 to L2 (default: true)
   * @param {number} options.reserveAmount - Amount to keep in L1 vault (default: 0)
   * @returns {Promise<Object>} Login response with wallet, balance, and bridge info
   */
  async loginWithAutoDeposit(jwt, username = null, options = {}) {
    const { autoDeposit = true, reserveAmount = 0 } = options;

    // Step 1: Standard login
    const loginResult = await this.loginWithSupabase(jwt, username);
    
    if (!loginResult.success || !this.walletAddress) {
      return loginResult;
    }

    // Step 2: Get full balance (L1 + L2)
    const balances = await this.getFullBalance();
    console.log(`💰 Balances - L1 Vault: ${balances.l1_balance} BB, L2 Hot: ${balances.l2_balance} BB`);

    // Step 3: Auto-deposit if enabled and L1 has funds
    let bridgeResult = null;
    if (autoDeposit && balances.l1_balance > reserveAmount) {
      const amountToBridge = balances.l1_balance - reserveAmount;
      console.log(`🌉 Auto-depositing ${amountToBridge} BB from L1 Vault to L2 Hot wallet...`);
      
      bridgeResult = await this.bridgeToL2(amountToBridge);
      
      if (bridgeResult.success) {
        console.log(`✅ Session deposit complete! ${amountToBridge} BB now ready for betting.`);
      } else {
        console.warn(`⚠️ Auto-deposit failed: ${bridgeResult.error}. Manual bridge may be needed.`);
      }
    }

    // Step 4: Get updated unified balance
    const unifiedBalance = await this.getUnifiedBalance();

    return {
      ...loginResult,
      auto_deposit: bridgeResult,
      unified_balance: unifiedBalance,
      ready_to_bet: unifiedBalance.available > 0
    };
  }

  /**
   * Get authenticated user info
   * @returns {Promise<Object>} User info with balance and nonce
   */
  async getUserInfo() {
    if (!this.jwt) {
      throw new Error('Not logged in. Call loginWithSupabase() first.');
    }

    const response = await fetch(`${this.l2Url}/auth/user`, {
      headers: { 'Authorization': `Bearer ${this.jwt}` }
    });

    return response.json();
  }

  // ==========================================================================
  // AUTHENTICATION - Signed Transactions (Advanced)
  // ==========================================================================

  /**
   * Connect with L1 wallet for signed transactions
   * Requires nacl library for signing
   * @param {string} publicKeyHex - 64-char hex public key
   * @param {Uint8Array} privateKey - 64-byte ed25519 private key
   */
  connectWithWallet(publicKeyHex, privateKey) {
    this.publicKey = publicKeyHex;
    this.privateKey = privateKey;
    this.walletAddress = publicKeyHex;
    this.connectedAccount = {
      name: 'L1_WALLET',
      address: publicKeyHex
    };
    console.log(`✅ Connected L1 wallet: ${publicKeyHex.slice(0, 16)}...`);
  }

  /**
   * Create a signed transaction (requires nacl)
   * @param {Object} payload - Transaction payload
   * @param {Function} signFn - Signing function (message => signature)
   * @returns {Object} Signed request body
   */
  createSignedRequest(payload, signFn) {
    if (!this.publicKey) {
      throw new Error('No wallet connected. Call connectWithWallet() first.');
    }

    const timestamp = getTimestamp();
    const nonce = generateNonce();
    
    // Create canonical message
    const message = this.publicKey + JSON.stringify(payload) + timestamp + nonce;
    const signature = signFn(message);
    
    return {
      public_key: this.publicKey,
      payload,
      timestamp,
      nonce,
      signature
    };
  }

  // ==========================================================================
  // AUTH STATE
  // ==========================================================================

  /**
   * Connect wallet address to L2 server
   * This MUST be called before placing bets to register the address
   * POST /auth/connect
   * @param {string} address - Wallet address (defaults to connected wallet)
   * @returns {Promise<Object>} Connection result with balance
   */
  async connectToL2(address = null) {
    const walletAddr = address || this.walletAddress;
    if (!walletAddr) {
      throw new Error('No wallet address. Call loginWithSupabase() or set walletAddress first.');
    }

    console.log(`🔗 Connecting wallet to L2: ${walletAddr}`);

    try {
      const response = await fetch(`${this.l2Url}/auth/connect`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {})
        },
        body: JSON.stringify({
          address: walletAddr,
          wallet_address: walletAddr  // Some backends use this field name
        })
      });

      const text = await response.text();
      console.log(`📥 L2 connect response (${response.status}): ${text}`);

      if (!response.ok) {
        // If 404, the endpoint might not exist - try deposit as fallback
        if (response.status === 404) {
          console.log('⚠️ /auth/connect not found, trying /deposit fallback...');
          return this.ensureL2Balance(walletAddr);
        }
        return { success: false, error: `HTTP ${response.status}: ${text}` };
      }

      let data = {};
      if (text && text.trim()) {
        try {
          data = JSON.parse(text);
        } catch {
          data = { message: text };
        }
      }

      console.log(`✅ Wallet connected to L2: ${walletAddr}`);
      return { success: true, ...data };
    } catch (error) {
      console.error('❌ L2 connect error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Ensure wallet has a balance entry in L2 (fallback method)
   * Uses deposit endpoint to create the address if it doesn't exist
   */
  async ensureL2Balance(address = null) {
    const walletAddr = address || this.walletAddress;
    if (!walletAddr) {
      throw new Error('No wallet address.');
    }

    console.log(`💰 Ensuring L2 balance entry for: ${walletAddr}`);

    try {
      // First check if balance exists
      const balanceResponse = await fetch(`${this.l2Url}/balance/${walletAddr}`);
      
      if (balanceResponse.ok) {
        const balanceData = await balanceResponse.json();
        if (balanceData.balance !== undefined && balanceData.balance !== null) {
          console.log(`✅ L2 balance already exists: ${balanceData.balance}`);
          return { success: true, balance: balanceData.balance, existed: true };
        }
      }

      // Balance doesn't exist - try to create with deposit of 0 (or initial amount)
      console.log('📤 Creating L2 balance entry via deposit...');
      const depositResponse = await fetch(`${this.l2Url}/deposit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: walletAddr,
          amount: 0  // Just create the entry
        })
      });

      if (depositResponse.ok) {
        const depositData = await depositResponse.json();
        console.log(`✅ L2 balance entry created:`, depositData);
        return { success: true, ...depositData };
      }

      // If deposit fails, try admin/mint as last resort
      console.log('⚠️ Deposit failed, trying admin mint...');
      const mintResponse = await fetch(`${this.l2Url}/admin/mint`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: walletAddr,
          amount: 0
        })
      });

      if (mintResponse.ok) {
        console.log('✅ L2 balance created via admin mint');
        return { success: true, method: 'mint' };
      }

      return { success: false, error: 'Could not create L2 balance entry' };
    } catch (error) {
      console.error('❌ ensureL2Balance error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Check if user is authenticated
   */
  isAuthenticated() {
    return !!(this.jwt || this.connectedAccount);
  }

  /**
   * Get current wallet address
   */
  getWalletAddress() {
    return this.walletAddress;
  }

  /**
   * Get connected account name (for test accounts)
   */
  getAccountName() {
    return this.connectedAccount ? this.connectedAccount.name : null;
  }

  /**
   * Logout / disconnect
   */
  logout() {
    this.jwt = null;
    this.connectedAccount = null;
    this.walletAddress = null;
    this.publicKey = null;
    this.privateKey = null;
  }

  // ==========================================================================
  // BETTING ENDPOINTS
  // ==========================================================================

  /**
   * Get nonce for transaction signing
   * GET /rpc/nonce/:address
   * @param {string} address - Wallet address
   * @returns {Promise<number>} Current nonce
   */
  async fetchNonce(address) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified or connected.');
    }

    try {
      const response = await fetch(`${this.l2Url}/rpc/nonce/${addr}`);
      if (!response.ok) {
        console.warn(`⚠️ Failed to fetch nonce: HTTP ${response.status}`);
        return 0; // Default nonce
      }
      
      const text = await response.text();
      if (!text || text.trim() === '') {
        return 0; // Default nonce for empty response
      }
      
      try {
        const data = JSON.parse(text);
        // Ensure we always return a number, not an object
        const nonceValue = data.nonce !== undefined ? data.nonce : data;
        const parsed = parseInt(nonceValue, 10);
        return isNaN(parsed) ? 0 : parsed;
      } catch {
        // Response might be just a number
        const parsed = parseInt(text, 10);
        return isNaN(parsed) ? 0 : parsed;
      }
    } catch (error) {
      console.warn('⚠️ Error fetching nonce:', error);
      return 0;
    }
  }

  /**
   * Place a bet using signed transaction format
   * POST /rpc/submit
   * @param {string} marketId - Market UUID
   * @param {number} outcome - Outcome index (0 = first option, 1 = second, etc.)
   * @param {number} amount - Amount in BB tokens
   * @param {string} signature - Hex signature string (optional, mock for now)
   * @returns {Promise<Object>} Bet response with transaction ID
   */
  async placeBet(marketId, outcome, amount, signature = null) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected. Call loginWithSupabase() first.');
    }

    try {
      // ✅ STEP 1: Ensure wallet is connected to L2 (has balance entry)
      console.log('🔗 Ensuring wallet is connected to L2...');
      const connectResult = await this.connectToL2(this.walletAddress);
      if (!connectResult.success) {
        console.warn('⚠️ L2 connect warning:', connectResult.error);
        // Continue anyway - the address might already exist
      }

      // ==================================================================
      // 🧠 SMART BETTING LOGIC (Auto-Bridge / Optimistic Bridging)
      // ==================================================================
      // Instead of blocking the user, we automatically bridge funds from
      // L1 (Vault) to L2 (Hot) when L2 balance is insufficient.
      // This makes L1 and L2 feel like a single unified balance.
      // ==================================================================
      
      // 1. Check current L2 Balance (Fast)
      const l2Status = await this.getBalance(this.walletAddress);
      const currentL2Balance = l2Status.balance || 0;

      // 2. If we don't have enough chips on the table, check vault
      if (currentL2Balance < amount) {
        const deficit = amount - currentL2Balance;
        console.log(`📉 Low L2 Balance (${currentL2Balance} BB). Need ${amount} BB. Deficit: ${deficit} BB`);

        // 3. Check the Vault (L1)
        const l1Status = await this.getL1Balance(this.walletAddress);
        const currentL1Balance = l1Status.balance || 0;

        // 4. If Vault has enough, auto-bridge!
        if (currentL1Balance >= deficit) {
          console.log(`🏦 Found funds in L1 Vault (${currentL1Balance} BB). Auto-bridging ${deficit} BB...`);
          
          // OPTIMISTIC BRIDGE: Initiate bridge and wait briefly for L2 credit
          // In a full optimistic setup, we'd send the bridge signature with the bet
          // and L2 would credit instantly, trusting L1 confirmation will follow
          const bridgeResult = await this.bridgeToL2(deficit);
          
          if (!bridgeResult.success) {
            return { 
              success: false, 
              error: `Auto-bridge failed: ${bridgeResult.error}`,
              auto_bridge_attempted: true
            };
          }

          // Wait briefly for L2 to credit (simulated optimistic latency)
          // Production: L2 verifies signature is valid and credits instantly
          await new Promise(resolve => setTimeout(resolve, 500));
          console.log('✅ Auto-bridge complete. Proceeding with bet...');
        } else {
          // Truly insufficient funds (neither L1 nor L2 has enough)
          const totalAvailable = currentL1Balance + currentL2Balance;
          return { 
            success: false, 
            error: `Insufficient funds. Available: ${totalAvailable} BB (L2: ${currentL2Balance}, Vault: ${currentL1Balance}). Need: ${amount} BB`,
            l2_balance: currentL2Balance,
            l1_balance: currentL1Balance,
            total_available: totalAvailable,
            amount_needed: amount,
            deficit: amount - totalAvailable
          };
        }
      }

      // ✅ STEP 2: Fetch current nonce from the API
      const fetchedNonce = await this.fetchNonce(this.walletAddress);
      // Ensure nonce is a simple number (u64), not an object
      const currentNonce = parseInt(fetchedNonce, 10);
      if (isNaN(currentNonce)) {
        throw new Error(`Invalid nonce received: ${fetchedNonce}`);
      }
      
      // ✅ FIX: Get current nonce and ADD 1
      const nextNonce = currentNonce + 1;
      const timestamp = getTimestamp();

      // Create the message to sign
      const betMessage = `bet:${this.walletAddress}:${marketId}:${outcome}:${amount}:${timestamp}:${nextNonce}`;

      // Generate signature using real Ed25519 signing
      let sig;
      if (signature) {
        // External signature provided
        sig = signature;
      } else if (this.privateKey) {
        // ✅ PRODUCTION: Use real Ed25519 signing
        sig = this.signMessage(betMessage);
        console.log(`🔏 Bet signed: ${sig.slice(0, 32)}...`);
      } else {
        // ⚠️ DEV FALLBACK: Mock signature (L2 may accept for dev, L1 will reject)
        console.warn('⚠️ No private key set! Using mock signature.');
        sig = '0'.repeat(128);
      }

      // ✅ NEW FORMAT (simple flat structure)
      const body = {
        signature: sig,
        public_key: this.publicKey || '0000000000000000000000000000000000000000000000000000000000000000',
        from_address: this.walletAddress,
        market_id: marketId,
        option: String(outcome),  // Must be string: "0" or "1"
        amount: parseFloat(amount),
        nonce: nextNonce,  // ✅ Send incremented nonce
        timestamp: timestamp,
        payload: betMessage  // Include signed message for verification
      };

      console.log('📤 Placing bet with nonce:', nextNonce);
      console.log('📤 Bet payload:', JSON.stringify(body, null, 2));

      // Send to /bet/signed endpoint
      const response = await fetch(`${this.l2Url}/bet/signed`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {})
        },
        body: JSON.stringify(body)
      });

      // Handle response - check if there's content
      const text = await response.text();
      console.log(`📥 Raw response from /bet/signed: "${text}" (status: ${response.status})`);

      // Parse response if there's content
      let data = {};
      if (text && text.trim() !== '') {
        try {
          data = JSON.parse(text);
        } catch (e) {
          // Response might be a simple string/ID
          data = { tx_id: text.trim() };
        }
      }

      // ✅ STEP 3: Handle "Address not found" error by connecting and retrying
      if (!response.ok && (text.includes('Address not found') || text.includes('not found'))) {
        console.log('🔄 Address not found in L2, attempting to register and retry...');
        
        // Force create the balance entry
        const ensureResult = await this.ensureL2Balance(this.walletAddress);
        if (ensureResult.success) {
          console.log('✅ L2 balance created, retrying bet...');
          
          // Retry the bet with updated nonce
          const retryNonce = currentNonce + 2;  // Increment again
          const retryBody = { ...body, nonce: retryNonce };
          
          const retryResponse = await fetch(`${this.l2Url}/bet/signed`, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {})
            },
            body: JSON.stringify(retryBody)
          });

          const retryText = await retryResponse.text();
          console.log(`📥 Retry response: "${retryText}" (status: ${retryResponse.status})`);

          if (retryResponse.ok) {
            let retryData = {};
            if (retryText && retryText.trim()) {
              try { retryData = JSON.parse(retryText); } catch { retryData = { tx_id: retryText.trim() }; }
            }
            return { 
              success: true, 
              tx_id: retryData.tx_id || retryData.transaction_id || `tx_${Date.now()}`,
              retry: true,
              ...retryData 
            };
          }
        }
      }

      // Check if successful based on HTTP status
      if (response.ok) {
        return { 
          success: true, 
          tx_id: data.tx_id || data.transaction_id || data.hash || `tx_${Date.now()}`,
          ...data 
        };
      }

      // Handle error response
      return { 
        success: false, 
        error: data.error || data.message || `HTTP ${response.status}` 
      };

    } catch (error) {
      console.error('❌ placeBet error:', error);
      return { success: false, error: error.message || 'Network error' };
    }
  }

  /**
   * Place a bet (alias for placeBet)
   * @param {string} marketId - Market UUID
   * @param {number} outcome - Outcome index
   * @param {number} amount - Amount in BB tokens
   */
  async bet(marketId, outcome, amount) {
    return this.placeBet(marketId, outcome, amount);
  }

  /**
   * Get user's bet history
   * GET /bets/:account
   * @param {string} [account] - Account name or address (defaults to connected)
   */
  async getUserBets(account) {
    const addr = account || this.getAccountName() || this.walletAddress;
    if (!addr) {
      throw new Error('No account specified or connected.');
    }

    const response = await fetch(`${this.l2Url}/bets/${addr}`);
    return response.json();
  }

  /**
   * Get all bets for a market
   * GET /markets/:id/bets
   */
  async getMarketBets(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}/bets`);
    return response.json();
  }

  // ==========================================================================
  // MARKET ENDPOINTS
  // ==========================================================================

  /**
   * Get all prediction markets
   * GET /markets
   */
  async getMarkets() {
    const response = await fetch(`${this.l2Url}/markets`);
    return response.json();
  }

  /**
   * Get a specific market
   * GET /markets/:id
   */
  async getMarket(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}`);
    return response.json();
  }

  /**
   * Get market statistics
   * GET /markets/:id/stats
   */
  async getMarketStats(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}/stats`);
    return response.json();
  }

  /**
   * Create a new market
   * POST /markets
   * @param {Object} market - Market config
   * @param {string} market.title - Market title
   * @param {string} market.description - Market description
   * @param {string} market.category - Category (crypto, sports, politics, etc.)
   * @param {string[]} market.options - Betting options (e.g., ["Yes", "No"])
   */
  async createMarket(market) {
    const response = await fetch(`${this.l2Url}/markets`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(market)
    });
    return response.json();
  }

  /**
   * Resolve a market (admin only)
   * POST /resolve/:marketId/:winningOption
   * @param {string} marketId - Market UUID
   * @param {number} winningOption - Winning option index
   */
  async resolveMarket(marketId, winningOption) {
    const response = await fetch(`${this.l2Url}/resolve/${marketId}/${winningOption}`, {
      method: 'POST'
    });
    return response.json();
  }

  /**
   * Get leaderboard (markets with 10+ bettors)
   * GET /leaderboard
   */
  async getLeaderboard() {
    const response = await fetch(`${this.l2Url}/leaderboard`);
    return response.json();
  }

  /**
   * Get leaderboard by category
   * GET /leaderboard/:category
   */
  async getLeaderboardByCategory(category) {
    const response = await fetch(`${this.l2Url}/leaderboard/${category}`);
    return response.json();
  }

  /**
   * Get all market activities
   * GET /activities
   */
  async getActivities() {
    const response = await fetch(`${this.l2Url}/activities`);
    return response.json();
  }

  // ==========================================================================
  // BALANCE & ACCOUNT ENDPOINTS
  // ==========================================================================

  /**
   * Get balance for an address
   * GET /balance/:address
   */
  async getBalance(address) {
    const addr = address || this.walletAddress || (this.connectedAccount && this.connectedAccount.address);
    if (!addr) {
      throw new Error('No address specified or connected.');
    }

    const response = await fetch(`${this.l2Url}/balance/${addr}`);
    return response.json();
  }

  /**
   * Get my balance (shorthand)
   */
  async getMyBalance() {
    return this.getBalance();
  }

  // ==========================================================================
  // HYBRID L1/L2 SETTLEMENT ENDPOINTS
  // ==========================================================================
  //
  // The hybrid model allows instant L2 betting with L1 as source of truth:
  // - Bets execute instantly on L2 (optimistic)
  // - Balance has two parts: confirmed (L1) + pending (L2 changes)
  // - Periodic batch settlements sync L2 changes to L1
  // - Users can manually trigger settlement to claim winnings
  //
  // ==========================================================================

  /**
   * Get detailed balance showing L1 (confirmed) and L2 (pending) breakdown
   * GET /balance/details/:account
   * 
   * Returns:
   *   - confirmed_balance: L1-confirmed balance (source of truth)
   *   - pending_delta: Unsettled L2 changes (+/- from bets)
   *   - available_balance: confirmed + pending (what you can bet with)
   *   - last_l1_sync_slot: Last L1 block synced
   * 
   * @param {string} [address] - Wallet address (defaults to connected)
   * @returns {Promise<Object>} Detailed balance breakdown
   */
  async getBalanceDetails(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified or connected.');
    }

    try {
      const response = await fetch(`${this.l2Url}/balance/details/${addr}`);
      
      if (!response.ok) {
        // Fallback to regular balance endpoint
        const fallback = await this.getBalance(addr);
        return {
          success: true,
          address: addr,
          confirmed_balance: fallback.balance || 0,
          pending_delta: 0,
          available_balance: fallback.balance || 0,
          last_l1_sync_slot: 0,
          last_l1_sync_timestamp: 0,
          is_fallback: true
        };
      }

      const data = await response.json();
      return { success: true, ...data };
    } catch (error) {
      console.error('❌ getBalanceDetails error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Get hybrid balance showing both L1 and L2 states
   * Combines L1 balance query + L2 detailed balance
   * 
   * @returns {Promise<Object>} Combined L1/L2 balance info
   */
  async getHybridBalance() {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    try {
      const [l1Result, l2Details] = await Promise.all([
        this.getL1Balance(this.walletAddress).catch(() => ({ balance: 0 })),
        this.getBalanceDetails(this.walletAddress).catch(() => ({ available_balance: 0 }))
      ]);

      return {
        success: true,
        wallet: this.walletAddress,
        
        // L1 (Vault) - Source of Truth
        l1_balance: l1Result.balance || 0,
        
        // L2 Breakdown
        l2_confirmed: l2Details.confirmed_balance || 0,
        l2_pending: l2Details.pending_delta || 0,
        l2_available: l2Details.available_balance || 0,
        
        // Sync status
        last_sync_slot: l2Details.last_l1_sync_slot || 0,
        
        // Computed totals
        total_confirmed: (l1Result.balance || 0) + (l2Details.confirmed_balance || 0),
        has_pending_changes: (l2Details.pending_delta || 0) !== 0
      };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Trigger batch settlement to L1
   * POST /settle
   * 
   * This submits all pending L2 bets to L1 as a batch.
   * Use this when:
   * - User wants to "claim" their winnings to L1
   * - Periodic settlement (e.g., every hour)
   * - Before a large withdrawal
   * 
   * @returns {Promise<Object>} Settlement result with batch info
   */
  async settleToL1() {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    console.log('📤 Triggering batch settlement to L1...');

    try {
      const response = await fetch(`${this.l2Url}/settle`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {})
        },
        body: JSON.stringify({
          wallet_address: this.walletAddress
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Settlement submitted: batch ${data.batch_id}`);
        console.log(`   Bets settled: ${data.bets_settled || 0}`);
        console.log(`   L1 tx: ${data.l1_tx_hash || 'pending'}`);
      }

      return data;
    } catch (error) {
      console.error('❌ settleToL1 error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Get current settlement status
   * GET /settle/status
   * 
   * Shows:
   * - Pending bets awaiting settlement
   * - Batches submitted to L1
   * - Confirmed/finalized batches
   * - L2 block height
   * 
   * @returns {Promise<Object>} Settlement status
   */
  async getSettlementStatus() {
    try {
      const response = await fetch(`${this.l2Url}/settle/status`);
      
      if (!response.ok) {
        return { success: false, error: `HTTP ${response.status}` };
      }

      const data = await response.json();
      return { success: true, ...data };
    } catch (error) {
      console.error('❌ getSettlementStatus error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Manually sync L2 balances from L1
   * POST /sync
   * 
   * This fetches current L1 balances and updates L2's
   * confirmed_balance to match. Useful after:
   * - L1 deposits
   * - External L1 transfers
   * - Debugging sync issues
   * 
   * @param {string} [address] - Specific address to sync (optional)
   * @returns {Promise<Object>} Sync result
   */
  async syncFromL1(address = null) {
    console.log('🔄 Syncing L2 balances from L1...');

    try {
      const response = await fetch(`${this.l2Url}/sync`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: address || this.walletAddress
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Sync complete: ${data.accounts_synced || 0} accounts updated`);
      }

      return data;
    } catch (error) {
      console.error('❌ syncFromL1 error:', error);
      return { success: false, error: error.message };
    }
  }

  // ==========================================================================
  // L1 OPTIMISTIC SESSIONS (Game Sessions)
  // ==========================================================================
  //
  // Sessions allow users to "lock in" their L1 balance for a gaming session:
  // 1. Start session: Mirror L1 balance to L2 session
  // 2. Play: Bets update session balance (L2 only)
  // 3. Settle: Write net PnL back to L1
  //
  // ==========================================================================

  /**
   * Start an L1 gaming session
   * POST L1:/session/start
   * 
   * @param {number} [amount] - Amount to allocate (default: full balance)
   * @returns {Promise<Object>} Session info
   */
  async startSession(amount = null) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    console.log(`🎮 Starting L1 session for ${this.walletAddress}`);

    try {
      const response = await fetch(`${this.l1Url}/session/start`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          amount: amount,
          timestamp: getTimestamp()
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Session started: ${data.session_id}`);
        console.log(`   L1 Balance: ${data.l1_balance}`);
        console.log(`   Session Balance: ${data.session_balance}`);
      }

      return data;
    } catch (error) {
      console.error('❌ startSession error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Get session status with both L1 and session balances
   * GET L1:/session/status/:address
   * 
   * @param {string} [address] - Wallet address
   * @returns {Promise<Object>} Session status
   */
  async getSessionStatus(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified.');
    }

    try {
      const response = await fetch(`${this.l1Url}/session/status/${addr}`);
      
      if (!response.ok) {
        return { success: false, has_session: false, error: `HTTP ${response.status}` };
      }

      const data = await response.json();
      return { success: true, ...data };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Record a bet result in the session (L2 only)
   * POST L1:/session/bet
   * 
   * @param {string} marketId - Market that was bet on
   * @param {number} pnl - Profit/loss from bet (positive = win)
   * @returns {Promise<Object>} Updated session info
   */
  async recordSessionBet(marketId, pnl) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    try {
      const response = await fetch(`${this.l1Url}/session/bet`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          market_id: marketId,
          pnl: parseFloat(pnl),
          timestamp: getTimestamp()
        })
      });

      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Settle session - write net PnL to L1
   * POST L1:/session/settle
   * 
   * @returns {Promise<Object>} Settlement result
   */
  async settleSession() {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    console.log(`🏁 Settling session for ${this.walletAddress}`);

    try {
      const response = await fetch(`${this.l1Url}/session/settle`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          timestamp: getTimestamp()
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Session settled!`);
        console.log(`   Net PnL: ${data.net_pnl > 0 ? '+' : ''}${data.net_pnl} BB`);
        console.log(`   New L1 Balance: ${data.new_l1_balance}`);
      }

      return data;
    } catch (error) {
      console.error('❌ settleSession error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * List all active sessions
   * GET L1:/session/list
   * 
   * @returns {Promise<Object>} List of active sessions
   */
  async listSessions() {
    try {
      const response = await fetch(`${this.l1Url}/session/list`);
      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Get all accounts
   * GET /accounts
   */
  async getAccounts() {
    const response = await fetch(`${this.l2Url}/accounts`);
    return response.json();
  }

  /**
   * Get nonce for replay protection
   * GET /rpc/nonce/:address
   */
  async getNonce(address) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified or connected.');
    }

    const response = await fetch(`${this.l2Url}/rpc/nonce/${addr}`);
    return response.json();
  }

  /**
   * Deposit funds (admin only)
   * POST /deposit
   */
  async deposit(address, amount) {
    const response = await fetch(`${this.l2Url}/deposit`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address, amount })
    });
    return response.json();
  }

  /**
   * Transfer funds between addresses
   * POST /transfer
   */
  async transfer(from, to, amount) {
    const response = await fetch(`${this.l2Url}/transfer`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ from, to, amount })
    });
    return response.json();
  }

  // ==========================================================================
  // TRANSACTION ENDPOINTS
  // ==========================================================================

  /**
   * Get transaction history for an address
   * GET /transactions/:address
   */
  async getTransactions(address) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified or connected.');
    }

    const response = await fetch(`${this.l2Url}/transactions/${addr}`);
    return response.json();
  }

  /**
   * Get recent transactions
   * GET /transactions/recent
   */
  async getRecentTransactions() {
    const response = await fetch(`${this.l2Url}/transactions/recent`);
    return response.json();
  }

  /**
   * Get all transactions
   * GET /transactions
   */
  async getAllTransactions() {
    const response = await fetch(`${this.l2Url}/transactions`);
    return response.json();
  }

  // ==========================================================================
  // LIVE BETTING ENDPOINTS - Real-time Price Predictions
  // ==========================================================================

  /**
   * Place a live price bet
   * POST /live-bet
   * @param {Object} bet - Live bet config
   * @param {string} bet.bettor - Account name or address
   * @param {string} bet.asset - Asset (BTC, SOL, ETH)
   * @param {string} bet.direction - UP or DOWN
   * @param {number} bet.amount - Bet amount
   * @param {string} bet.timeframe - 1min or 15min
   */
  async placeLiveBet(bet) {
    const response = await fetch(`${this.l2Url}/live-bet`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(bet)
    });
    return response.json();
  }

  /**
   * Quick live bet using connected account
   */
  async liveBet(asset, direction, amount, timeframe = '1min') {
    const bettor = this.getAccountName() || this.walletAddress;
    if (!bettor) {
      throw new Error('Not authenticated.');
    }

    return this.placeLiveBet({
      bettor,
      asset,
      direction,
      amount,
      timeframe
    });
  }

  /**
   * Get active live bets
   * GET /live-bets/active
   */
  async getActiveLiveBets() {
    const response = await fetch(`${this.l2Url}/live-bets/active`);
    return response.json();
  }

  /**
   * Get live bet history
   * GET /live-bets/history/:bettor
   */
  async getLiveBetHistory(bettor) {
    const addr = bettor || this.getAccountName() || this.walletAddress;
    if (!addr) {
      throw new Error('No bettor specified or connected.');
    }

    const response = await fetch(`${this.l2Url}/live-bets/history/${addr}`);
    return response.json();
  }

  /**
   * Check live bet status
   * GET /live-bets/check/:betId
   */
  async checkLiveBetStatus(betId) {
    const response = await fetch(`${this.l2Url}/live-bets/check/${betId}`);
    return response.json();
  }

  // ==========================================================================
  // BRIDGE ENDPOINTS - L1 ↔ L2 Token Movement
  // ==========================================================================
  //
  // FULL FLOW:
  // 1. bridgeToL2(amount)      - Lock tokens on L1, get credited on L2
  // 2. [User places bets on L2]
  // 3. [L2 settles markets periodically via Merkle roots]
  // 4. withdrawToL1(amount)    - Burn on L2, unlock on L1
  //
  // ==========================================================================

  /**
   * STEP 1: Bridge tokens FROM L1 TO L2 (Lock on L1, Credit on L2)
   * 
   * This is the REAL bridge that:
   * 1. Sends a signed transaction to L1 to LOCK tokens
   * 2. L1 creates a pending bridge record
   * 3. L2 polls or gets notified to credit the user
   * 
   * Uses PATH B SignedRequest format:
   * - public_key: used for signature verification
   * - wallet_address: used for balance operations
   * 
   * POST L1:/bridge/initiate
   * @param {number} amount - Amount to bridge to L2
   * @param {Function} signFn - Optional signing function for advanced users
   * @returns {Promise<Object>} Bridge initiation result with bridge_id
   */
  async bridgeToL2(amount, signFn = null) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected. Call loginWithSupabase() first.');
    }

    if (amount <= 0) {
      return { success: false, error: 'Amount must be positive' };
    }

    // ❌ STOPPER: If we don't have a key, don't even try the server.
    if (!this.privateKey || !this.publicKey) {
      console.error('❌ BRIDGE FAILED: No private key loaded.');
      return { 
        success: false, 
        error: 'Wallet not unlocked. Please generate or load a private key to bridge funds.' 
      };
    }

    const timestamp = getTimestamp();
    const nonce = generateNonce();
    
    // ============================================================================
    // PATH B: SignedRequest Format
    // - payload: JSON object with the actual request data
    // - signature signs: payload_json + "\n" + timestamp + "\n" + nonce
    // - L1 uses public_key for verification, wallet_address for balances
    // ============================================================================

    // Create the payload JSON (what we're requesting)
    const payloadObj = {
      target_address: this.walletAddress,  // Same address on L2
      amount: parseFloat(amount),
      target_layer: 'L2'
    };
    const payloadJson = JSON.stringify(payloadObj);
    
    // Create the signed content: payload\ntimestamp\nnonce
    const signedContent = `${payloadJson}\n${timestamp}\n${nonce}`;

    // 🔍 DEBUG: Show what we're signing
    console.log('========================================');
    console.log('[bridgeToL2] 📝 PATH B SIGNATURE:');
    console.log('   payload JSON:', payloadJson);
    console.log('   timestamp:', timestamp);
    console.log('   nonce:', nonce);
    console.log('   ---');
    console.log('   SIGNED CONTENT:');
    console.log('   "' + signedContent.replace(/\n/g, '\\n') + '"');
    console.log('========================================');

    // Create signature
    let signature;
    if (signFn) {
      signature = signFn(signedContent);
    } else {
      signature = this.signMessage(signedContent);
    }

    // ✅ PATH B: SignedRequest format with decoupled identity
    const requestBody = {
      public_key: this.publicKey,           // 64-char hex - for signature verification
      wallet_address: this.walletAddress,   // L1... address - for balance operations
      payload: payloadJson,                 // JSON string of the actual request
      timestamp: timestamp,
      nonce: nonce,
      signature: signature                  // Signs: payload\ntimestamp\nnonce
    };

    console.log('🌉 Initiating L1→L2 bridge (Path B):', {
      wallet_address: this.walletAddress,
      public_key: this.publicKey?.slice(0, 16) + '...',
      amount: amount
    });

    try {
      // Send to L1 (not L2!)
      const response = await fetch(`${this.l1Url}/bridge/initiate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody)
      });

      // Handle response
      const text = await response.text();
      console.log(`📥 L1 bridge response (${response.status}): ${text}`);

      let data;
      try {
        data = JSON.parse(text);
      } catch (e) {
        return { 
          success: false, 
          error: text || `HTTP ${response.status}` 
        };
      }

      if (!response.ok) {
        return { 
          success: false, 
          error: data.error || data.message || `HTTP ${response.status}` 
        };
      }

      if (data.success || data.bridge_id) {
        console.log(`✅ Bridge initiated: ${data.bridge_id}`);
        console.log(`   L1 Balance reduced by ${amount} BB`);
        console.log(`   L2 will credit ${amount} BB shortly`);
        
        // Notify L2 to credit immediately (for dev)
        await this.notifyL2BridgeComplete(data.bridge_id, amount);
        
        return { success: true, ...data };
      }

      return data;
    } catch (error) {
      console.error('❌ bridgeToL2 error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Notify L2 that a bridge is complete (for development/testing)
   * In production, L2 would poll L1 or use webhooks
   */
  async notifyL2BridgeComplete(bridgeId, amount) {
    try {
      // Try to deposit on L2 directly (dev mode)
      const response = await fetch(`${this.l2Url}/deposit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          amount: amount,
          bridge_id: bridgeId
        })
      });
      
      if (response.ok) {
        console.log(`✅ L2 credited ${amount} BB (bridge: ${bridgeId})`);
      }
    } catch (e) {
      // Silent fail - L2 might credit via polling
      console.log('⚠️ L2 notification skipped (will poll)');
    }
  }

  /**
   * STEP 4: Withdraw tokens FROM L2 TO L1 (Burn on L2, Unlock on L1)
   * 
   * This calls L2 to burn tokens, which then calls L1 to unlock them.
   * 
   * Flow:
   * 1. User calls withdrawToL1(amount)
   * 2. SDK calls L2 /withdraw endpoint
   * 3. L2 burns tokens from user's L2 balance
   * 4. L2 calls L1 /bridge/withdraw to unlock tokens
   * 5. User's L1 balance increases
   * 
   * @param {number} amount - Amount to withdraw back to L1
   * @returns {Promise<Object>} Withdrawal result
   */
  async withdrawToL1(amount) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected. Call loginWithSupabase() first.');
    }

    if (amount <= 0) {
      return { success: false, error: 'Amount must be positive' };
    }

    console.log(`🏦 Initiating L2→L1 withdrawal: ${amount} BB`);

    try {
      // Check L2 balance first
      const l2Balance = await this.getBalance(this.walletAddress);
      if (l2Balance.balance < amount) {
        return { 
          success: false, 
          error: `Insufficient L2 balance. Have: ${l2Balance.balance}, Need: ${amount}` 
        };
      }

      // Call L2 withdraw endpoint
      const response = await fetch(`${this.l2Url}/withdraw`, {
        method: 'POST',
        headers: { 
          'Content-Type': 'application/json',
          ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {})
        },
        body: JSON.stringify({
          address: this.walletAddress,
          amount: parseFloat(amount)
        })
      });

      const l2Data = await response.json();

      if (!response.ok || !l2Data.success) {
        return { success: false, error: l2Data.error || 'L2 withdrawal failed' };
      }

      // L2 should have called L1 /bridge/withdraw
      // But let's also call it directly to ensure funds are unlocked
      const l1Response = await fetch(`${this.l1Url}/bridge/withdraw`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user_address: this.walletAddress,
          amount: parseFloat(amount),
          l2_burn_tx: l2Data.tx_id || `l2_burn_${Date.now()}`,
          l2_signature: l2Data.signature || 'dev_mode'
        })
      });

      const l1Data = await l1Response.json();

      if (l1Data.success) {
        console.log(`✅ Withdrawal complete: ${amount} BB unlocked on L1`);
        return {
          success: true,
          withdrawal_id: l1Data.withdrawal_id,
          amount: amount,
          l2_burn: l2Data,
          l1_unlock: l1Data
        };
      } else {
        return { success: false, error: l1Data.error || 'L1 unlock failed' };
      }
    } catch (error) {
      console.error('❌ withdrawToL1 error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Get combined L1 + L2 balances
   * Shows both "locked" (L1) and "available for betting" (L2) balances
   * 
   * @returns {Promise<Object>} Combined balance info
   */
  async getFullBalance() {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    try {
      const [l1, l2] = await Promise.all([
        this.getL1Balance(this.walletAddress).catch(() => ({ balance: 0 })),
        this.getBalance(this.walletAddress).catch(() => ({ balance: 0 }))
      ]);

      return {
        success: true,
        wallet: this.walletAddress,
        l1_balance: l1.balance || 0,        // "Real" tokens on L1
        l2_balance: l2.balance || 0,        // Tokens available for betting
        total: (l1.balance || 0) + (l2.balance || 0)
      };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Get the "Unified Balance" - making L1 and L2 feel like one account.
   * Returns a single object that UI can use to show "Available Funds".
   * 
   * UI should display: Total Balance = Vault (L1) + Hot (L2)
   * This abstracts away the L1/L2 complexity from the user.
   * 
   * @returns {Promise<Object>} Unified balance info
   */
  async getUnifiedBalance() {
    const data = await this.getFullBalance();
    if (!data.success) {
      return { 
        total: 0, 
        available: 0, 
        vault: 0, 
        needs_bridging: false,
        error: data.error 
      };
    }

    return {
      total: data.total,              // Show this big number to the user as "Balance"
      available: data.l2_balance,     // Actual chips on table (ready for instant bets)
      vault: data.l1_balance,         // Funds in L1 that need bridging
      needs_bridging: data.l1_balance > 0, // UI hint: show "funds available to bridge"
      wallet: data.wallet
    };
  }

  /**
   * Get bridge status from L1
   * GET L1:/bridge/status/:bridgeId
   */
  async getBridgeStatusFromL1(bridgeId) {
    const response = await fetch(`${this.l1Url}/bridge/status/${bridgeId}`);
    return response.json();
  }

  /**
   * Get all settlement roots from L1
   * GET L1:/bridge/settlement-roots
   */
  async getSettlementRoots() {
    const response = await fetch(`${this.l1Url}/bridge/settlement-roots`);
    return response.json();
  }

  /**
   * Claim settlement winnings with Merkle proof
   * POST L1:/bridge/claim
   * 
   * @param {string} rootId - Settlement root ID
   * @param {number} amount - Amount to claim
   * @param {string[]} merkleProof - Merkle proof path
   * @param {number} leafIndex - Position in Merkle tree
   */
  async claimSettlement(rootId, amount, merkleProof = [], leafIndex = 0) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    const response = await fetch(`${this.l1Url}/bridge/claim`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        root_id: rootId,
        user_address: this.walletAddress,
        amount: parseFloat(amount),
        merkle_proof: merkleProof,
        leaf_index: leafIndex
      })
    });

    return response.json();
  }

  /**
   * Bridge tokens from L2 to L1 (legacy alias)
   * POST /rpc/bridge
   */
  async bridgeToL1(request) {
    const response = await fetch(`${this.l2Url}/rpc/bridge`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request)
    });
    return response.json();
  }

  /**
   * Get bridge status
   * GET /rpc/bridge/:bridgeId
   */
  async getBridgeStatus(bridgeId) {
    const response = await fetch(`${this.l2Url}/rpc/bridge/${bridgeId}`);
    return response.json();
  }

  /**
   * Get pending bridges
   * GET /bridge/pending
   */
  async getPendingBridges() {
    const response = await fetch(`${this.l2Url}/bridge/pending`);
    return response.json();
  }

  /**
   * Get bridge statistics
   * GET /bridge/stats
   */
  async getBridgeStats() {
    const response = await fetch(`${this.l2Url}/bridge/stats`);
    return response.json();
  }

  /**
   * Get L1 bridge statistics
   * GET L1:/bridge/stats
   */
  async getL1BridgeStats() {
    const response = await fetch(`${this.l1Url}/bridge/stats`);
    return response.json();
  }

  // ==========================================================================
  // AI EVENTS ENDPOINTS
  // ==========================================================================

  /**
   * Get pending AI events
   * GET /events/pending
   */
  async getPendingEvents() {
    const response = await fetch(`${this.l2Url}/events/pending`);
    return response.json();
  }

  /**
   * Launch a pending event as a market
   * POST /events/:eventId/launch
   */
  async launchEvent(eventId) {
    const response = await fetch(`${this.l2Url}/events/${eventId}/launch`, {
      method: 'POST'
    });
    return response.json();
  }

  /**
   * Create an AI event
   * POST /ai/events
   */
  async createAIEvent(event) {
    const response = await fetch(`${this.l2Url}/ai/events`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(event)
    });
    return response.json();
  }

  /**
   * Get AI events RSS feed URL
   */
  getAIEventsFeedUrl() {
    return `${this.l2Url}/ai/events/feed.rss`;
  }

  // ==========================================================================
  // PRICE ENDPOINTS
  // ==========================================================================

  /**
   * Get Bitcoin price
   * GET /bitcoin-price
   */
  async getBitcoinPrice() {
    const response = await fetch(`${this.l2Url}/bitcoin-price`);
    return response.json();
  }

  /**
   * Get Solana price
   * GET /solana-price
   */
  async getSolanaPrice() {
    const response = await fetch(`${this.l2Url}/solana-price`);
    return response.json();
  }

  /**
   * Get all prices
   */
  async getPrices() {
    const [btc, sol] = await Promise.all([
      this.getBitcoinPrice(),
      this.getSolanaPrice()
    ]);
    return { btc, sol };
  }

  // ==========================================================================
  // L1 RPC INTEGRATION
  // ==========================================================================

  /**
   * Get L1 wallet address for a Supabase user
   * GET L1:/auth/wallet/:userId
   */
  async getL1WalletByUserId(userId) {
    const response = await fetch(`${this.l1Url}/auth/wallet/${userId}`);
    if (!response.ok) {
      throw new Error(`L1 wallet not found for user ${userId}. User must register on L1 first.`);
    }
    return response.json();
  }

  /**
   * Verify an L1 signature
   * POST L1:/rpc/verify
   */
  async verifyL1Signature(params) {
    const response = await fetch(`${this.l1Url}/rpc/verify`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    });
    return response.json();
  }

  /**
   * Get L1 balance
   * GET L1:/balance/:address
   */
  async getL1Balance(address) {
    const response = await fetch(`${this.l1Url}/balance/${address}`);
    return response.json();
  }

  /**
   * Get L1 nonce
   * GET L1:/rpc/nonce/:address
   */
  async getL1Nonce(address) {
    const response = await fetch(`${this.l1Url}/rpc/nonce/${address}`);
    return response.json();
  }

  /**
   * Record settlement on L1
   * POST L1:/rpc/settlement
   */
  async recordL1Settlement(settlement) {
    const response = await fetch(`${this.l1Url}/rpc/settlement`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settlement)
    });
    return response.json();
  }

  /**
   * Get L1 health status
   * GET L1:/health
   */
  async getL1Health() {
    const response = await fetch(`${this.l1Url}/health`);
    return response.json();
  }

  /**
   * Get L1 PoH status
   * GET L1:/poh/status
   */
  async getL1PoHStatus() {
    const response = await fetch(`${this.l1Url}/poh/status`);
    return response.json();
  }

  // ==========================================================================
  // HEALTH & LEDGER ENDPOINTS
  // ==========================================================================

  /**
   * Health check
   * GET /health
   */
  async health() {
    const response = await fetch(`${this.l2Url}/health`);
    return response.json();
  }

  /**
   * Check L1 ↔ L2 connection
   */
  async checkConnection() {
    try {
      const [l1, l2] = await Promise.all([
        fetch(`${this.l1Url}/health`).then(r => r.json()).catch(() => null),
        fetch(`${this.l2Url}/health`).then(r => r.json()).catch(() => null)
      ]);

      return {
        connected: !!(l1 && l2),
        l1,
        l2
      };
    } catch (error) {
      return {
        connected: false,
        l1: null,
        l2: null,
        error: error.message
      };
    }
  }

  /**
   * Get ledger stats
   * GET /ledger/stats
   */
  async getLedgerStats() {
    const response = await fetch(`${this.l2Url}/ledger/stats`);
    return response.json();
  }

  /**
   * Get blockchain activity feed (JSON)
   * GET /ledger/json
   */
  async getBlockchainActivity() {
    const response = await fetch(`${this.l2Url}/ledger/json`);
    return response.json();
  }

  /**
   * Get blockchain activity feed (HTML)
   * GET /ledger
   */
  async getBlockchainActivityHTML() {
    const response = await fetch(`${this.l2Url}/ledger`);
    return response.text();
  }

  // ==========================================================================
  // L1 SOCIAL MINING ENDPOINTS
  // ==========================================================================
  //
  // Social Mining rewards users for engagement:
  // - Creating posts (50 BB) + bonuses for likes
  // - Liking posts (10 BB reward)
  // - Daily activity bonuses
  //
  // ==========================================================================

  /**
   * Create a social post on L1 (earns 50 BB)
   * POST L1:/social/post
   * 
   * @param {string} content - Post content (max 280 chars)
   * @param {string} [mediaType] - 'text' | 'image' | 'video'
   * @returns {Promise<Object>} Post result with post_id and reward
   */
  async createSocialPost(content, mediaType = 'text') {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    console.log('📝 Creating L1 social post...');

    try {
      const response = await fetch(`${this.l1Url}/social/post`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          content: content,
          media_type: mediaType,
          timestamp: getTimestamp()
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Post created: ${data.post_id}`);
        console.log(`   Reward: +${data.reward || 50} BB`);
      }

      return data;
    } catch (error) {
      console.error('❌ createSocialPost error:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Like a social post on L1 (earns 10 BB, gives poster bonus)
   * POST L1:/social/like
   * 
   * @param {string} postId - Post ID to like
   * @returns {Promise<Object>} Like result with rewards
   */
  async likeSocialPost(postId) {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    try {
      const response = await fetch(`${this.l1Url}/social/like`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: this.walletAddress,
          post_id: postId,
          timestamp: getTimestamp()
        })
      });

      const data = await response.json();

      if (data.success) {
        console.log(`❤️ Liked post ${postId}`);
        console.log(`   Your reward: +${data.liker_reward || 10} BB`);
        console.log(`   Poster bonus: +${data.poster_bonus || 5} BB`);
      }

      return data;
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Get social stats for an address
   * GET L1:/social/stats/:address
   * 
   * @param {string} [address] - Address to check (defaults to connected)
   * @returns {Promise<Object>} Social mining stats
   */
  async getSocialStats(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) {
      throw new Error('No address specified.');
    }

    try {
      const response = await fetch(`${this.l1Url}/social/stats/${addr}`);
      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Get social feed
   * GET L1:/social/feed
   * 
   * @param {number} [limit=20] - Number of posts to fetch
   * @returns {Promise<Object>} Social feed
   */
  async getSocialFeed(limit = 20) {
    try {
      const response = await fetch(`${this.l1Url}/social/feed?limit=${limit}`);
      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  // ==========================================================================
  // L1 ADVANCED BRIDGE ENDPOINTS
  // ==========================================================================

  /**
   * Initiate bridge directly on L1
   * POST L1:/bridge/initiate
   * 
   * @param {number} amount - Amount to bridge
   * @param {string} targetLayer - 'L2' (from L1 to L2)
   * @returns {Promise<Object>} Bridge result
   */
  async initiateL1Bridge(amount, targetLayer = 'L2') {
    if (!this.walletAddress) {
      throw new Error('No wallet connected.');
    }

    const timestamp = getTimestamp();
    const nonce = await this.getL1Nonce(this.walletAddress);
    const nextNonce = (nonce?.cross_layer_nonce || nonce?.l1_nonce || 0) + 1;

    const payload = JSON.stringify({
      target_address: this.walletAddress,
      amount: parseFloat(amount),
      target_layer: targetLayer
    });

    const signature = `sig_bridge_${this.walletAddress.slice(0, 8)}_${timestamp.toString(16)}`;

    try {
      const response = await fetch(`${this.l1Url}/bridge/initiate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          public_key: this.walletAddress,
          payload: payload,
          timestamp: timestamp,
          nonce: nextNonce,
          signature: signature
        })
      });

      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Get pending bridges from L1
   * GET L1:/bridge/pending/:address
   * 
   * @param {string} [address] - Address to check
   * @returns {Promise<Object>} Pending bridges
   */
  async getL1PendingBridges(address = null) {
    const addr = address || this.walletAddress || 'all';

    try {
      const response = await fetch(`${this.l1Url}/bridge/pending/${addr}`);
      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Complete a pending bridge
   * POST L1:/bridge/complete/:bridgeId
   * 
   * @param {string} bridgeId - Bridge ID to complete
   * @returns {Promise<Object>} Completion result
   */
  async completeL1Bridge(bridgeId) {
    try {
      const response = await fetch(`${this.l1Url}/bridge/complete/${bridgeId}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ timestamp: getTimestamp() })
      });

      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  /**
   * Post a settlement root to L1 (L2 → L1 settlement)
   * POST L1:/bridge/settlement-root
   * 
   * @param {string} merkleRoot - Merkle root hash of settlements
   * @param {Object[]} settlements - Array of {address, amount} pairs
   * @returns {Promise<Object>} Settlement root result
   */
  async postSettlementRoot(merkleRoot, settlements) {
    try {
      const response = await fetch(`${this.l1Url}/bridge/settlement-root`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          merkle_root: merkleRoot,
          settlements: settlements,
          l2_block: Date.now(),
          timestamp: getTimestamp()
        })
      });

      return await response.json();
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  // ==========================================================================
  // ADMIN ENDPOINTS
  // ==========================================================================

  /**
   * Mint tokens (admin only)
   * POST /admin/mint
   */
  async adminMint(address, amount) {
    const response = await fetch(`${this.l2Url}/admin/mint`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address, amount })
    });
    return response.json();
  }

  /**
   * Set balance (admin only)
   * POST /admin/set-balance
   */
  async adminSetBalance(address, balance) {
    const response = await fetch(`${this.l2Url}/admin/set-balance`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address, balance })
    });
    return response.json();
  }

  // ==========================================================================
  // KEY STORAGE HELPERS (Browser localStorage)
  // ==========================================================================

  /**
   * Save the current keypair to localStorage (encrypted with password)
   * WARNING: localStorage is not secure for production. Use a proper wallet.
   * @param {string} password - Password to encrypt the key
   */
  saveKeyToStorage(password = '') {
    if (!this.privateKey) {
      throw new Error('No private key to save. Call generateKeypair() first.');
    }
    
    const seed = bytesToHex(this.privateKey.slice(0, 32)); // Get seed from secretKey
    
    if (typeof localStorage === 'undefined') {
      console.warn('⚠️ localStorage not available (not in browser)');
      return { success: false, error: 'localStorage not available' };
    }

    // Simple XOR "encryption" with password (NOT SECURE - demo only)
    // For production, use WebCrypto API with PBKDF2 + AES-GCM
    let storedValue = seed;
    if (password) {
      const encoder = new TextEncoder();
      const passwordBytes = encoder.encode(password);
      const seedBytes = hexToBytes(seed);
      const encrypted = new Uint8Array(32);
      for (let i = 0; i < 32; i++) {
        encrypted[i] = seedBytes[i] ^ passwordBytes[i % passwordBytes.length];
      }
      storedValue = bytesToHex(encrypted);
    }

    localStorage.setItem('blackbook_wallet_key', storedValue);
    localStorage.setItem('blackbook_wallet_public', this.publicKey);
    console.log('💾 Key saved to localStorage');
    return { success: true, publicKey: this.publicKey };
  }

  /**
   * Load keypair from localStorage
   * @param {string} password - Password to decrypt the key
   */
  loadKeyFromStorage(password = '') {
    if (typeof localStorage === 'undefined') {
      console.warn('⚠️ localStorage not available (not in browser)');
      return { success: false, error: 'localStorage not available' };
    }

    const storedValue = localStorage.getItem('blackbook_wallet_key');
    if (!storedValue) {
      return { success: false, error: 'No key found in storage' };
    }

    let seed = storedValue;
    if (password) {
      const encoder = new TextEncoder();
      const passwordBytes = encoder.encode(password);
      const encryptedBytes = hexToBytes(storedValue);
      const decrypted = new Uint8Array(32);
      for (let i = 0; i < 32; i++) {
        decrypted[i] = encryptedBytes[i] ^ passwordBytes[i % passwordBytes.length];
      }
      seed = bytesToHex(decrypted);
    }

    try {
      this.setPrivateKey(seed);
      console.log('🔑 Key loaded from localStorage');
      return { success: true, publicKey: this.publicKey };
    } catch (e) {
      return { success: false, error: `Invalid key or wrong password: ${e.message}` };
    }
  }

  /**
   * Clear stored keys from localStorage
   */
  clearStoredKey() {
    if (typeof localStorage !== 'undefined') {
      localStorage.removeItem('blackbook_wallet_key');
      localStorage.removeItem('blackbook_wallet_public');
      console.log('🗑️ Stored keys cleared');
    }
    this.privateKey = null;
    this.publicKey = null;
  }
}

// ============================================================================
// EXPORTS
// ============================================================================

// ES Module exports
export { PredictionMarket, generateNonce, getTimestamp, hexToBytes, bytesToHex };
export default PredictionMarket;

// CommonJS (for Node.js environments)
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { PredictionMarket, generateNonce, getTimestamp, hexToBytes, bytesToHex };
}

// Browser global
if (typeof window !== 'undefined') {
  window.PredictionMarket = PredictionMarket;
}
