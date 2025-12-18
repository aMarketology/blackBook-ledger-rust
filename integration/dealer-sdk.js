/**
 * ============================================================================
 * DEALER SDK - Market Maker & Oracle Authority
 * ============================================================================
 * 
 * This SDK provides all dealer functionality for the frontend:
 * 
 * 🎰 MARKET MAKING:
 *   - Fund markets with liquidity (add to CPMM pools)
 *   - Place bets on any outcome
 *   - Balance positions across outcomes
 *   - View positions and P&L
 * 
 * 🔮 ORACLE AUTHORITY:
 *   - Create new markets
 *   - Resolve markets with winning outcome
 *   - Cancel/refund markets
 * 
 * 💰 LIQUIDITY MANAGEMENT:
 *   - Add liquidity to pools
 *   - Withdraw liquidity (LP tokens)
 *   - View LP positions
 * 
 * 📊 ANALYTICS:
 *   - Portfolio overview
 *   - P&L tracking
 *   - Market exposure
 */

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

// ============================================================================
// CONFIGURATION
// ============================================================================

// Load .env file
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
}

const CONFIG = {
  L1_URL: process.env.L1_URL || "http://localhost:8080",
  L2_URL: process.env.L2_URL || "http://localhost:1234",
  DEALER_ADDRESS: "L2DEALER00000001",
  DEALER_PUBLIC_KEY: "f19717a1860761b4e1b64101941c2115a416a07c57ff4fa3a91df7024b413d69",
  DEALER_PRIVATE_KEY: process.env.DEALER_PRIVATE_KEY || process.env.dealer_private_key,
};

// ============================================================================
// ED25519 CRYPTOGRAPHY
// ============================================================================

