// ============================================================================
// BlackBook L2 Prediction Market SDK
// ============================================================================
//
// SDK for interacting with BlackBook Layer 2 Prediction Market
// Uses L1 wallet signatures for authentication
//
// Architecture:
// - L1 Blockchain (Port 8080): Core wallet, balances, signature verification
// - L2 Prediction Market (Port 1234): Betting, markets, AI events
//
// Authentication Flow:
// 1. User has L1 wallet (from BlackBook Wallet SDK V2)
// 2. L2 actions are signed with L1 private key
// 3. L2 verifies signature against L1 or uses test accounts
//
// Token: BlackBook (BB) - Stable at $0.01
// ============================================================================

import nacl from 'tweetnacl';

// ============================================================================
// CONFIGURATION
// ============================================================================

const L1_API_URL = process.env.L1_API_URL || 'http://localhost:8080';
const L2_API_URL = process.env.L2_API_URL || 'http://localhost:1234';

// ============================================================================
// L2 TEST ACCOUNTS - Pre-funded wallets for development
// ============================================================================
// These accounts are hardcoded on both L1 and L2 for testing
// Each has 1000 BB (except HOUSE which has 10000 BB)

export const L2_TEST_ACCOUNTS = {
  ALICE: {
    name: 'ALICE',
    address: 'L1_48F58216BD686E2F8F710E227EBB91539F30FA506336688393DAC058B11461DA',
    balance: 1000
  },
  BOB: {
    name: 'BOB', 
    address: 'L1_6DD0DC4C96CABD0EAF36F62A32F79FAD55FE050EE1D188526C5B686B0688D0E7',
    balance: 1000
  },
  CHARLIE: {
    name: 'CHARLIE',
    address: 'L1_9337C145B33978237B84DAC70C7590F3415BEC105A9B96D2D7B2152EE398B8BD',
    balance: 1000
  },
  DIANA: {
    name: 'DIANA',
    address: 'L1_9B800929EED7BD068CD897731EF1BF126098FC59DAB193154B53D9F76E839135',
    balance: 1000
  },
  ETHAN: {
    name: 'ETHAN',
    address: 'L1_0FF0D66C66FD7AB9756A2EA8443E9C34BBDEF2E5831BCC2B1719A78EAFFFE3A2',
    balance: 1000
  },
  FIONA: {
    name: 'FIONA',
    address: 'L1_01A28A1681ED8C4C0496DA9C881EC156C05E309200A7A20E73CD559FDEB51C28',
    balance: 1000
  },
  GEORGE: {
    name: 'GEORGE',
    address: 'L1_D4298B959D93C051F8AD3AC011CE7E084542E3FEC8B34A4FB9D9DD920C9DF58D',
    balance: 1000
  },
  HANNAH: {
    name: 'HANNAH',
    address: 'L1_452752DA4284AC7DDD3EA42EEB5D210D4EDAFEF649CEEA33F99C179BA68C3091',
    balance: 1000
  },
  HOUSE: {
    name: 'HOUSE',
    address: 'L1_CDCC4C18855728E00EFD5BC22CD594D91F4BF305FEA61D8A07492B5F7099E95E',
    balance: 10000  // House has more funds
  }
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Generate a random nonce for request uniqueness
 */
function generateNonce() {
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return Array.from(array, b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Get current Unix timestamp in seconds
 */
function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

/**
 * Sign a message with ed25519 private key
 * @param {string} message - Message to sign
 * @param {Uint8Array} privateKey - 64-byte ed25519 private key
 * @returns {string} - Hex-encoded signature
 */
function signMessage(message, privateKey) {
  const messageBytes = new TextEncoder().encode(message);
  const signature = nacl.sign.detached(messageBytes, privateKey);
  return Array.from(signature, b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Create a signed request for L2 API
 * @param {string} publicKey - Hex public key
 * @param {Uint8Array} privateKey - 64-byte private key
 * @param {Object} payload - Request payload
 * @returns {Object} - Signed request body
 */
function createSignedRequest(publicKey, privateKey, payload) {
  const timestamp = getTimestamp();
  const nonce = generateNonce();
  
  // Create canonical message: publicKey + JSON(payload) + timestamp + nonce
  const message = publicKey + JSON.stringify(payload) + timestamp + nonce;
  const signature = signMessage(message, privateKey);
  
  return {
    public_key: publicKey,
    payload,
    timestamp,
    nonce,
    signature
  };
}

// ============================================================================
// L2 PREDICTION MARKET SDK CLASS
// ============================================================================

export class BlackBookL2SDK {
  constructor(options = {}) {
    this.l1Url = options.l1Url || L1_API_URL;
    this.l2Url = options.l2Url || L2_API_URL;
    
    // Connected wallet state
    this.connectedAccount = null;
    this.publicKey = null;
    this.privateKey = null;
  }

  // ==========================================================================
  // WALLET CONNECTION
  // ==========================================================================

  /**
   * Connect using a test account name (ALICE, BOB, etc.)
   * For development only - in production use L1 wallet
   * @param {string} accountName - One of: ALICE, BOB, CHARLIE, DIANA, ETHAN, FIONA, GEORGE, HANNAH, HOUSE
   */
  async connectTestAccount(accountName) {
    const account = L2_TEST_ACCOUNTS[accountName.toUpperCase()];
    if (!account) {
      throw new Error(`Unknown test account: ${accountName}. Available: ${Object.keys(L2_TEST_ACCOUNTS).join(', ')}`);
    }
    
    // Call L2 connect endpoint
    const response = await fetch(`${this.l2Url}/wallet/connect/${accountName.toUpperCase()}`);
    const data = await response.json();
    
    this.connectedAccount = account;
    console.log(`✅ Connected to ${account.name}: ${account.address}`);
    
    return data;
  }

  /**
   * Connect with L1 wallet (full cryptographic auth)
   * @param {string} publicKeyHex - 64-char hex public key
   * @param {Uint8Array} privateKey - 64-byte ed25519 private key
   */
  connectWithWallet(publicKeyHex, privateKey) {
    this.publicKey = publicKeyHex;
    this.privateKey = privateKey;
    this.connectedAccount = {
      name: 'L1_WALLET',
      address: publicKeyHex
    };
    console.log(`✅ Connected L1 wallet: ${publicKeyHex.slice(0, 16)}...`);
  }

  /**
   * Get all available test accounts
   */
  async getTestAccounts() {
    const response = await fetch(`${this.l2Url}/wallet/test-accounts`);
    return response.json();
  }

  /**
   * Get detailed account info
   * @param {string} accountName - Account name (ALICE, BOB, etc.)
   */
  async getAccountInfo(accountName) {
    const response = await fetch(`${this.l2Url}/wallet/account-info/${accountName.toUpperCase()}`);
    return response.json();
  }

  // ==========================================================================
  // ACCOUNT MANAGEMENT
  // ==========================================================================

  /**
   * Health check - verify L2 is running
   */
  async health() {
    const response = await fetch(`${this.l2Url}/health`);
    return response.json();
  }

  /**
   * Get all accounts on L2
   */
  async getAccounts() {
    const response = await fetch(`${this.l2Url}/accounts`);
    return response.json();
  }

  /**
   * Get balance for an address
   * @param {string} address - L1 address (L1_...) or wallet public key
   */
  async getBalance(address) {
    const response = await fetch(`${this.l2Url}/balance/${address}`);
    return response.json();
  }

  /**
   * Get connected wallet balance
   */
  async getMyBalance() {
    if (!this.connectedAccount) {
      throw new Error('No wallet connected. Call connectTestAccount() or connectWithWallet() first.');
    }
    return this.getBalance(this.connectedAccount.address);
  }

  /**
   * Deposit funds to L2 account
   * @param {string} address - Destination address
   * @param {number} amount - Amount in BB
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
   * Transfer funds between L2 accounts
   * @param {string} from - Source address
   * @param {string} to - Destination address  
   * @param {number} amount - Amount in BB
   */
  async transfer(from, to, amount) {
    const response = await fetch(`${this.l2Url}/transfer`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ from, to, amount })
    });
    return response.json();
  }

  /**
   * Get transaction history for an address
   * @param {string} address - Address to query
   */
  async getTransactions(address) {
    const response = await fetch(`${this.l2Url}/transactions/${address}`);
    return response.json();
  }

  /**
   * Get all transactions
   */
  async getAllTransactions() {
    const response = await fetch(`${this.l2Url}/transactions`);
    return response.json();
  }

  /**
   * Get ledger statistics
   */
  async getLedgerStats() {
    const response = await fetch(`${this.l2Url}/ledger/stats`);
    return response.json();
  }

  // ==========================================================================
  // PREDICTION MARKETS
  // ==========================================================================

  /**
   * Get all prediction markets
   */
  async getMarkets() {
    const response = await fetch(`${this.l2Url}/markets`);
    return response.json();
  }

  /**
   * Get a specific market by ID
   * @param {string} marketId - Market UUID
   */
  async getMarket(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}`);
    return response.json();
  }

  /**
   * Place a bet on a prediction market
   * @param {Object} bet - Bet details
   * @param {string} bet.market_id - Market UUID
   * @param {string} bet.bettor - Bettor address
   * @param {string} bet.option - Option to bet on (e.g., "YES", "NO", or custom)
   * @param {number} bet.amount - Bet amount in BB
   * 
   * @example
   * await sdk.placeBet({
   *   market_id: "abc123",
   *   bettor: "L1_48F58216...",
   *   option: "YES",
   *   amount: 10
   * });
   */
  async placeBet(bet) {
    const response = await fetch(`${this.l2Url}/bet`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(bet)
    });
    return response.json();
  }

  /**
   * Place a bet using connected wallet
   * @param {string} marketId - Market UUID
   * @param {string} option - Option to bet on
   * @param {number} amount - Amount in BB
   */
  async bet(marketId, option, amount) {
    if (!this.connectedAccount) {
      throw new Error('No wallet connected');
    }
    return this.placeBet({
      market_id: marketId,
      bettor: this.connectedAccount.address,
      option,
      amount
    });
  }

  /**
   * Resolve a market (admin only)
   * @param {string} marketId - Market UUID
   * @param {string} winningOption - The winning option
   */
  async resolveMarket(marketId, winningOption) {
    const response = await fetch(`${this.l2Url}/resolve/${marketId}/${winningOption}`, {
      method: 'POST'
    });
    return response.json();
  }

  /**
   * Get leaderboard (featured markets with 10+ bettors)
   */
  async getLeaderboard() {
    const response = await fetch(`${this.l2Url}/leaderboard`);
    return response.json();
  }

  /**
   * Get all market activities
   */
  async getActivities() {
    const response = await fetch(`${this.l2Url}/activities`);
    return response.json();
  }

  // ==========================================================================
  // AI EVENTS
  // ==========================================================================

  /**
   * Create an AI-generated prediction event
   * @param {Object} event - Event details
   * @param {string} event.title - Event title
   * @param {string} event.description - Event description
   * @param {Array<string>} event.options - Betting options
   * @param {string} event.source - Source of the event
   * @param {string} event.category - Category (sports, politics, crypto, etc.)
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
   * @returns {string} - RSS feed URL
   */
  getAIEventsFeedUrl() {
    return `${this.l2Url}/ai/events/feed.rss`;
  }

  /**
   * Get pending events (CPMM inbox)
   */
  async getPendingEvents() {
    const response = await fetch(`${this.l2Url}/events/pending`);
    return response.json();
  }

  /**
   * Launch a pending event as a market
   * @param {string} eventId - Event UUID
   */
  async launchEvent(eventId) {
    const response = await fetch(`${this.l2Url}/events/${eventId}/launch`, {
      method: 'POST'
    });
    return response.json();
  }

  // ==========================================================================
  // L1 ↔ L2 BRIDGE
  // ==========================================================================

  /**
   * Initiate a bridge from L2 to L1
   * Locks tokens on L2 and requests L1 to credit them
   * @param {Object} params - Bridge parameters
   * @param {string} params.from_address - L2 source address
   * @param {string} params.to_address - L1 destination address
   * @param {number} params.amount - Amount to bridge
   * 
   * @example
   * // Bridge 100 BB from L2 to L1
   * const result = await sdk.bridgeToL1({
   *   from_address: L2_TEST_ACCOUNTS.ALICE.address,
   *   to_address: "4013e5a9...",  // L1 public key
   *   amount: 100
   * });
   */
  async bridgeToL1(params) {
    const response = await fetch(`${this.l2Url}/rpc/bridge`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params)
    });
    return response.json();
  }

  /**
   * Check bridge status
   * @param {string} bridgeId - Bridge transaction ID
   */
  async getBridgeStatus(bridgeId) {
    const response = await fetch(`${this.l2Url}/rpc/bridge/${bridgeId}`);
    return response.json();
  }

  /**
   * Get all pending bridges
   */
  async getPendingBridges() {
    const response = await fetch(`${this.l2Url}/bridge/pending`);
    return response.json();
  }

  /**
   * Get bridge statistics
   */
  async getBridgeStats() {
    const response = await fetch(`${this.l2Url}/bridge/stats`);
    return response.json();
  }

  // ==========================================================================
  // ADMIN FUNCTIONS
  // ==========================================================================

  /**
   * Mint tokens to an account (admin only)
   * @param {string} address - Destination address
   * @param {number} amount - Amount to mint
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
   * Set account balance (admin only)
   * @param {string} address - Account address
   * @param {number} balance - New balance
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
  // L1 INTEGRATION HELPERS
  // ==========================================================================

  /**
   * Verify an L1 signature on L2 (for cross-chain verification)
   * @param {Object} signedRequest - Signed request from L1 wallet
   */
  async verifyL1Signature(signedRequest) {
    const response = await fetch(`${this.l1Url}/rpc/verify-signature`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });
    return response.json();
  }

  /**
   * Get L1 balance for verification
   * @param {string} address - L1 address/public key
   */
  async getL1Balance(address) {
    const response = await fetch(`${this.l1Url}/balance/${address}`);
    return response.json();
  }

  /**
   * Get L1 health status
   */
  async getL1Health() {
    const response = await fetch(`${this.l1Url}/health`);
    return response.json();
  }

  /**
   * Get L1 PoH (Proof of History) status
   */
  async getL1PoHStatus() {
    const response = await fetch(`${this.l1Url}/poh/status`);
    return response.json();
  }

  // ==========================================================================
  // L1 RPC INTEGRATION - L2 directly queries L1 blockchain
  // ==========================================================================

  /**
   * Get L1 wallet address for a Supabase user
   * This is called by L2 during login to fetch the user's L1 wallet
   * @param {string} userId - Supabase user ID
   * @returns {Promise<Object>} - { wallet_address, username, balance }
   */
  async getL1WalletByUserId(userId) {
    const response = await fetch(`${this.l1Url}/auth/wallet/${userId}`);
    if (!response.ok) {
      throw new Error(`L1 wallet not found for user ${userId}. User must register on L1 first.`);
    }
    return response.json();
  }

  /**
   * Verify an L1 signature (ed25519)
   * L2 calls this to verify that a signature is valid for a given public key
   * @param {Object} params
   * @param {string} params.pubkey - Hex public key (64 chars)
   * @param {string} params.message - Message that was signed
   * @param {string} params.signature - Hex signature (128 chars)
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
   * Get L1 balance for verification
   * @param {string} address - L1 address (L1_...) or public key
   */
  async getL1Balance(address) {
    const response = await fetch(`${this.l1Url}/balance/${address}`);
    return response.json();
  }

  /**
   * Get L1 account info
   * @param {string} address - L1 address or public key
   */
  async getL1Account(address) {
    const response = await fetch(`${this.l1Url}/account/${address}`);
    return response.json();
  }

  /**
   * Get nonce from L1 (for replay protection)
   * @param {string} address - L1 address
   */
  async getL1Nonce(address) {
    const response = await fetch(`${this.l1Url}/rpc/nonce/${address}`);
    return response.json();
  }

  /**
   * Record settlement on L1 (called by L2 after market resolution)
   * @param {Object} settlement
   * @param {string} settlement.market_id
   * @param {number} settlement.outcome
   * @param {Array} settlement.winners - Array of [address, payout]
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
   */
  async getL1Health() {
    const response = await fetch(`${this.l1Url}/health`);
    return response.json();
  }

  /**
   * Get L1 PoH (Proof of History) status
   */
  async getL1PoHStatus() {
    const response = await fetch(`${this.l1Url}/poh/status`);
    return response.json();
  }

  /**
   * Check if L1 and L2 are properly connected
   * @returns {Promise<Object>} - { connected: boolean, l1_status: ..., l2_status: ... }
   */
  async checkL1L2Connection() {
    try {
      const [l1Health, l2Health] = await Promise.all([
        this.getL1Health(),
        this.health()
      ]);
      
      return {
        connected: true,
        l1_status: l1Health,
        l2_status: l2Health,
        message: '✅ L1 and L2 are properly connected'
      };
    } catch (error) {
      return {
        connected: false,
        error: error.message,
        message: '❌ L1 and L2 connection failed'
      };
    }
  }

  // ==========================================================================
  // SUPABASE AUTHENTICATION FLOW
  // ==========================================================================

  /**
   * Login with Supabase JWT
   * This calls L2's /auth/login which queries L1 for the user's wallet
   * @param {string} jwt - Supabase Access Token
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
    
    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'Login failed');
    }
    
    const data = await response.json();
    
    if (!data.success) {
      throw new Error(data.error || 'Login failed');
    }
    
    // Cache the wallet address
    this.connectedAccount = {
      name: 'SUPABASE_USER',
      address: data.wallet_address,
      user_id: data.user_id
    };
    
    console.log(`✅ Logged in as ${data.wallet_address} (Balance: ${data.balance} BB)`);
    return data;
  }

  /**
   * Place a bet using Supabase Auth
   * @param {string} marketId 
   * @param {number} outcome (0 = first option, 1 = second option, etc.)
   * @param {number} amount 
   */
  async placeAuthenticatedBet(marketId, outcome, amount) {
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
        outcome: outcome,
        amount: amount
      })
    });

    const data = await response.json();
    
    if (!data.success) {
      throw new Error(data.error || 'Bet failed');
    }
    
    return data;
  }

  /**
   * Get authenticated user info
   */
  async getUserInfo() {
    if (!this.jwt) {
      throw new Error('Not logged in');
    }
    
    const response = await fetch(`${this.l2Url}/auth/user`, {
      headers: { 'Authorization': `Bearer ${this.jwt}` }
    });
    
    const data = await response.json();
    
    if (!data.success) {
      throw new Error(data.error || 'Failed to get user info');
    }
    
    return data;
  }
}

// ============================================================================
// QUICK START EXAMPLES
// ============================================================================

/*
// ============================================================================
// EXAMPLE 1: Supabase Auth Flow (Production)
// ============================================================================
import { BlackBookL2SDK } from './blackbook-l2-prediction-sdk.js';

const sdk = new BlackBookL2SDK();

// Check L1↔L2 connection
const connection = await sdk.checkL1L2Connection();
console.log(connection);

// Login with Supabase JWT (user registered on L1)
const supabaseJWT = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...'; // From Supabase Auth
const user = await sdk.loginWithSupabase(supabaseJWT);
console.log('User:', user);
// { success: true, user_id: "...", wallet_address: "L1_ABC...", balance: 1000 }

// Get user info
const userInfo = await sdk.getUserInfo();
console.log(userInfo);

// Get all markets
const markets = await sdk.getMarkets();

// Place a bet using JWT auth
const betResult = await sdk.placeAuthenticatedBet(
  markets[0].id,  // market_id
  0,              // outcome (0 = first option)
  50              // amount in BB
);
console.log('Bet placed:', betResult);


// ============================================================================
// EXAMPLE 2: Test Account (Development)
// ============================================================================
import { BlackBookL2SDK, L2_TEST_ACCOUNTS } from './blackbook-l2-prediction-sdk.js';

const sdk = new BlackBookL2SDK();

// Connect with Alice's test account
await sdk.connectTestAccount('ALICE');

// Check balance
const balance = await sdk.getMyBalance();
console.log('Balance:', balance);

// Get all markets
const markets = await sdk.getMarkets();
console.log('Markets:', markets);

// Place a bet on a market
if (markets.length > 0) {
  const result = await sdk.bet(markets[0].id, 'YES', 10);
  console.log('Bet placed:', result);
}


// ============================================================================
// EXAMPLE 3: L1 RPC Integration (Backend/Server)
// ============================================================================
const sdk = new BlackBookL2SDK();

// L2 queries L1 for user's wallet
const wallet = await sdk.getL1WalletByUserId('supabase-user-uuid');
console.log(wallet);
// { wallet_address: "L1_ABC123...", username: "Alice", balance: 1000 }

// Verify signature against L1
const isValid = await sdk.verifyL1Signature({
  pubkey: '4013e5a935e9873a57879c471d5da83845ed5fc4d7bf4ce6dca53d51f30e7ad2',
  message: 'bet:market-123:outcome-0:amount-50',
  signature: 'abc123def456...'
});
console.log('Signature valid:', isValid);

// Get L1 balance
const l1Balance = await sdk.getL1Balance('L1_ABC123...');

// Get L1 nonce (for replay protection)
const nonce = await sdk.getL1Nonce('L1_ABC123...');

// Record settlement on L1 (called by L2 after resolving market)
await sdk.recordL1Settlement({
  market_id: 'market-123',
  outcome: 0,
  winners: [
    ['L1_ABC123...', 150],
    ['L1_DEF456...', 50]
  ]
});


// ============================================================================
// EXAMPLE 4: Bridge L2 → L1
// ============================================================================
const bridgeResult = await sdk.bridgeToL1({
  from_address: L2_TEST_ACCOUNTS.ALICE.address,
  to_address: '4013e5a935e9873a57879c471d5da83845ed5fc4d7bf4ce6dca53d51f30e7ad2',
  amount: 50
});
console.log('Bridge initiated:', bridgeResult);

// Check bridge status
const status = await sdk.getBridgeStatus(bridgeResult.bridge_id);
console.log('Bridge status:', status);


// ============================================================================
// EXAMPLE 5: Create AI Event
// ============================================================================
const event = await sdk.createAIEvent({
  title: 'Will Bitcoin hit $100k by 2025?',
  description: 'Prediction market for BTC price milestone',
  options: ['YES', 'NO'],
  source: 'BlackBook AI',
  category: 'crypto'
});
console.log('Event created:', event);


// ============================================================================
// EXAMPLE 6: Full L1 Wallet Integration
// ============================================================================
import { BlackBookWallet } from './blackbook-wallet-sdk-v2.js';

// Create or load L1 wallet
const wallet = new BlackBookWallet();
await wallet.create('MySecurePassword123!');

// Connect L2 SDK with L1 wallet
const l2sdk = new BlackBookL2SDK();
l2sdk.connectWithWallet(wallet.publicKeyHex, wallet.privateKey);

// Now all L2 operations use L1 wallet signature
const myBalance = await l2sdk.getMyBalance();
*/

export default BlackBookL2SDK;
