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
//   L1 (Port 8080): Core blockchain - wallets, balances, signatures
//   L2 (Port 1234): Prediction market - betting, markets, AI events
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
   * @returns {Promise<Object>} Login response with wallet and balance
   */
  async loginWithSupabase(jwt) {
    this.jwt = jwt;
    
    const response = await fetch(`${this.l2Url}/auth/login`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${jwt}`,
        'Content-Type': 'application/json'
      }
    });

    const data = await response.json();

    if (data.success && data.wallet_address) {
      this.walletAddress = data.wallet_address;
      this.connectedAccount = {
        name: 'SUPABASE_USER',
        address: data.wallet_address,
        user_id: data.user_id
      };
      console.log(`✅ Logged in: ${data.wallet_address} (${data.balance} BB)`);
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
   * Place a bet using Supabase JWT authentication (Production)
   * POST /bet/auth
   * @param {string} marketId - Market UUID
   * @param {number} outcome - Outcome index (0 = first option, 1 = second, etc.)
   * @param {number} amount - Amount in BB tokens
   * @returns {Promise<Object>} Bet response with transaction ID
   */
  async placeBet(marketId, outcome, amount) {
    if (!this.jwt) {
      throw new Error('Not logged in. Call loginWithSupabase() first.');
    }

    const response = await fetch(`${this.l2Url}/bet/auth`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.jwt}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        market_id: marketId,
        outcome,
        amount
      })
    });

    return response.json();
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

  /**
   * Bridge tokens from L2 to L1
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
  // RSS MARKET INITIALIZATION
  // ==========================================================================

  /**
   * Initialize a market from RSS event payload
   * POST /markets/rss
   * 
   * @param {Object} rssEvent - RSS event payload
   * @param {string} rssEvent.market_id - Unique market ID
   * @param {string} rssEvent.meta_title - Market title
   * @param {string} rssEvent.meta_description - Market description
   * @param {string} rssEvent.market_type - "binary" | "three_choice" | "multi"
   * @param {string[]} rssEvent.outcomes - Betting outcomes ["Yes", "No Change", "No"]
   * @param {number[]} rssEvent.initial_odds - Initial odds [0.49, 0.02, 0.49]
   * @param {string} rssEvent.source - Source URL
   * @param {string} rssEvent.pub_date - Publication date (ISO8601)
   * @param {string} rssEvent.resolution_date - Resolution date (ISO8601)
   * @param {string} rssEvent.freeze_date - Betting freeze date (ISO8601)
   * @param {Object} [rssEvent.resolution_rules] - Resolution rules for each outcome
   * @returns {Promise<Object>} Created market response
   * 
   * @example
   * await sdk.initializeMarketFromRss({
   *   market_id: "rss_market_abc123",
   *   meta_title: "Will BTC hit $100k by Jan 2025?",
   *   meta_description: "Based on current market trends...",
   *   market_type: "three_choice",
   *   outcomes: ["Yes", "No Change", "No"],
   *   initial_odds: [0.49, 0.02, 0.49],
   *   source: "https://example.com/article",
   *   pub_date: "2024-12-10T00:00:00Z",
   *   resolution_date: "2025-01-01T00:00:00Z",
   *   freeze_date: "2024-12-31T23:00:00Z",
   *   resolution_rules: {
   *     optional: false,
   *     rules: {
   *       "YES": "BTC price >= $100,000 on Coinbase at 00:00 UTC Jan 1, 2025",
   *       "NO_CHANGE": "BTC price between $95,000 and $100,000",
   *       "NO": "BTC price < $95,000"
   *     }
   *   }
   * });
   */
  async initializeMarketFromRss(rssEvent) {
    // Validate required fields
    const required = ['market_id', 'meta_title', 'outcomes', 'initial_odds', 'resolution_date'];
    for (const field of required) {
      if (!rssEvent[field]) {
        throw new Error(`Missing required field: ${field}`);
      }
    }

    // Validate odds sum to 1.0
    const oddsSum = rssEvent.initial_odds.reduce((a, b) => a + b, 0);
    if (Math.abs(oddsSum - 1.0) > 0.01) {
      throw new Error(`initial_odds must sum to 1.0 (got ${oddsSum})`);
    }

    // Validate outcomes match odds
    if (rssEvent.outcomes.length !== rssEvent.initial_odds.length) {
      throw new Error(`outcomes count must match initial_odds count`);
    }

    const response = await fetch(`${this.l2Url}/markets/rss`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(rssEvent)
    });

    return response.json();
  }

  /**
   * Batch initialize multiple markets from RSS feed
   * POST /markets/rss/batch
   * @param {Object[]} rssEvents - Array of RSS event payloads
   * @returns {Promise<Object>} Batch creation response
   */
  async initializeMarketsFromRssBatch(rssEvents) {
    if (!Array.isArray(rssEvents) || rssEvents.length === 0) {
      throw new Error('rssEvents must be a non-empty array');
    }

    const response = await fetch(`${this.l2Url}/markets/rss/batch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ events: rssEvents })
    });

    return response.json();
  }

  /**
   * Get market by RSS market ID
   * GET /markets/rss/:marketId
   * @param {string} marketId - RSS market ID
   */
  async getMarketByRssId(marketId) {
    const response = await fetch(`${this.l2Url}/markets/rss/${marketId}`);
    return response.json();
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
