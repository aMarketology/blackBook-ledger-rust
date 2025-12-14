// ============================================================================
// BLACKBOOK L1 INTEGRATION SDK
// ============================================================================
//
// This SDK provides L1 blockchain operations for L2 and frontend integration.
// Includes full cross-layer RPC support for L2 prediction market.
//
// ARCHITECTURE:
//   L1 (Port 8080) - Source of truth, token custody, PoH consensus
//   L2 (Port 1234) - Prediction market, instant bets, optimistic execution
//
// USAGE:
//   import { L1Client, L2RpcClient } from './layer1_integration_sdk.js';
//   
//   // L1 Client (read-only blockchain queries)
//   const l1 = new L1Client('http://localhost:8080');
//   const balance = await l1.getBalance('L1ALICE000000001');
//   
//   // L2 RPC Client (for L2 server to communicate with L1)
//   const l2rpc = new L2RpcClient('http://localhost:8080');
//   const valid = await l2rpc.verifyUserSignature(pubKey, message, sig);
//   await l2rpc.recordSettlement({ marketId, outcome, winners, l2BlockHeight });
//
// ============================================================================

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

function generateNonce() {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return 'xxxx-xxxx-xxxx'.replace(/x/g, () => 
    Math.floor(Math.random() * 16).toString(16)
  );
}

function getTimestamp() {
  return Math.floor(Date.now() / 1000);
}

// ============================================================================
// CONFIGURATION
// ============================================================================

const DEFAULT_CONFIG = {
  L1_URL: 'http://localhost:8080',
  L2_URL: 'http://localhost:1234',
  TIMEOUT: 30000,  // 30 second timeout
  RETRY_ATTEMPTS: 3,
  RETRY_DELAY: 1000,  // 1 second between retries
};

// ============================================================================
// L1 CLIENT CLASS - Read-only blockchain operations
// ============================================================================

export class L1Client {
  /**
   * Create L1 Client
   * @param {string} [url] - L1 server URL
   * @param {Object} [options] - Additional options
   */
  constructor(url = null, options = {}) {
    this.url = url || (typeof process !== 'undefined' && process.env?.L1_URL) || DEFAULT_CONFIG.L1_URL;
    this.timeout = options.timeout || DEFAULT_CONFIG.TIMEOUT;
    this.retryAttempts = options.retryAttempts || DEFAULT_CONFIG.RETRY_ATTEMPTS;
    this.retryDelay = options.retryDelay || DEFAULT_CONFIG.RETRY_DELAY;
    this.debug = options.debug || false;
  }

  // ==========================================================================
  // INTERNAL HELPERS
  // ==========================================================================

  _log(...args) {
    if (this.debug) {
      console.log('[L1Client]', ...args);
    }
  }

  async _fetch(endpoint, options = {}) {
    const url = `${this.url}${endpoint}`;
    this._log('Fetching:', url);
    
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);
    
