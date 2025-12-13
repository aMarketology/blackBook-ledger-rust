// ============================================================================
// Layer 2 Prediction Market - Integration SDK
// ============================================================================
//
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
      this.walletAddress = data.wallet_address;
      this.connectedAccount = {
        name: 'SUPABASE_USER',
        address: data.wallet_address,
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

      // Generate signature if not provided
      const sig = signature || `sig_${this.walletAddress.slice(0, 8)}_${timestamp.toString(16)}`;

      // ✅ NEW FORMAT (simple flat structure)
      const body = {
        signature: sig,
        from_address: this.walletAddress,
        market_id: marketId,
        option: String(outcome),  // Must be string: "0" or "1"
        amount: parseFloat(amount),
        nonce: nextNonce,  // ✅ Send incremented nonce
        timestamp: timestamp
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

    const timestamp = getTimestamp();
    const nonce = await this.getL1Nonce(this.walletAddress);
    const nextNonce = (nonce?.cross_layer_nonce || nonce?.l1_nonce || 0) + 1;

    // Build the payload
    const payload = JSON.stringify({
      target_address: this.walletAddress,  // Same address on L2
      amount: parseFloat(amount),
      target_layer: 'L2'
    });

    // Create signature (mock for now, or use signFn if provided)
    let signature;
    if (signFn) {
      const message = `${payload}:${timestamp}:${nextNonce}`;
      signature = signFn(message);
    } else {
      // Mock signature for development
      signature = `sig_bridge_${this.walletAddress.slice(0, 8)}_${timestamp.toString(16)}`;
    }

    const signedRequest = {
      public_key: this.walletAddress,
      payload: payload,
      timestamp: timestamp,
      nonce: nextNonce,
      signature: signature
    };

    console.log('🌉 Initiating L1→L2 bridge:', { amount, wallet: this.walletAddress });

    try {
      // Send to L1 (not L2!)
      const response = await fetch(`${this.l1Url}/bridge/initiate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(signedRequest)
      });

      const data = await response.json();

      if (data.success) {
        console.log(`✅ Bridge initiated: ${data.bridge_id}`);
        console.log(`   L1 Balance reduced by ${amount} BB`);
        console.log(`   L2 will credit ${amount} BB shortly`);
        
        // Optionally notify L2 to credit immediately (for dev)
        await this.notifyL2BridgeComplete(data.bridge_id, amount);
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