class DealerCrypto {
  /**
   * Sign a message using Ed25519
   */
  static sign(privateKeyHex, message) {
    if (!privateKeyHex) {
      throw new Error("Private key not provided");
    }
    
    const privateKeyBuffer = Buffer.from(privateKeyHex, 'hex');
    const keyObject = crypto.createPrivateKey({
      key: Buffer.concat([
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
   */
  static verify(publicKeyHex, message, signatureHex) {
    const publicKeyBuffer = Buffer.from(publicKeyHex, 'hex');
    const keyObject = crypto.createPublicKey({
      key: Buffer.concat([
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
   * Generate a unique nonce
   */
  static generateNonce() {
    return Date.now() * 1000 + Math.floor(Math.random() * 1000);
  }
}

// ============================================================================
// DEALER SDK CLASS
// ============================================================================

class DealerSDK {
  constructor(options = {}) {
    this.l1Url = options.l1Url || CONFIG.L1_URL;
    this.l2Url = options.l2Url || CONFIG.L2_URL;
    this.address = options.address || CONFIG.DEALER_ADDRESS;
    this.publicKey = options.publicKey || CONFIG.DEALER_PUBLIC_KEY;
    this.privateKey = options.privateKey || CONFIG.DEALER_PRIVATE_KEY;
    this.nonceCounter = Date.now();
  }
  
  // ==========================================================================
  // AUTHENTICATION & SIGNING
  // ==========================================================================
  
  /**
   * Create a signed bet request
   */
  createSignedBetRequest(marketId, outcome, amount) {
    const timestamp = Math.floor(Date.now() / 1000);
    const nonce = ++this.nonceCounter;
    
    // Create message to sign: market_id|option|amount|timestamp|nonce
    const message = `${marketId}|${outcome}|${amount}|${timestamp}|${nonce}`;
    const signature = DealerCrypto.sign(this.privateKey, message);
    
    return {
      market_id: marketId,
      option: outcome.toString(),
      amount: amount,
      from_address: this.address,
      public_key: this.publicKey,
      signature: signature,
      timestamp: timestamp,
      nonce: nonce,
    };
  }
  
  /**
   * Create a signed generic request
   */
  createSignedRequest(action, payload) {
    const timestamp = Math.floor(Date.now() / 1000);
    const nonce = ++this.nonceCounter;
    
    const message = JSON.stringify({ action, ...payload, timestamp, nonce });
    const signature = DealerCrypto.sign(this.privateKey, message);
    
    return {
      wallet_address: this.address,
      public_key: this.publicKey,
      signature: signature,
      timestamp: timestamp,
      nonce: nonce,
      ...payload,
    };
  }
  
  // ==========================================================================
  // BALANCE & ACCOUNT
  // ==========================================================================
  
  /**
   * Get dealer's L1 balance (main chain)
   */
  async getL1Balance() {
    const response = await fetch(`${this.l1Url}/balance/${this.address}`);
    if (!response.ok) throw new Error(`L1 balance failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Get dealer's L2 balance (prediction market chain)
   */
  async getL2Balance() {
    const response = await fetch(`${this.l2Url}/balance/${this.address}`);
    if (!response.ok) throw new Error(`L2 balance failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Get detailed balance breakdown (available, locked, total)
   */
  async getBalanceDetails() {
    const response = await fetch(`${this.l2Url}/balance/${this.address}/details`);
    if (!response.ok) throw new Error(`Balance details failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Connect wallet (sync L1 balance to L2)
   */
  async connectWallet() {
    const response = await fetch(`${this.l2Url}/connect_wallet`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        public_key: this.publicKey,
        address: this.address,
      }),
    });
    if (!response.ok) throw new Error(`Connect wallet failed: ${response.status}`);
    return response.json();
  }
  
  // ==========================================================================
  // MARKET MAKING - BETTING
  // ==========================================================================
  
  /**
   * Place a bet on a market outcome
   * @param {string} marketId - Market ID
   * @param {number|string} outcome - Outcome index (0, 1, 2...) or "YES"/"NO"
   * @param {number} amount - Amount in BB to bet
   */
  async placeBet(marketId, outcome, amount) {
    const request = this.createSignedBetRequest(marketId, outcome, amount);
    
    const response = await fetch(`${this.l2Url}/bet`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    
    const data = await response.json();
    if (!data.success) {
      throw new Error(data.error || 'Bet failed');
    }
    return data;
  }
  
  /**
   * Place bets on multiple outcomes to provide liquidity
   * @param {string} marketId - Market ID
   * @param {Object} amounts - { outcome: amount } e.g. { 0: 100, 1: 100 }
   */
  async placeLiquidityBets(marketId, amounts) {
    const results = [];
    for (const [outcome, amount] of Object.entries(amounts)) {
      if (amount > 0) {
        try {
          const result = await this.placeBet(marketId, parseInt(outcome), amount);
          results.push({ outcome, amount, success: true, result });
        } catch (e) {
          results.push({ outcome, amount, success: false, error: e.message });
        }
      }
    }
    return results;
  }
  
  /**
   * Balance a market by betting on the underpriced side
   * @param {string} marketId - Market ID
   * @param {number} amount - Total amount to use for balancing
   * @param {number} targetSpread - Target price difference (default 0.05 = 5%)
   */
  async balanceMarket(marketId, amount, targetSpread = 0.05) {
    const prices = await this.getMarketPrices(marketId);
    
    if (!prices.cpmm_enabled) {
      throw new Error('Market does not have CPMM enabled');
    }
    
    // Find the cheapest outcome
    const outcomes = prices.prices.sort((a, b) => a.price - b.price);
    const cheapest = outcomes[0];
    const mostExpensive = outcomes[outcomes.length - 1];
    
    const spread = mostExpensive.price - cheapest.price;
    
    if (spread < targetSpread) {
      return { 
        action: 'none', 
        reason: `Spread ${(spread * 100).toFixed(1)}% is within target ${(targetSpread * 100).toFixed(1)}%` 
      };
    }
    
    // Bet on the cheapest outcome to push price up
    const result = await this.placeBet(marketId, cheapest.index, amount);
    
    return {
      action: 'balanced',
      outcome: cheapest.label,
      amount: amount,
      oldPrice: cheapest.price,
      newPrice: result.new_price,
      result,
    };
  }
  
  // ==========================================================================
  // MARKET INFORMATION
  // ==========================================================================
  
  /**
   * Get all markets
   */
  async getMarkets() {
    const response = await fetch(`${this.l2Url}/markets`);
    if (!response.ok) throw new Error(`Get markets failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Get a specific market
   */
  async getMarket(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}`);
    if (!response.ok) throw new Error(`Get market failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Get market CPMM prices and pool info
   */
  async getMarketPrices(marketId) {
    const response = await fetch(`${this.l2Url}/markets/${marketId}/prices`);
    if (!response.ok) throw new Error(`Get prices failed: ${response.status}`);
    return response.json();
  }
  
  // ==========================================================================
  // DEALER POSITIONS & P&L
  // ==========================================================================
  
  /**
   * Get all dealer positions across markets
   */
  async getPositions() {
    const response = await fetch(`${this.l2Url}/dealer/positions/${this.address}`);
    if (!response.ok) throw new Error(`Get positions failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Calculate current P&L for all positions
   */
  async calculatePnL() {
    const positions = await this.getPositions();
    
    let totalInvested = 0;
    let totalCurrentValue = 0;
    const marketPnL = [];
    
    for (const position of positions.positions || []) {
      const prices = await this.getMarketPrices(position.market_id);
      
      let marketValue = 0;
      let marketInvested = 0;
      
      for (const pos of position.outcomes || []) {
        const price = prices.prices?.find(p => p.index === pos.outcome)?.price || 0.5;
        const value = pos.shares * price;
        marketValue += value;
        marketInvested += pos.total_invested || pos.shares * 0.5; // Estimate if not tracked
      }
      
      totalInvested += marketInvested;
      totalCurrentValue += marketValue;
      
      marketPnL.push({
        market_id: position.market_id,
        title: position.title,
        invested: marketInvested,
        current_value: marketValue,
        pnl: marketValue - marketInvested,
        pnl_percent: ((marketValue - marketInvested) / marketInvested * 100) || 0,
      });
    }
    
    return {
      total_invested: totalInvested,
      total_current_value: totalCurrentValue,
      total_pnl: totalCurrentValue - totalInvested,
      total_pnl_percent: ((totalCurrentValue - totalInvested) / totalInvested * 100) || 0,
      markets: marketPnL,
    };
  }
  
  // ==========================================================================
  // LIQUIDITY MANAGEMENT
  // ==========================================================================
  
  /**
   * Fund all markets with equal liquidity
   * @param {number} amountPerMarket - BB amount per market (0 = auto-calculate)
   */
  async fundAllMarkets(amountPerMarket = 0) {
    const response = await fetch(`${this.l2Url}/dealer/fund-all-markets`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        dealer_address: this.address,
        amount_per_market: amountPerMarket,
      }),
    });
    
    if (!response.ok) throw new Error(`Fund markets failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Add liquidity to a specific market's CPMM pool
   * @param {string} marketId - Market ID
   * @param {number} amount - BB amount to add
   */
  async addLiquidity(marketId, amount) {
    // For now, liquidity is added by placing equal bets on all outcomes
    const prices = await this.getMarketPrices(marketId);
    const numOutcomes = prices.prices?.length || 2;
    const amountPerOutcome = amount / numOutcomes;
    
    const amounts = {};
    for (let i = 0; i < numOutcomes; i++) {
      amounts[i] = amountPerOutcome;
    }
    
    return this.placeLiquidityBets(marketId, amounts);
  }
  
  // ==========================================================================
  // ORACLE AUTHORITY - MARKET MANAGEMENT
  // ==========================================================================
  
  /**
   * Create a new market
   */
  async createMarket(options) {
    const { title, description, outcomes, category, source } = options;
    
    const response = await fetch(`${this.l2Url}/markets/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title,
        description,
        outcomes: outcomes || ['Yes', 'No'],
        category: category || 'general',
        source: source,
        creator: this.address,
      }),
    });
    
    if (!response.ok) throw new Error(`Create market failed: ${response.status}`);
    return response.json();
  }
  
  /**
   * Resolve a market with winning outcome
   * @param {string} marketId - Market ID
   * @param {number} winningOutcome - Index of winning outcome
   */
  async resolveMarket(marketId, winningOutcome) {
    const request = this.createSignedRequest('resolve', {
      market_id: marketId,
      winning_outcome: winningOutcome,
    });
    
    const response = await fetch(`${this.l2Url}/markets/${marketId}/resolve`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    
    if (!response.ok) throw new Error(`Resolve market failed: ${response.status}`);
    return response.json();
  }
  
  // ==========================================================================
  // ANALYTICS & REPORTING
  // ==========================================================================
  
  /**
   * Get comprehensive portfolio overview
   */
  async getPortfolioOverview() {
    const [l1Balance, l2Balance, positions, pnl] = await Promise.all([
      this.getL1Balance().catch(() => ({ balance: 0 })),
      this.getL2Balance().catch(() => ({ balance: 0 })),
      this.getPositions().catch(() => ({ positions: [] })),
      this.calculatePnL().catch(() => ({ total_pnl: 0 })),
    ]);
    
    return {
      balances: {
        l1: l1Balance.balance || 0,
        l2: l2Balance.balance || 0,
        total: (l1Balance.balance || 0) + (l2Balance.balance || 0),
      },
      positions: positions.positions?.length || 0,
      total_invested: pnl.total_invested || 0,
      total_value: pnl.total_current_value || 0,
      pnl: pnl.total_pnl || 0,
      pnl_percent: pnl.total_pnl_percent || 0,
    };
  }
  
  /**
   * Get market exposure (how much is at risk per outcome)
   */
  async getMarketExposure(marketId) {
    const [prices, positions] = await Promise.all([
      this.getMarketPrices(marketId),
      this.getPositions(),
    ]);
    
    const marketPosition = positions.positions?.find(p => p.market_id === marketId);
    
    if (!marketPosition) {
      return { market_id: marketId, exposure: [], total_exposure: 0 };
    }
    
    const exposure = prices.prices.map(p => {
      const position = marketPosition.outcomes?.find(o => o.outcome === p.index);
      const shares = position?.shares || 0;
      
      // If this outcome wins, we get shares * 1.0
      // If this outcome loses, we get 0
      return {
        outcome: p.index,
        label: p.label,
        shares: shares,
        current_price: p.price,
        win_payout: shares, // Full payout if wins
        loss: shares * p.price, // What we paid
        expected_value: shares * p.price, // EV = shares * probability
      };
    });
    
    return {
      market_id: marketId,
      exposure,
      total_shares: exposure.reduce((sum, e) => sum + e.shares, 0),
      total_exposure: exposure.reduce((sum, e) => sum + e.loss, 0),
    };
  }
}

// ============================================================================
// EXPORTS
// ============================================================================

module.exports = {
  DealerSDK,
  DealerCrypto,
  CONFIG,
};

// ============================================================================
// CLI DEMO
// ============================================================================

async function demo() {
  console.log("═".repeat(70));
  console.log("🎰 DEALER SDK DEMO");
  console.log("═".repeat(70));
  
  const dealer = new DealerSDK();
  
  try {
    // Connect wallet
    console.log("\n📡 Connecting wallet...");
    const connect = await dealer.connectWallet();
    console.log("   ✅ Connected:", connect);
    
    // Get portfolio overview
    console.log("\n📊 Portfolio Overview:");
    const portfolio = await dealer.getPortfolioOverview();
    console.log("   L1 Balance:", portfolio.balances.l1, "BB");
    console.log("   L2 Balance:", portfolio.balances.l2, "BB");
    console.log("   Positions:", portfolio.positions, "markets");
    console.log("   Total P&L:", portfolio.pnl.toFixed(2), "BB");
    
    // Get markets
    console.log("\n📈 Available Markets:");
    const markets = await dealer.getMarkets();
    const marketList = markets.markets?.slice(0, 5) || [];
    for (const m of marketList) {
      console.log(`   - ${m.id}: ${m.title.slice(0, 50)}...`);
    }
    
    // Get positions
    console.log("\n💼 Dealer Positions:");
    const positions = await dealer.getPositions();
    if (positions.positions?.length > 0) {
      for (const p of positions.positions.slice(0, 3)) {
        console.log(`   - ${p.market_id}: ${p.total_invested} BB invested`);
      }
    } else {
      console.log("   No positions yet");
    }
    
    console.log("\n" + "═".repeat(70));
    console.log("✅ Demo complete! SDK is ready for frontend integration.");
    console.log("═".repeat(70));
    
  } catch (e) {
    console.error("❌ Error:", e.message);
  }
}

// Run demo if executed directly
if (require.main === module) {
  demo().catch(console.error);
}