    try {
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
        headers: {
          'Content-Type': 'application/json',
          ...options.headers,
        },
      });
      
      clearTimeout(timeoutId);
      
      if (!response.ok) {
        const error = await response.text();
        throw new Error(`HTTP ${response.status}: ${error}`);
      }
      
      return await response.json();
    } catch (error) {
      clearTimeout(timeoutId);
      if (error.name === 'AbortError') {
        throw new Error(`Request timeout after ${this.timeout}ms`);
      }
      throw error;
    }
  }

  async _fetchWithRetry(endpoint, options = {}) {
    let lastError;
    
    for (let attempt = 1; attempt <= this.retryAttempts; attempt++) {
      try {
        return await this._fetch(endpoint, options);
      } catch (error) {
        lastError = error;
        this._log(`Attempt ${attempt} failed:`, error.message);
        
        if (attempt < this.retryAttempts) {
          await new Promise(r => setTimeout(r, this.retryDelay));
        }
      }
    }
    
    throw lastError;
  }

  // ==========================================================================
  // HEALTH & STATUS
  // ==========================================================================

  /**
   * Check if L1 server is healthy
   * @returns {Promise<boolean>}
   */
  async health() {
    try {
      const response = await fetch(`${this.url}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Get detailed health info
   * @returns {Promise<Object>}
   */
  async healthInfo() {
    try {
      return await this._fetch('/health');
    } catch {
      return { status: 'offline', error: 'Connection failed' };
    }
  }

  /**
   * Get L1 server version info
   * @returns {Promise<Object>}
   */
  async getVersion() {
    try {
      return await this._fetch('/version');
    } catch {
      return { version: 'unknown' };
    }
  }

  // ==========================================================================
  // BLOCKCHAIN STATS
  // ==========================================================================

  /**
   * Get blockchain statistics
   * @returns {Promise<Object>}
   */
  async getStats() {
    return await this._fetchWithRetry('/stats');
  }

  /**
   * Get total supply of BB tokens
   * @returns {Promise<number>}
   */
  async getTotalSupply() {
    const stats = await this.getStats();
    return stats.total_supply || 0;
  }

  /**
   * Get circulating supply
   * @returns {Promise<number>}
   */
  async getCirculatingSupply() {
    const stats = await this.getStats();
    return stats.circulating_supply || stats.total_supply || 0;
  }

  /**
   * Get number of registered accounts
   * @returns {Promise<number>}
   */
  async getAccountCount() {
    const stats = await this.getStats();
    return stats.accounts || stats.account_count || 0;
  }

  // ==========================================================================
  // BLOCKS
  // ==========================================================================

  /**
   * Get current block height
   * @returns {Promise<number>}
   */
  async getBlockHeight() {
    const stats = await this.getStats();
    return stats.block_height || stats.blocks || 0;
  }

  /**
   * Get block by height
   * @param {number} height - Block height
   * @returns {Promise<Object>}
   */
  async getBlock(height) {
    return await this._fetchWithRetry(`/block/${height}`);
  }

  /**
   * Get latest block
   * @returns {Promise<Object>}
   */
  async getLatestBlock() {
    return await this._fetchWithRetry('/block/latest');
  }

  /**
   * Get multiple blocks
   * @param {number} startHeight - Starting height
   * @param {number} count - Number of blocks
   * @returns {Promise<Array>}
   */
  async getBlocks(startHeight, count = 10) {
    return await this._fetchWithRetry(`/blocks?start=${startHeight}&count=${count}`);
  }

  /**
   * Get block by hash
   * @param {string} hash - Block hash
   * @returns {Promise<Object>}
   */
  async getBlockByHash(hash) {
    return await this._fetchWithRetry(`/block/hash/${hash}`);
  }

  // ==========================================================================
  // TRANSACTIONS
  // ==========================================================================

  /**
   * Get transaction by ID
   * @param {string} txId - Transaction ID
   * @returns {Promise<Object>}
   */
  async getTransaction(txId) {
    return await this._fetchWithRetry(`/transaction/${txId}`);
  }

  /**
   * Get transactions for an address
   * @param {string} address - L1 address
   * @param {number} [limit=50] - Max transactions
   * @returns {Promise<Array>}
   */
  async getTransactions(address, limit = 50) {
    const result = await this._fetchWithRetry(`/transactions/${address}?limit=${limit}`);
    return result.transactions || result || [];
  }

  /**
   * Get recent transactions (network-wide)
   * @param {number} [limit=20] - Max transactions
   * @returns {Promise<Array>}
   */
  async getRecentTransactions(limit = 20) {
    const result = await this._fetchWithRetry(`/transactions/recent?limit=${limit}`);
    return result.transactions || result || [];
  }

  /**
   * Get transaction count for address
   * @param {string} address - L1 address
   * @returns {Promise<number>}
   */
  async getTransactionCount(address) {
    const txs = await this.getTransactions(address, 1000);
    return txs.length;
  }

  // ==========================================================================
  // BALANCES (Read-Only)
  // ==========================================================================

  /**
   * Get balance for an address
   * @param {string} address - L1 address
   * @returns {Promise<number>}
   */
  async getBalance(address) {
    const result = await this._fetchWithRetry(`/balance/${address}`);
    return result.balance || 0;
  }

  /**
   * Get detailed balance breakdown
   * @param {string} address - L1 address
   * @returns {Promise<Object>}
   */
  async getBalanceDetails(address) {
    try {
      const result = await this._fetchWithRetry(`/balance/${address}/details`);
      return result;
    } catch {
      // Fallback to simple balance
      const balance = await this.getBalance(address);
      return {
        total: balance,
        available: balance,
        locked: 0,
        l2_gaming: 0
      };
    }
  }

  /**
   * Get balances for multiple addresses
   * @param {Array<string>} addresses - Array of L1 addresses
   * @returns {Promise<Object>} Map of address -> balance
   */
  async getBalances(addresses) {
    const results = await Promise.all(
      addresses.map(async (addr) => {
        try {
          const balance = await this.getBalance(addr);
          return [addr, balance];
        } catch {
          return [addr, 0];
        }
      })
    );
    return Object.fromEntries(results);
  }

  // ==========================================================================
  // PROOF OF HISTORY (PoH)
  // ==========================================================================

  /**
   * Get current PoH status
   * @returns {Promise<Object>}
   */
  async getPoHStatus() {
    try {
      return await this._fetch('/poh/status');
    } catch {
      return { tick: 0, hash: null, running: false };
    }
  }

  /**
   * Get current PoH tick
   * @returns {Promise<number>}
   */
  async getPoHTick() {
    const status = await this.getPoHStatus();
    return status.tick || 0;
  }

  /**
   * Get PoH hash at specific tick
   * @param {number} tick - PoH tick number
   * @returns {Promise<string|null>}
   */
  async getPoHHash(tick) {
    try {
      const result = await this._fetch(`/poh/hash/${tick}`);
      return result.hash || null;
    } catch {
      return null;
    }
  }

  // ==========================================================================
  // BRIDGE STATUS (Read-Only)
  // ==========================================================================

  /**
   * Get bridge statistics
   * @returns {Promise<Object>}
   */
  async getBridgeStats() {
    try {
      return await this._fetch('/bridge/stats');
    } catch {
      return { total_bridged: 0, total_withdrawn: 0, active_sessions: 0 };
    }
  }

  /**
   * Get bridge transaction status
   * @param {string} bridgeId - Bridge transaction ID
   * @returns {Promise<Object>}
   */
  async getBridgeStatus(bridgeId) {
    return await this._fetchWithRetry(`/bridge/status/${bridgeId}`);
  }

  /**
   * Get pending bridge transactions
   * @returns {Promise<Array>}
   */
  async getPendingBridges() {
    try {
      const result = await this._fetch('/bridge/pending');
      return result.pending || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // MERKLE PROOFS & SETTLEMENTS
  // ==========================================================================

  /**
   * Get Merkle root for a settlement batch
   * @param {number} batchId - Settlement batch ID
   * @returns {Promise<Object>}
   */
  async getMerkleRoot(batchId) {
    return await this._fetchWithRetry(`/merkle/root/${batchId}`);
  }

  /**
   * Verify a Merkle proof
   * @param {string} rootHash - Merkle root hash
   * @param {Array<string>} proof - Proof path
   * @param {string} leaf - Leaf hash
   * @returns {Promise<boolean>}
   */
  async verifyMerkleProof(rootHash, proof, leaf) {
    const result = await this._fetch('/merkle/verify', {
      method: 'POST',
      body: JSON.stringify({ root: rootHash, proof, leaf }),
    });
    return result.valid === true;
  }

  /**
   * Get settlement history
   * @param {string} address - L1 address
   * @returns {Promise<Array>}
   */
  async getSettlements(address) {
    try {
      const result = await this._fetch(`/settlements/${address}`);
      return result.settlements || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // VALIDATORS & CONSENSUS
  // ==========================================================================

  /**
   * Get active validators
   * @returns {Promise<Array>}
   */
  async getValidators() {
    try {
      const result = await this._fetch('/validators');
      return result.validators || [];
    } catch {
      return [];
    }
  }

  /**
   * Get validator info
   * @param {string} validatorAddress - Validator address
   * @returns {Promise<Object>}
   */
  async getValidator(validatorAddress) {
    return await this._fetchWithRetry(`/validator/${validatorAddress}`);
  }

  /**
   * Get consensus status
   * @returns {Promise<Object>}
   */
  async getConsensusStatus() {
    try {
      return await this._fetch('/consensus/status');
    } catch {
      return { type: 'unknown', active: false };
    }
  }

  // ==========================================================================
  // SESSIONS (Read-Only)
  // ==========================================================================

  /**
   * Get session status for an address
   * @param {string} address - L1 address
   * @returns {Promise<Object>}
   */
  async getSessionStatus(address) {
    try {
      return await this._fetch(`/session/status/${address}`);
    } catch {
      return { active: false, balance: 0 };
    }
  }

  /**
   * List all active sessions
   * @returns {Promise<Array>}
   */
  async listSessions() {
    try {
      const result = await this._fetch('/session/list');
      return result.sessions || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // RPC INTERFACE
  // ==========================================================================

  /**
   * Make a raw RPC call
   * @param {string} method - RPC method name
   * @param {Object} [params={}] - RPC parameters
   * @returns {Promise<any>}
   */
  async rpc(method, params = {}) {
    const result = await this._fetch('/rpc', {
      method: 'POST',
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: Date.now(),
        method: method,
        params: params,
      }),
    });

    if (result.error) {
      throw new Error(result.error.message || JSON.stringify(result.error));
    }

    return result.result;
  }

  /**
   * Batch RPC calls
   * @param {Array<{method: string, params: Object}>} calls - Array of RPC calls
   * @returns {Promise<Array>}
   */
  async rpcBatch(calls) {
    const batch = calls.map((call, index) => ({
      jsonrpc: '2.0',
      id: index,
      method: call.method,
      params: call.params || {},
    }));

    const results = await this._fetch('/rpc', {
      method: 'POST',
      body: JSON.stringify(batch),
    });

    return results.map(r => r.result || r.error);
  }

  // ==========================================================================
  // ACCOUNT QUERIES
  // ==========================================================================

  /**
   * Check if an address exists (has any transactions)
   * @param {string} address - L1 address
   * @returns {Promise<boolean>}
   */
  async addressExists(address) {
    try {
      const result = await this._fetch(`/account/${address}/exists`);
      return result.exists === true;
    } catch {
      // Fallback: check balance
      const balance = await this.getBalance(address);
      return balance > 0;
    }
  }

  /**
   * Get account info
   * @param {string} address - L1 address
   * @returns {Promise<Object>}
   */
  async getAccountInfo(address) {
    try {
      return await this._fetch(`/account/${address}`);
    } catch {
      // Fallback
      const balance = await this.getBalance(address);
      return { address, balance, exists: balance > 0 };
    }
  }

  /**
   * Get account nonce (for replay protection)
   * @param {string} address - L1 address
   * @returns {Promise<number>}
   */
  async getNonce(address) {
    try {
      const result = await this._fetch(`/account/${address}/nonce`);
      return result.nonce || 0;
    } catch {
      return 0;
    }
  }

  // ==========================================================================
  // TOKEN INFO
  // ==========================================================================

  /**
   * Get token metadata
   * @returns {Promise<Object>}
   */
  async getTokenInfo() {
    try {
      return await this._fetch('/token/info');
    } catch {
      return {
        name: 'BlackBook Token',
        symbol: 'BB',
        decimals: 0,
        total_supply: await this.getTotalSupply(),
      };
    }
  }

  /**
   * Get top holders
   * @param {number} [limit=100] - Max holders to return
   * @returns {Promise<Array>}
   */
  async getTopHolders(limit = 100) {
    try {
      const result = await this._fetch(`/token/holders?limit=${limit}`);
      return result.holders || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // L2 CROSS-LAYER RPC (For L2 to call L1)
  // ==========================================================================

  /**
   * Verify an L1 signature (L2 calls this to validate cross-layer tx)
   * @param {string} publicKey - 64-char hex public key
   * @param {string} message - Original message that was signed
   * @param {string} signature - 128-char hex signature
   * @returns {Promise<Object>} { valid, wallet_address, message }
   */
  async verifySignature(publicKey, message, signature) {
    return await this._fetch('/rpc/verify-signature', {
      method: 'POST',
      body: JSON.stringify({
        public_key: publicKey,
        message: message,
        signature: signature,
      }),
    });
  }

  /**
   * Get cross-layer nonce for an address (replay protection)
   * @param {string} address - L1 address
   * @returns {Promise<Object>} { l1_nonce, cross_layer_nonce, next_valid_nonce }
   */
  async getCrossLayerNonce(address) {
    return await this._fetchWithRetry(`/rpc/nonce/${address}`);
  }

  /**
   * Record a market settlement on L1 (L2 calls this after resolving a market)
   * Creates an audit trail on L1 for regulatory compliance and dispute resolution.
   * @param {Object} settlement - Settlement data
   * @param {string} settlement.market_id - Market ID being settled
   * @param {string} settlement.outcome - Winning outcome
   * @param {Array<{address: string, amount: number}>} settlement.winners - Winner list
   * @param {number} settlement.l2_block_height - L2 block where settlement occurred
   * @param {string} settlement.l2_signature - L2 authority signature
   * @param {string} [settlement.market_title] - Human-readable market title
   * @param {number} [settlement.total_pool] - Total pool amount
   * @returns {Promise<Object>} { success, settlement_id, l1_tx_hash, l1_slot }
   */
  async recordSettlement(settlement) {
    return await this._fetch('/rpc/settlement', {
      method: 'POST',
      body: JSON.stringify(settlement),
    });
  }

  /**
   * Get a settlement record by ID
   * @param {string} settlementId - Settlement ID
   * @returns {Promise<Object>} Settlement record or { found: false }
   */
  async getSettlement(settlementId) {
    return await this._fetchWithRetry(`/rpc/settlement/${settlementId}`);
  }

  /**
   * Relay a signed action to L2 (L1 verifies signature and forwards to L2)
   * @param {Object} signedRequest - Signed request from wallet
   * @returns {Promise<Object>} { success, l2_response, wallet_address }
   */
  async relayToL2(signedRequest) {
    return await this._fetch('/rpc/relay', {
      method: 'POST',
      body: JSON.stringify(signedRequest),
    });
  }

  /**
   * Lookup L1 wallet address by user ID (for Supabase user lookup)
   * @param {string} userId - User ID from Supabase
   * @returns {Promise<Object>} { found, wallet_address, user_id }
   */
  async lookupWalletByUserId(userId) {
    return await this._fetchWithRetry(`/rpc/lookup/${userId}`);
  }

  /**
   * Request withdrawal from L2 to L1 (L2 authority calls this)
   * @param {Object} withdrawal - Withdrawal request
   * @param {string} withdrawal.user_address - L1 address to credit
   * @param {number} withdrawal.amount - Amount to credit
   * @param {string} withdrawal.l2_burn_tx - L2 burn transaction ID
   * @param {string} withdrawal.l2_signature - L2 authority signature
   * @returns {Promise<Object>} { success, new_balance, withdrawal_id }
   */
  async processWithdrawal(withdrawal) {
    return await this._fetch('/bridge/withdraw', {
      method: 'POST',
      body: JSON.stringify(withdrawal),
    });
  }

  /**
   * Get all settlements for a specific market
   * @param {string} marketId - Market ID
   * @returns {Promise<Array>} List of settlements
   */
  async getMarketSettlements(marketId) {
    try {
      const result = await this._fetch(`/settlements/market/${marketId}`);
      return result.settlements || [];
    } catch {
      return [];
    }
  }

  /**
   * Get all settlements for an address (as winner)
   * @param {string} address - L1 address
   * @returns {Promise<Array>} List of settlements
   */
  async getAddressSettlements(address) {
    try {
      const result = await this._fetch(`/settlements/address/${address}`);
      return result.settlements || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // UTILITY METHODS
  // ==========================================================================

  /**
   * Validate L1 address format
   * @param {string} address - Address to validate
   * @returns {boolean}
   */
  static isValidAddress(address) {
    if (!address || typeof address !== 'string') return false;
    // L1 + 14 hex chars = 16 total
    return /^L1[A-F0-9]{14}$/i.test(address);
  }

  /**
   * Format balance for display
   * @param {number} amount - Amount in BB
   * @param {number} [decimals=0] - Decimal places
   * @returns {string}
   */
  static formatBalance(amount, decimals = 0) {
    if (decimals === 0) {
      return `${amount.toLocaleString()} BB`;
    }
    return `${amount.toFixed(decimals)} BB`;
  }

  /**
   * Parse BB amount from string
   * @param {string} str - Amount string (e.g., "1,000 BB")
   * @returns {number}
   */
  static parseAmount(str) {
    const cleaned = str.replace(/[,\sBB]/gi, '');
    return parseInt(cleaned, 10) || 0;
  }
}

// ============================================================================
// EVENT SUBSCRIBER - WebSocket connection for real-time updates
// ============================================================================

export class L1EventSubscriber {
  /**
   * Create event subscriber
   * @param {string} wsUrl - WebSocket URL (e.g., ws://localhost:8080/ws)
   */
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.ws = null;
    this.handlers = {
      block: [],
      transaction: [],
      transfer: [],
      bridge: [],
      error: [],
      connected: [],
      disconnected: [],
    };
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 5;
    this.reconnectDelay = 1000;
  }

  /**
   * Connect to WebSocket
   */
  connect() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }

    this.ws = new WebSocket(this.wsUrl);

    this.ws.onopen = () => {
      console.log('[L1Events] Connected');
      this.reconnectAttempts = 0;
      this._emit('connected', { url: this.wsUrl });
    };

    this.ws.onclose = () => {
      console.log('[L1Events] Disconnected');
      this._emit('disconnected', {});
      this._attemptReconnect();
    };

    this.ws.onerror = (error) => {
      console.error('[L1Events] Error:', error);
      this._emit('error', { error });
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this._handleMessage(data);
      } catch (e) {
        console.error('[L1Events] Invalid message:', e);
      }
    };
  }

  /**
   * Disconnect from WebSocket
   */
  disconnect() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  /**
   * Subscribe to event type
   * @param {string} eventType - Event type (block, transaction, transfer, bridge)
   * @param {Function} handler - Event handler
   */
  on(eventType, handler) {
    if (this.handlers[eventType]) {
      this.handlers[eventType].push(handler);
    }
    return this;
  }

  /**
   * Unsubscribe from event type
   * @param {string} eventType - Event type
   * @param {Function} handler - Event handler
   */
  off(eventType, handler) {
    if (this.handlers[eventType]) {
      this.handlers[eventType] = this.handlers[eventType].filter(h => h !== handler);
    }
    return this;
  }

  /**
   * Subscribe to specific address events
   * @param {string} address - L1 address to watch
   */
  subscribeAddress(address) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        type: 'subscribe',
        address: address,
      }));
    }
  }

  /**
   * Unsubscribe from address events
   * @param {string} address - L1 address
   */
  unsubscribeAddress(address) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        type: 'unsubscribe',
        address: address,
      }));
    }
  }

  _emit(eventType, data) {
    if (this.handlers[eventType]) {
      this.handlers[eventType].forEach(handler => {
        try {
          handler(data);
        } catch (e) {
          console.error(`[L1Events] Handler error:`, e);
        }
      });
    }
  }

  _handleMessage(data) {
    const eventType = data.type || data.event;
    
    switch (eventType) {
      case 'new_block':
      case 'block':
        this._emit('block', data);
        break;
      case 'transaction':
      case 'tx':
        this._emit('transaction', data);
        break;
      case 'transfer':
        this._emit('transfer', data);
        break;
      case 'bridge':
      case 'bridge_event':
        this._emit('bridge', data);
        break;
      default:
        this._emit(eventType, data);
    }
  }

  _attemptReconnect() {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.error('[L1Events] Max reconnect attempts reached');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * this.reconnectAttempts;
    
    console.log(`[L1Events] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
    
    setTimeout(() => this.connect(), delay);
  }
}

// ============================================================================
// L2 RPC CLIENT - Specifically for L2 server to communicate with L1
// ============================================================================

/**
 * L2 RPC Client for L2 server to communicate with L1
 * Handles cross-layer verification, settlements, and bridge operations
 */
export class L2RpcClient {
  /**
   * Create L2 RPC Client
   * @param {string} [l1Url] - L1 server URL
   * @param {Object} [options] - Options
   */
  constructor(l1Url = null, options = {}) {
    this.l1Url = l1Url || (typeof process !== 'undefined' && process.env?.L1_URL) || 'http://localhost:8080';
    this.l2Authority = options.l2Authority || null;  // L2's signing authority
    this.debug = options.debug || false;
  }

  _log(...args) {
    if (this.debug) {
      console.log('[L2RPC]', ...args);
    }
  }

  async _post(endpoint, body) {
    const response = await fetch(`${this.l1Url}${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return await response.json();
  }

  async _get(endpoint) {
    const response = await fetch(`${this.l1Url}${endpoint}`);
    return await response.json();
  }

  // ==========================================================================
  // SIGNATURE VERIFICATION
  // ==========================================================================

  /**
   * Verify that a user's L1 signature is valid
   * Call this before accepting any signed action from a user
   * @param {string} publicKey - User's public key
   * @param {string} message - Message that was signed
   * @param {string} signature - User's signature
   * @returns {Promise<{valid: boolean, wallet_address: string}>}
   */
  async verifyUserSignature(publicKey, message, signature) {
    this._log('Verifying signature for', publicKey.slice(0, 16) + '...');
    return await this._post('/rpc/verify-signature', {
      public_key: publicKey,
      message: message,
      signature: signature,
    });
  }

  /**
   * Check if a signed request is valid and get the wallet address
   * @param {Object} signedRequest - Full signed request object
   * @returns {Promise<{valid: boolean, wallet_address: string}>}
   */
  async validateSignedRequest(signedRequest) {
    const { payload, timestamp, nonce, signature, public_key } = signedRequest;
    const signedContent = `${payload}\n${timestamp}\n${nonce}`;
    return await this.verifyUserSignature(public_key, signedContent, signature);
  }

  // ==========================================================================
  // CROSS-LAYER NONCES (Replay Protection)
  // ==========================================================================

  /**
   * Get the next valid cross-layer nonce for an address
   * Use this to prevent replay attacks
   * @param {string} address - L1 address
   * @returns {Promise<{l1_nonce: number, cross_layer_nonce: number, next_valid_nonce: number}>}
   */
  async getNonce(address) {
    this._log('Getting nonce for', address);
    return await this._get(`/rpc/nonce/${address}`);
  }

  /**
   * Validate that a nonce is valid (not already used)
   * @param {string} address - L1 address
   * @param {number} nonce - Nonce to validate
   * @returns {Promise<boolean>}
   */
  async validateNonce(address, nonce) {
    const { next_valid_nonce } = await this.getNonce(address);
    return nonce >= next_valid_nonce;
  }

  // ==========================================================================
  // BALANCE QUERIES
  // ==========================================================================

  /**
   * Get L1 balance for a user (before allowing L2 actions)
   * @param {string} address - L1 address
   * @returns {Promise<number>}
   */
  async getL1Balance(address) {
    const result = await this._get(`/balance/${address}`);
    return result.balance || 0;
  }

  /**
   * Get detailed balance breakdown
   * @param {string} address - L1 address
   * @returns {Promise<{available: number, locked: number, total: number}>}
   */
  async getBalanceBreakdown(address) {
    try {
      return await this._get(`/balance/${address}/details`);
    } catch {
      const balance = await this.getL1Balance(address);
      return { available: balance, locked: 0, total: balance };
    }
  }

  /**
   * Check if user has sufficient L1 balance for an action
   * @param {string} address - L1 address
   * @param {number} required - Required amount
   * @returns {Promise<boolean>}
   */
  async hasBalance(address, required) {
    const balance = await this.getL1Balance(address);
    return balance >= required;
  }

  // ==========================================================================
  // SETTLEMENTS (L2 → L1 Recording)
  // ==========================================================================

  /**
   * Record a market settlement on L1
   * Call this when L2 resolves a prediction market
   * @param {Object} params - Settlement parameters
   * @param {string} params.marketId - Market ID
   * @param {string} params.outcome - Winning outcome
   * @param {Array<{address: string, amount: number}>} params.winners - Winners
   * @param {number} params.l2BlockHeight - L2 block height
   * @param {string} [params.marketTitle] - Market title
   * @param {number} [params.totalPool] - Total pool amount
   * @returns {Promise<{success: boolean, settlement_id: string, l1_tx_hash: string}>}
   */
  async recordSettlement({ marketId, outcome, winners, l2BlockHeight, marketTitle = '', totalPool = 0 }) {
    this._log('Recording settlement for market:', marketId);
    
    return await this._post('/rpc/settlement', {
      market_id: marketId,
      outcome: outcome,
      winners: winners,
      l2_block_height: l2BlockHeight,
      l2_signature: this.l2Authority || 'L2_AUTHORITY',
      market_title: marketTitle,
      total_pool: totalPool,
    });
  }

  /**
   * Get a settlement record
   * @param {string} settlementId - Settlement ID
   * @returns {Promise<Object>}
   */
  async getSettlement(settlementId) {
    return await this._get(`/rpc/settlement/${settlementId}`);
  }

  // ==========================================================================
  // WITHDRAWALS (L2 → L1 Token Unlock)
  // ==========================================================================

  /**
   * Process a withdrawal from L2 to L1
   * Call this when a user withdraws from L2
   * @param {string} userAddress - User's L1 address
   * @param {number} amount - Amount to credit on L1
   * @param {string} l2BurnTx - L2 burn transaction ID
   * @returns {Promise<{success: boolean, new_balance: number}>}
   */
  async processWithdrawal(userAddress, amount, l2BurnTx) {
    this._log('Processing withdrawal:', userAddress, amount);
    
    return await this._post('/bridge/withdraw', {
      user_address: userAddress,
      amount: amount,
      l2_burn_tx: l2BurnTx,
      l2_signature: this.l2Authority || 'L2_AUTHORITY',
    });
  }

  // ==========================================================================
  // BRIDGE STATUS
  // ==========================================================================

  /**
   * Get bridge statistics
   * @returns {Promise<Object>}
   */
  async getBridgeStats() {
    try {
      return await this._get('/bridge/stats');
    } catch {
      return { total_bridged: 0, total_withdrawn: 0 };
    }
  }

  /**
   * Get pending bridge operations
   * @returns {Promise<Array>}
   */
  async getPendingBridges() {
    try {
      const result = await this._get('/bridge/pending');
      return result.pending || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // CHAIN STATE
  // ==========================================================================

  /**
   * Get current L1 slot/block height
   * @returns {Promise<{block_height: number, slot: number}>}
   */
  async getChainState() {
    const stats = await this._get('/stats');
    return {
      block_height: stats.stats?.total_blocks || 0,
      slot: stats.stats?.total_blocks || 0,
    };
  }

  /**
   * Get PoH status
   * @returns {Promise<Object>}
   */
  async getPoHStatus() {
    try {
      return await this._get('/poh/status');
    } catch {
      return { poh: { current_slot: 0, is_running: false } };
    }
  }

  /**
   * Health check - verify L1 is reachable
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
}

// ============================================================================
// L2 CLIENT - Direct L2 Prediction Market API
// ============================================================================

/**
 * L2 Client for direct communication with L2 prediction market
 * Use this for frontend/dApp integration
 */
export class L2Client {
  /**
   * Create L2 Client
   * @param {string} [l2Url] - L2 server URL
   * @param {Object} [options] - Options
   */
  constructor(l2Url = null, options = {}) {
    this.l2Url = l2Url || DEFAULT_CONFIG.L2_URL;
    this.debug = options.debug || false;
    this.walletAddress = null;
    this.publicKey = null;
  }

  _log(...args) {
    if (this.debug) {
      console.log('[L2Client]', ...args);
    }
  }

  async _fetch(endpoint, options = {}) {
    const url = `${this.l2Url}${endpoint}`;
    this._log('Fetching:', url);
    
    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
    });
    
    if (!response.ok) {
      const error = await response.text();
      throw new Error(`HTTP ${response.status}: ${error}`);
    }
    
    return await response.json();
  }

  async _post(endpoint, body) {
    return await this._fetch(endpoint, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  // ==========================================================================
  // AUTHENTICATION
  // ==========================================================================

  /**
   * Connect wallet to L2
   * @param {string} address - L1 wallet address
   * @param {string} publicKey - Public key for signature verification
   * @returns {Promise<Object>}
   */
  async connectWallet(address, publicKey) {
    this._log('Connecting wallet:', address);
    
    const result = await this._post('/auth/connect', {
      address,
      public_key: publicKey,
      timestamp: getTimestamp(),
    });

    if (result.success) {
      this.walletAddress = address;
      this.publicKey = publicKey;
    }

    return result;
  }

  /**
   * Check if connected
   * @returns {boolean}
   */
  isConnected() {
    return !!this.walletAddress;
  }

  // ==========================================================================
  // MARKETS
  // ==========================================================================

  /**
   * Get all prediction markets
   * @returns {Promise<Array>}
   */
  async getMarkets() {
    const result = await this._fetch('/markets');
    return result.markets || [];
  }

  /**
   * Get specific market
   * @param {string} marketId - Market ID
   * @returns {Promise<Object>}
   */
  async getMarket(marketId) {
    return await this._fetch(`/markets/${marketId}`);
  }

  /**
   * Create a new market
   * @param {Object} market - Market config
   * @returns {Promise<Object>}
   */
  async createMarket(market) {
    return await this._post('/markets', market);
  }

  // ==========================================================================
  // BETTING
  // ==========================================================================

  /**
   * Place a signed bet
   * @param {Object} bet - Bet parameters
   * @param {string} bet.signature - Ed25519 signature
   * @param {string} bet.from_address - Wallet address
   * @param {string} bet.market_id - Market ID
   * @param {string} bet.option - '0' (YES) or '1' (NO)
   * @param {number} bet.amount - Bet amount
   * @param {number} bet.nonce - Transaction nonce
   * @param {number} bet.timestamp - Unix timestamp
   * @returns {Promise<Object>}
   */
  async placeBet(bet) {
    this._log('Placing bet:', bet.market_id, bet.amount);
    return await this._post('/bet/signed', bet);
  }

  /**
   * Get bets for an account
   * @param {string} account - Account name or address
   * @returns {Promise<Array>}
   */
  async getBets(account) {
    const result = await this._fetch(`/bets/${account}`);
    return result.bets || [];
  }

  // ==========================================================================
  // BALANCES
  // ==========================================================================

  /**
   * Get balance for an account
   * @param {string} account - Account name or address
   * @returns {Promise<Object>}
   */
  async getBalance(account) {
    return await this._fetch(`/balance/${account}`);
  }

  /**
   * Get detailed balance breakdown
   * @param {string} account - Account name or address
   * @returns {Promise<Object>}
   */
  async getBalanceDetails(account) {
    return await this._fetch(`/balance/details/${account}`);
  }

  /**
   * Transfer tokens
   * @param {string} from - From account
   * @param {string} to - To account
   * @param {number} amount - Amount
   * @returns {Promise<Object>}
   */
  async transfer(from, to, amount) {
    return await this._post('/transfer', { from, to, amount });
  }

  // ==========================================================================
  // RPC
  // ==========================================================================

  /**
   * Get nonce for an address
   * @param {string} address - Wallet address
   * @returns {Promise<Object>}
   */
  async getNonce(address) {
    return await this._fetch(`/rpc/nonce/${address}`);
  }

  // ==========================================================================
  // SETTLEMENT
  // ==========================================================================

  /**
   * Trigger batch settlement to L1
   * @returns {Promise<Object>}
   */
  async settleToL1() {
    return await this._post('/settle', {});
  }

  /**
   * Get settlement status
   * @returns {Promise<Object>}
   */
  async getSettlementStatus() {
    return await this._fetch('/settle/status');
  }

  /**
   * Sync balances from L1
   * @returns {Promise<Object>}
   */
  async syncFromL1() {
    return await this._post('/sync', {});
  }

  // ==========================================================================
  // LEDGER
  // ==========================================================================

  /**
   * Get ledger activity
   * @returns {Promise<Object>}
   */
  async getLedgerActivity() {
    return await this._fetch('/ledger');
  }

  // ==========================================================================
  // HEALTH
  // ==========================================================================

  /**
   * Health check
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
}

// ============================================================================
// CONVENIENCE FACTORY
// ============================================================================

/**
 * Create L1 client with default settings
 * @param {string} [url] - L1 URL
 * @returns {L1Client}
 */
export function createL1Client(url = null) {
  return new L1Client(url);
}

/**
 * Create L2 client with default settings
 * @param {string} [url] - L2 URL
 * @returns {L2Client}
 */
export function createL2Client(url = null) {
  return new L2Client(url);
}

/**
 * Create event subscriber with default settings
 * @param {string} [wsUrl] - WebSocket URL
 * @returns {L1EventSubscriber}
 */
export function createEventSubscriber(wsUrl = 'ws://localhost:8080/ws') {
  return new L1EventSubscriber(wsUrl);
}

/**
 * Create L2 RPC client for L2 server to communicate with L1
 * @param {string} [l1Url] - L1 URL
 * @param {Object} [options] - Options
 * @returns {L2RpcClient}
 */
export function createL2RpcClient(l1Url = null, options = {}) {
  return new L2RpcClient(l1Url, options);
}

/**
 * Create a cross-layer bridge client
 * @param {string} [l1Url] - L1 URL
 * @param {string} [l2Url] - L2 URL
 * @returns {Object} - { l1, l2, l2rpc }
 */
export function createBridgeClient(l1Url = null, l2Url = null) {
  return {
    l1: new L1Client(l1Url),
    l2: new L2Client(l2Url),
    l2rpc: new L2RpcClient(l1Url),
  };
}

// ============================================================================
// EXPORTS
// ============================================================================

// Named exports
export { DEFAULT_CONFIG, generateNonce, getTimestamp };

// Default export
export default L1Client;

// CommonJS (for Node.js environments)
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { 
    L1Client, 
    L2Client,
    L1EventSubscriber, 
    L2RpcClient,
    createL1Client,
    createL2Client,
    createL2RpcClient,
    createEventSubscriber,
    createBridgeClient,
    generateNonce,
    getTimestamp,
    DEFAULT_CONFIG
  };
}

// Browser globals
if (typeof window !== 'undefined') {
  window.L1Client = L1Client;
  window.L2Client = L2Client;
  window.L1EventSubscriber = L1EventSubscriber;
  window.L2RpcClient = L2RpcClient;
  window.createL1Client = createL1Client;
  window.createL2Client = createL2Client;
  window.createL2RpcClient = createL2RpcClient;
  window.createEventSubscriber = createEventSubscriber;
  window.createBridgeClient = createBridgeClient;
}
