// ============================================================================
// BLACKBOOK LAYER 2 PREDICTION MARKET - INTEGRATION SDK
// ============================================================================
//
// Complete SDK for integrating with the BlackBook L2 Prediction Market.
// This SDK is designed for use by:
//   1. Frontend Applications (Bun/Tauri, React, etc.)
//   2. L1 Backend (for cross-layer operations)
//   3. Oracle/Admin systems (for market resolution)
//
// ARCHITECTURE:
//   ┌─────────────────────────────────────────────────────────────────────┐
//   │                        BLACKBOOK ECOSYSTEM                          │
//   ├─────────────────────────────────────────────────────────────────────┤
//   │                                                                      │
//   │   ┌─────────────┐         ┌─────────────────────────────────────┐  │
//   │   │   FRONTEND  │         │            L2 PREDICTION MARKET     │  │
//   │   │  (Tauri/Bun)│◄───────►│  Port 1234                          │  │
//   │   └─────────────┘   HTTP  │  • Markets, Bets, Shares            │  │
//   │                           │  • CLOB Orderbook                   │  │
//   │                           │  • Oracle Resolution                │  │
//   │   ┌─────────────┐         │  • Bridge L1↔L2                     │  │
//   │   │  L1 CHAIN   │◄───────►│                                     │  │
//   │   │  Port 8080  │   RPC   └─────────────────────────────────────┘  │
//   │   │  • Balances │                                                   │
//   │   │  • PoH/Slots│                                                   │
//   │   │  • Sessions │                                                   │
//   │   └─────────────┘                                                   │
//   │                                                                      │
//   └─────────────────────────────────────────────────────────────────────┘
//
// TOKEN: BlackBook (BB) - Pegged at $0.01 USD
//
// ============================================================================

import nacl from 'tweetnacl';

// ============================================================================
// CONFIGURATION
// ============================================================================

const DEFAULT_CONFIG = {
  L1_URL: 'http://localhost:8080',
  L2_URL: 'http://localhost:1234',
  TIMEOUT: 30000,
  RETRY_ATTEMPTS: 3,
  RETRY_DELAY: 1000,
};

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/**
 * Generate cryptographically secure random nonce
 * @returns {string} 32-character hex nonce
 */
function generateNonce() {
  if (typeof crypto === 'undefined' || !crypto.getRandomValues) {
    throw new Error('Secure random number generator required. Use modern browser or Node.js 19+');
  }
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return Array.from(array, b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Get current Unix timestamp in seconds
 * @returns {number}
 */
function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

/**
 * Convert hex string to Uint8Array
 * @param {string} hex 
 * @returns {Uint8Array}
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
 * @param {Uint8Array} bytes 
 * @returns {string}
 */
function bytesToHex(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

// ============================================================================
// L2 CLIENT - Main SDK Class
// ============================================================================

/**
 * BlackBook L2 Prediction Market Client
 * 
 * @example
 * // Frontend Usage (Tauri/Bun)
 * const l2 = new L2Client();
 * await l2.connect('L1ALICE000000001');
 * await l2.placeBet('market-123', 0, 100); // Bet 100 BB on option 0
 * 
 * @example
 * // With Ed25519 Signing (Production)
 * const l2 = new L2Client();
 * l2.setPrivateKey('your-64-char-hex-seed');
 * await l2.connect();
 * await l2.placeSignedBet('market-123', 0, 100);
 */
export class L2Client {
  constructor(config = {}) {
    this.l1Url = config.l1Url || DEFAULT_CONFIG.L1_URL;
    this.l2Url = config.l2Url || DEFAULT_CONFIG.L2_URL;
    this.timeout = config.timeout || DEFAULT_CONFIG.TIMEOUT;
    
    // Authentication state
    this.walletAddress = null;
    this.publicKey = null;
    this.privateKey = null;
    this.jwt = null;
    
    // Debug mode
    this.debug = config.debug || false;
  }

  _log(...args) {
    if (this.debug) console.log('[L2Client]', ...args);
  }

  // ==========================================================================
  // HTTP HELPERS
  // ==========================================================================

  async _fetch(endpoint, options = {}) {
    const url = endpoint.startsWith('http') ? endpoint : `${this.l2Url}${endpoint}`;
    this._log('Fetching:', url);
    
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);
    
    try {
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
        headers: {
          'Content-Type': 'application/json',
          ...(this.jwt ? { 'Authorization': `Bearer ${this.jwt}` } : {}),
          ...options.headers,
        },
      });
      
      clearTimeout(timeoutId);
      const text = await response.text();
      
      let data;
      try {
        data = text ? JSON.parse(text) : {};
      } catch {
        data = { raw: text };
      }
      
      if (!response.ok) {
        throw new Error(data.error || data.message || `HTTP ${response.status}: ${text}`);
      }
      
      return data;
    } catch (error) {
      clearTimeout(timeoutId);
      if (error.name === 'AbortError') {
        throw new Error(`Request timeout after ${this.timeout}ms`);
      }
      throw error;
    }
  }

  async _post(endpoint, body) {
    return this._fetch(endpoint, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async _get(endpoint) {
    return this._fetch(endpoint);
  }

  // ==========================================================================
  // CRYPTOGRAPHY - Ed25519 Signing
  // ==========================================================================

  /**
   * Set private key from 64-character hex seed
   * @param {string} privateKeyHex - 32-byte seed as 64 hex characters
   * @returns {string} Derived public key (64 hex characters)
   */
  setPrivateKey(privateKeyHex) {
    if (!/^[0-9a-fA-F]{64}$/.test(privateKeyHex)) {
      throw new Error('Invalid private key: must be 64 hex characters (32 bytes)');
    }
    
    const seed = hexToBytes(privateKeyHex);
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    
    this.privateKey = keyPair.secretKey;
    this.publicKey = bytesToHex(keyPair.publicKey);
    this.walletAddress = this.publicKey; // Can be overridden
    
    this._log('Private key set. Public key:', this.publicKey.slice(0, 16) + '...');
    return this.publicKey;
  }

  /**
   * Generate a new random keypair
   * @returns {{ publicKey: string, privateKey: string }}
   */
  generateKeypair() {
    const keyPair = nacl.sign.keyPair();
    
    this.privateKey = keyPair.secretKey;
    this.publicKey = bytesToHex(keyPair.publicKey);
    this.walletAddress = this.publicKey;
    
    const seed = bytesToHex(keyPair.secretKey.slice(0, 32));
    
    this._log('New keypair generated. Public key:', this.publicKey.slice(0, 16) + '...');
    return { publicKey: this.publicKey, privateKey: seed };
  }

  /**
   * Sign a message with Ed25519
   * @param {string} message - Message to sign
   * @returns {string} 128-character hex signature
   */
  sign(message) {
    if (!this.privateKey) {
      throw new Error('No private key. Call setPrivateKey() or generateKeypair() first.');
    }
    
    const messageBytes = new TextEncoder().encode(message);
    const signature = nacl.sign.detached(messageBytes, this.privateKey);
    return bytesToHex(signature);
  }

  /**
   * Verify a signature
   * @param {string} message 
   * @param {string} signatureHex 
   * @param {string} publicKeyHex 
   * @returns {boolean}
   */
  verify(message, signatureHex, publicKeyHex = null) {
    const pubKey = hexToBytes(publicKeyHex || this.publicKey);
    const signature = hexToBytes(signatureHex);
    const messageBytes = new TextEncoder().encode(message);
    return nacl.sign.detached.verify(messageBytes, signature, pubKey);
  }

  // ==========================================================================
  // CONNECTION & AUTHENTICATION
  // ==========================================================================

  /**
   * Connect wallet to L2
   * @param {string} [address] - Wallet address (uses publicKey if not provided)
   * @returns {Promise<Object>}
   */
  async connect(address = null) {
    this.walletAddress = address || this.publicKey || this.walletAddress;
    
    if (!this.walletAddress) {
      throw new Error('No wallet address. Provide address or call setPrivateKey() first.');
    }
    
    this._log('Connecting wallet:', this.walletAddress);
    
    try {
      const result = await this._post('/auth/connect', {
        address: this.walletAddress,
        public_key: this.publicKey,
        timestamp: getTimestamp(),
      });
      
      this._log('Connected:', result);
      return { success: true, ...result };
    } catch (error) {
      // Try deposit endpoint as fallback to create address
      try {
        await this._post('/deposit', { address: this.walletAddress, amount: 0 });
        return { success: true, created: true };
      } catch {
        return { success: false, error: error.message };
      }
    }
  }

  /**
   * Login with Supabase JWT
   * @param {string} jwt - Supabase JWT token
   * @param {string} [username] - Optional username
   * @returns {Promise<Object>}
   */
  async loginWithJwt(jwt, username = null) {
    this.jwt = jwt;
    
    const result = await this._post('/auth/login', {
      token: jwt,
      username: username,
    });
    
    if (result.success && result.wallet_address) {
      this.walletAddress = result.wallet_address;
    }
    
    return result;
  }

  /**
   * Check if authenticated
   * @returns {boolean}
   */
  isConnected() {
    return !!this.walletAddress;
  }

  /**
   * Disconnect / logout
   */
  disconnect() {
    this.walletAddress = null;
    this.jwt = null;
  }

  // ==========================================================================
  // BALANCE ENDPOINTS
  // ==========================================================================

  /**
   * Get L2 balance for an address
   * @param {string} [address] - Defaults to connected wallet
   * @returns {Promise<{ balance: number, address: string }>}
   */
  async getBalance(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/balance/${addr}`);
  }

  /**
   * Get L1 balance (queries L1 directly)
   * @param {string} [address] 
   * @returns {Promise<{ balance: number }>}
   */
  async getL1Balance(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    
    const response = await fetch(`${this.l1Url}/balance/${addr}`);
    return response.json();
  }

  /**
   * Get combined L1 + L2 balance
   * @returns {Promise<Object>}
   */
  async getFullBalance() {
    if (!this.walletAddress) throw new Error('Not connected');
    
    const [l1, l2] = await Promise.all([
      this.getL1Balance().catch(() => ({ balance: 0 })),
      this.getBalance().catch(() => ({ balance: 0 })),
    ]);
    
    return {
      wallet: this.walletAddress,
      l1_balance: l1.balance || 0,
      l2_balance: l2.balance || 0,
      total: (l1.balance || 0) + (l2.balance || 0),
    };
  }

  /**
   * Get unified balance (abstracts L1/L2 complexity)
   * @returns {Promise<{ total: number, available: number, vault: number }>}
   */
  async getUnifiedBalance() {
    const full = await this.getFullBalance();
    return {
      total: full.total,
      available: full.l2_balance,  // Ready for betting
      vault: full.l1_balance,      // In L1, needs bridging
      needs_bridging: full.l1_balance > 0,
    };
  }

  // ==========================================================================
  // MARKET ENDPOINTS
  // ==========================================================================

  /**
   * Get all prediction markets
   * @returns {Promise<Object>}
   */
  async getMarkets() {
    return this._get('/markets');
  }

  /**
   * Get a specific market by ID
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getMarket(marketId) {
    return this._get(`/markets/${marketId}`);
  }

  /**
   * Get market statistics
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getMarketStats(marketId) {
    return this._get(`/markets/${marketId}/stats`);
  }

  /**
   * Create a new prediction market
   * @param {Object} market 
   * @param {string} market.title - Market question
   * @param {string} market.description - Detailed description
   * @param {string} market.category - Category (crypto, sports, politics, etc.)
   * @param {string[]} market.options - Betting options (e.g., ["Yes", "No"])
   * @param {number} [market.end_time] - Unix timestamp when market closes
   * @returns {Promise<Object>}
   * 
   * @example
   * await l2.createMarket({
   *   title: "Will BTC reach $100k by EOY?",
   *   description: "Bitcoin price prediction",
   *   category: "crypto",
   *   options: ["Yes", "No"],
   *   end_time: 1735689600
   * });
   */
  async createMarket(market) {
    return this._post('/markets', market);
  }

  /**
   * Get market leaderboard (markets with 10+ bettors)
   * @returns {Promise<Object>}
   */
  async getLeaderboard() {
    return this._get('/leaderboard');
  }

  // ==========================================================================
  // BETTING ENDPOINTS
  // ==========================================================================

  /**
   * Get current nonce for replay protection
   * @param {string} [address] 
   * @returns {Promise<number>}
   */
  async getNonce(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    
    const result = await this._get(`/rpc/nonce/${addr}`);
    return typeof result === 'number' ? result : (result.nonce || 0);
  }

  /**
   * Place a signed bet (production method)
   * @param {string} marketId - Market UUID
   * @param {number} outcome - Option index (0 = first option, 1 = second, etc.)
   * @param {number} amount - Amount in BB tokens
   * @returns {Promise<Object>}
   * 
   * @example
   * // Bet 100 BB on "Yes" (option 0)
   * const result = await l2.placeSignedBet('market-123', 0, 100);
   * console.log(result.tx_id);
   */
  async placeSignedBet(marketId, outcome, amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    if (!this.privateKey) throw new Error('No private key for signing');
    
    const nonce = await this.getNonce() + 1;
    const timestamp = getTimestamp();
    
    // Create signing message
    const message = `bet:${this.walletAddress}:${marketId}:${outcome}:${amount}:${timestamp}:${nonce}`;
    const signature = this.sign(message);
    
    const body = {
      signature,
      public_key: this.publicKey,
      from_address: this.walletAddress,
      market_id: marketId,
      option: String(outcome),
      amount: parseFloat(amount),
      nonce,
      timestamp,
      payload: message,
    };
    
    this._log('Placing bet:', body);
    return this._post('/bet/signed', body);
  }

  /**
   * Place a simple bet (development/testing)
   * @param {string} marketId 
   * @param {number} outcome 
   * @param {number} amount 
   * @returns {Promise<Object>}
   */
  async placeBet(marketId, outcome, amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    // If we have a private key, use signed betting
    if (this.privateKey) {
      return this.placeSignedBet(marketId, outcome, amount);
    }
    
    // Simple bet for development
    return this._post('/bet', {
      bettor: this.walletAddress,
      market_id: marketId,
      option: outcome,
      amount: parseFloat(amount),
    });
  }

  /**
   * Get user's bet history
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getBets(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/bets/${addr}`);
  }

  /**
   * Get all bets on a market
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getMarketBets(marketId) {
    return this._get(`/markets/${marketId}/bets`);
  }

  // ==========================================================================
  // SHARES SYSTEM
  // ==========================================================================

  /**
   * Mint outcome shares (1 BB = 1 YES + 1 NO share)
   * @param {string} marketId 
   * @param {number} amount - Amount of BB to convert to shares
   * @returns {Promise<Object>}
   * 
   * @example
   * // Convert 100 BB into 100 YES + 100 NO shares
   * await l2.mintShares('market-123', 100);
   */
  async mintShares(marketId, amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/shares/mint', {
      wallet: this.walletAddress,
      market_id: marketId,
      amount: parseFloat(amount),
    });
  }

  /**
   * Redeem share pairs back to BB (1 YES + 1 NO = 1 BB)
   * @param {string} marketId 
   * @param {number} amount 
   * @returns {Promise<Object>}
   */
  async redeemShares(marketId, amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/shares/redeem', {
      wallet: this.walletAddress,
      market_id: marketId,
      amount: parseFloat(amount),
    });
  }

  /**
   * Get share position for a market
   * @param {string} marketId 
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getSharePosition(marketId, address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/shares/position/${addr}/${marketId}`);
  }

  /**
   * Get all share positions for an address
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getAllSharePositions(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/shares/positions/${addr}`);
  }

  // ==========================================================================
  // ORDERBOOK (CLOB) ENDPOINTS
  // ==========================================================================

  /**
   * Place a limit order on the CLOB
   * @param {Object} order 
   * @param {string} order.market_id - Market UUID
   * @param {string} order.side - 'buy' or 'sell'
   * @param {string} order.outcome - 'yes' or 'no'
   * @param {number} order.price - Price (0.01 - 0.99)
   * @param {number} order.quantity - Number of shares
   * @returns {Promise<Object>}
   * 
   * @example
   * // Buy 50 YES shares at $0.65
   * await l2.placeLimitOrder({
   *   market_id: 'market-123',
   *   side: 'buy',
   *   outcome: 'yes',
   *   price: 0.65,
   *   quantity: 50
   * });
   */
  async placeLimitOrder(order) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/orderbook/order', {
      wallet: this.walletAddress,
      ...order,
    });
  }

  /**
   * Cancel an open order
   * @param {string} orderId 
   * @returns {Promise<Object>}
   */
  async cancelOrder(orderId) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/orderbook/cancel', {
      wallet: this.walletAddress,
      order_id: orderId,
    });
  }

  /**
   * Get orderbook for a market
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getOrderbook(marketId) {
    return this._get(`/orderbook/${marketId}`);
  }

  /**
   * Get user's open orders
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getOpenOrders(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/orderbook/orders/${addr}`);
  }

  /**
   * Get market odds (from CLOB + CPMM)
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getMarketOdds(marketId) {
    return this._get(`/markets/${marketId}/odds`);
  }

  // ==========================================================================
  // MARKET RESOLUTION (Oracle/Admin)
  // ==========================================================================

  /**
   * Resolve a market (Oracle only)
   * @param {string} marketId 
   * @param {number} winningOutcome - Index of winning option
   * @param {string} [reason] - Resolution reason
   * @returns {Promise<Object>}
   * 
   * @example
   * // Resolve market with "Yes" winning (option 0)
   * await l2.resolveMarket('market-123', 0, 'BTC reached $100k on Dec 15');
   */
  async resolveMarket(marketId, winningOutcome, reason = null) {
    if (!this.walletAddress) throw new Error('Not connected');
    if (!this.privateKey) throw new Error('Resolution requires signing');
    
    const nonce = await this.getNonce() + 1;
    const timestamp = getTimestamp();
    
    // Create resolution message
    const message = `resolve:${marketId}:${winningOutcome}:${timestamp}:${nonce}`;
    const signature = this.sign(message);
    
    return this._post(`/markets/${marketId}/resolve`, {
      resolver_address: this.walletAddress,
      winning_outcome: winningOutcome,
      resolution_reason: reason,
      timestamp,
      nonce,
      signature,
    });
  }

  /**
   * Admin shortcut to resolve market (requires admin privileges)
   * @param {string} marketId 
   * @param {number} winningOutcome 
   * @returns {Promise<Object>}
   */
  async adminResolveMarket(marketId, winningOutcome) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post(`/admin/resolve/${marketId}/${winningOutcome}`, {
      admin_address: this.walletAddress,
    });
  }

  /**
   * Get resolution details for a market
   * @param {string} marketId 
   * @returns {Promise<Object>}
   */
  async getMarketResolution(marketId) {
    return this._get(`/markets/${marketId}/resolution`);
  }

  /**
   * Claim winnings from a resolved market
   * @param {string} marketId 
   * @returns {Promise<Object>}
   * 
   * @example
   * // Claim winnings after market resolves
   * const resolution = await l2.getMarketResolution('market-123');
   * if (resolution.resolved) {
   *   const claim = await l2.claimWinnings('market-123');
   *   console.log(`Claimed ${claim.bb_received} BB`);
   * }
   */
  async claimWinnings(marketId) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post(`/shares/claim/${marketId}`, {
      wallet: this.walletAddress,
    });
  }

  // ==========================================================================
  // ORACLE MANAGEMENT (Admin)
  // ==========================================================================

  /**
   * Add an oracle address (Admin only)
   * @param {string} oracleAddress 
   * @returns {Promise<Object>}
   */
  async addOracle(oracleAddress) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/oracles/add', {
      admin_address: this.walletAddress,
      oracle_address: oracleAddress,
    });
  }

  /**
   * Remove an oracle address (Admin only)
   * @param {string} oracleAddress 
   * @returns {Promise<Object>}
   */
  async removeOracle(oracleAddress) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/oracles/remove', {
      admin_address: this.walletAddress,
      oracle_address: oracleAddress,
    });
  }

  /**
   * List all authorized oracles
   * @returns {Promise<Object>}
   */
  async listOracles() {
    return this._get('/oracles');
  }

  // ==========================================================================
  // L1 SETTLEMENT
  // ==========================================================================

  /**
   * Submit market settlements to L1
   * @param {string} [marketId] - Specific market, or null for all pending
   * @returns {Promise<Object>}
   */
  async settleToL1(marketId = null) {
    return this._post('/settle/l1', {
      market_id: marketId,
    });
  }

  /**
   * Get markets pending L1 settlement
   * @returns {Promise<Object>}
   */
  async getPendingSettlements() {
    return this._get('/settle/pending');
  }

  // ==========================================================================
  // BRIDGE ENDPOINTS (L1 ↔ L2)
  // ==========================================================================

  /**
   * Initiate bridge from L1 to L2 (deposit)
   * @param {number} amount - Amount to bridge
   * @returns {Promise<Object>}
   * 
   * @example
   * // Bridge 1000 BB from L1 vault to L2 for betting
   * const result = await l2.bridgeDeposit(1000);
   * console.log(`Bridge ID: ${result.bridge_id}`);
   */
  async bridgeDeposit(amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    if (!this.privateKey) throw new Error('Bridge requires signing');
    
    const nonce = await this.getNonce() + 1;
    const timestamp = getTimestamp();
    
    const payload = JSON.stringify({
      target_address: this.walletAddress,
      amount: parseFloat(amount),
      target_layer: 'L2',
    });
    
    const signedContent = `${payload}\n${timestamp}\n${nonce}`;
    const signature = this.sign(signedContent);
    
    // Send to L1 bridge endpoint
    const response = await fetch(`${this.l1Url}/bridge/initiate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        public_key: this.publicKey,
        wallet_address: this.walletAddress,
        payload,
        timestamp,
        nonce,
        signature,
      }),
    });
    
    return response.json();
  }

  /**
   * Initiate bridge from L2 to L1 (withdraw)
   * @param {number} amount - Amount to withdraw
   * @param {string} [targetAddress] - L1 address (defaults to wallet)
   * @returns {Promise<Object>}
   */
  async bridgeWithdraw(amount, targetAddress = null) {
    if (!this.walletAddress) throw new Error('Not connected');
    if (!this.privateKey) throw new Error('Bridge requires signing');
    
    const target = targetAddress || this.walletAddress;
    const nonce = await this.getNonce() + 1;
    const timestamp = getTimestamp();
    
    const message = `withdraw:${this.walletAddress}:${target}:${amount}:${timestamp}:${nonce}`;
    const signature = this.sign(message);
    
    return this._post('/bridge/withdraw', {
      wallet: this.walletAddress,
      target_address: target,
      amount: parseFloat(amount),
      nonce,
      timestamp,
      signature,
    });
  }

  /**
   * Complete a deposit bridge (L1 → L2)
   * @param {Object} bridgeData 
   * @returns {Promise<Object>}
   */
  async completeBridgeDeposit(bridgeData) {
    return this._post('/bridge/deposit', bridgeData);
  }

  /**
   * Get bridge transaction status
   * @param {string} bridgeId 
   * @returns {Promise<Object>}
   */
  async getBridgeStatus(bridgeId) {
    return this._get(`/bridge/status/${bridgeId}`);
  }

  /**
   * List bridges for a wallet
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getWalletBridges(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/bridge/wallet/${addr}`);
  }

  /**
   * Get bridge statistics
   * @returns {Promise<Object>}
   */
  async getBridgeStats() {
    return this._get('/bridge/stats');
  }

  // ==========================================================================
  // TRANSACTIONS & LEDGER
  // ==========================================================================

  /**
   * Get transactions for an address
   * @param {string} [address] 
   * @returns {Promise<Object>}
   */
  async getTransactions(address = null) {
    const addr = address || this.walletAddress;
    if (!addr) throw new Error('No address specified');
    return this._get(`/transactions/${addr}`);
  }

  /**
   * Get ledger activity
   * @returns {Promise<Object>}
   */
  async getLedgerActivity() {
    return this._get('/ledger/json');
  }

  /**
   * Get ledger statistics
   * @returns {Promise<Object>}
   */
  async getLedgerStats() {
    return this._get('/ledger/stats');
  }

  /**
   * Transfer tokens between addresses
   * @param {string} to - Recipient address
   * @param {number} amount 
   * @returns {Promise<Object>}
   */
  async transfer(to, amount) {
    if (!this.walletAddress) throw new Error('Not connected');
    
    return this._post('/transfer', {
      from: this.walletAddress,
      to,
      amount: parseFloat(amount),
    });
  }

  // ==========================================================================
  // HEALTH & STATUS
  // ==========================================================================

  /**
   * Check L2 health
   * @returns {Promise<boolean>}
   */
  async health() {
    try {
      const response = await fetch(`${this.l2Url}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Check both L1 and L2 connectivity
   * @returns {Promise<Object>}
   */
  async checkConnection() {
    const [l1, l2] = await Promise.all([
      fetch(`${this.l1Url}/health`).then(r => r.ok).catch(() => false),
      fetch(`${this.l2Url}/health`).then(r => r.ok).catch(() => false),
    ]);
    
    return { l1_connected: l1, l2_connected: l2, fully_connected: l1 && l2 };
  }

  /**
   * Get all accounts (development)
   * @returns {Promise<Object>}
   */
  async getAccounts() {
    return this._get('/accounts');
  }

  /**
   * Get activity feed
   * @returns {Promise<Object>}
   */
  async getActivities() {
    return this._get('/activities');
  }
}

// ============================================================================
// L1 RPC CLIENT - For L2 Server to communicate with L1
// ============================================================================

/**
 * L1 RPC Client for cross-layer operations
 * Used by L2 backend to verify signatures, record settlements, etc.
 */
export class L1RpcClient {
  constructor(l1Url = DEFAULT_CONFIG.L1_URL, options = {}) {
    this.l1Url = l1Url;
    this.debug = options.debug || false;
  }

  _log(...args) {
    if (this.debug) console.log('[L1RPC]', ...args);
  }

  async _post(endpoint, body) {
    const response = await fetch(`${this.l1Url}${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return response.json();
  }

  async _get(endpoint) {
    const response = await fetch(`${this.l1Url}${endpoint}`);
    return response.json();
  }

  /**
   * Verify an Ed25519 signature via L1
   * @param {string} publicKey 
   * @param {string} message 
   * @param {string} signature 
   * @returns {Promise<{ valid: boolean, wallet_address: string }>}
   */
  async verifySignature(publicKey, message, signature) {
    return this._post('/rpc/verify-signature', {
      public_key: publicKey,
      message,
      signature,
    });
  }

  /**
   * Get cross-layer nonce
   * @param {string} address 
   * @returns {Promise<Object>}
   */
  async getNonce(address) {
    return this._get(`/rpc/nonce/${address}`);
  }

  /**
   * Get L1 balance
   * @param {string} address 
   * @returns {Promise<number>}
   */
  async getBalance(address) {
    const result = await this._get(`/balance/${address}`);
    return result.balance || 0;
  }

  /**
   * Record market settlement on L1
   * @param {Object} settlement 
   * @returns {Promise<Object>}
   */
  async recordSettlement(settlement) {
    return this._post('/rpc/settlement', settlement);
  }

  /**
   * Process withdrawal from L2 to L1
   * @param {string} userAddress 
   * @param {number} amount 
   * @param {string} l2BurnTx 
   * @returns {Promise<Object>}
   */
  async processWithdrawal(userAddress, amount, l2BurnTx) {
    return this._post('/bridge/withdraw', {
      user_address: userAddress,
      amount,
      l2_burn_tx: l2BurnTx,
    });
  }

  /**
   * Health check
   * @returns {Promise<boolean>}
   */
  async health() {
    try {
      const response = await fetch(`${this.l1Url}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Get PoH status
   * @returns {Promise<Object>}
   */
  async getPoHStatus() {
    return this._get('/poh/status');
  }

  /**
   * Get blockchain stats
   * @returns {Promise<Object>}
   */
  async getStats() {
    return this._get('/stats');
  }
}

// ============================================================================
// FRONTEND HELPERS
// ============================================================================

/**
 * Format BB balance for display
 * @param {number} amount 
 * @returns {string}
 */
export function formatBB(amount) {
  return `${amount.toLocaleString()} BB`;
}

/**
 * Format BB balance as USD
 * @param {number} amount 
 * @returns {string}
 */
export function formatUSD(amount) {
  return `$${(amount * 0.01).toFixed(2)}`;
}

/**
 * Calculate implied probability from price
 * @param {number} price - Price between 0 and 1
 * @returns {string}
 */
export function impliedProbability(price) {
  return `${(price * 100).toFixed(1)}%`;
}

/**
 * Calculate potential payout
 * @param {number} betAmount 
 * @param {number} price - Current price (0-1)
 * @returns {number}
 */
export function calculatePayout(betAmount, price) {
  return betAmount / price;
}

// ============================================================================
// WEBSOCKET CLIENT (Real-time Updates)
// ============================================================================

/**
 * WebSocket client for real-time L2 updates
 */
export class L2EventSubscriber {
  constructor(wsUrl = 'ws://localhost:1234/ws') {
    this.wsUrl = wsUrl;
    this.ws = null;
    this.handlers = {
      bet: [],
      market: [],
      resolution: [],
      bridge: [],
      error: [],
    };
  }

  connect() {
    this.ws = new WebSocket(this.wsUrl);
    
    this.ws.onopen = () => console.log('[L2Events] Connected');
    this.ws.onclose = () => console.log('[L2Events] Disconnected');
    this.ws.onerror = (e) => this._emit('error', e);
    
    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this._handleMessage(data);
      } catch (e) {
        console.error('[L2Events] Parse error:', e);
      }
    };
  }

  disconnect() {
    if (this.ws) this.ws.close();
  }

  on(event, handler) {
    if (this.handlers[event]) {
      this.handlers[event].push(handler);
    }
    return this;
  }

  _emit(event, data) {
    if (this.handlers[event]) {
      this.handlers[event].forEach(h => h(data));
    }
  }

  _handleMessage(data) {
    const type = data.type || data.event;
    this._emit(type, data);
  }

  subscribeMarket(marketId) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'subscribe', market_id: marketId }));
    }
  }
}

// ============================================================================
// FACTORY FUNCTIONS
// ============================================================================

/**
 * Create L2 client with default config
 * @param {Object} [config] 
 * @returns {L2Client}
 */
export function createL2Client(config = {}) {
  return new L2Client(config);
}

/**
 * Create L1 RPC client
 * @param {string} [l1Url] 
 * @returns {L1RpcClient}
 */
export function createL1RpcClient(l1Url = null) {
  return new L1RpcClient(l1Url);
}

/**
 * Create event subscriber
 * @param {string} [wsUrl] 
 * @returns {L2EventSubscriber}
 */
export function createEventSubscriber(wsUrl = null) {
  return new L2EventSubscriber(wsUrl);
}

// ============================================================================
// EXPORTS
// ============================================================================

export {
  DEFAULT_CONFIG,
  generateNonce,
  getTimestamp,
  hexToBytes,
  bytesToHex,
};

export default L2Client;

// CommonJS
if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    L2Client,
    L1RpcClient,
    L2EventSubscriber,
    createL2Client,
    createL1RpcClient,
    createEventSubscriber,
    formatBB,
    formatUSD,
    impliedProbability,
    calculatePayout,
    generateNonce,
    getTimestamp,
    hexToBytes,
    bytesToHex,
    DEFAULT_CONFIG,
  };
}

// Browser globals
if (typeof window !== 'undefined') {
  window.L2Client = L2Client;
  window.L1RpcClient = L1RpcClient;
  window.L2EventSubscriber = L2EventSubscriber;
  window.createL2Client = createL2Client;
}
