// ============================================================================
// Market ID Generator - JavaScript Utility
// ============================================================================
//
// Generates unique market IDs for RSS events.
// Used by the frontend and RSS feed processors.
//
// ID Format: rss_market_<hash>
// Hash is SHA-256 of: title + source + pub_date
//
// ============================================================================

/**
 * Generate a market ID from RSS event data
 * @param {Object} event - RSS event data
 * @param {string} event.meta_title - Event title
 * @param {string} event.source - Source URL
 * @param {string} event.pub_date - Publication date
 * @returns {Promise<string>} Unique market ID
 */
async function generateMarketId(event) {
  const { meta_title, source, pub_date } = event;
  
  // Create canonical string for hashing
  const canonical = `${meta_title}|${source}|${pub_date}`;
  
  // Generate SHA-256 hash
  const hash = await sha256(canonical);
  
  // Return formatted market ID
  return `rss_market_${hash.slice(0, 32)}`;
}

/**
 * Generate SHA-256 hash of a string
 * @param {string} message - String to hash
 * @returns {Promise<string>} Hex-encoded hash
 */
async function sha256(message) {
  // Browser environment
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const encoder = new TextEncoder();
    const data = encoder.encode(message);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
  }
  
  // Node.js environment
  if (typeof require !== 'undefined') {
    const crypto = require('crypto');
    return crypto.createHash('sha256').update(message).digest('hex');
  }
  
  // Fallback: simple hash (not cryptographically secure)
  let hash = 0;
  for (let i = 0; i < message.length; i++) {
    const char = message.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return Math.abs(hash).toString(16).padStart(32, '0');
}

/**
 * Generate a random market ID (for manual market creation)
 * @returns {string} Random market ID
 */
function generateRandomMarketId() {
  const timestamp = Date.now().toString(16);
  const random = Math.random().toString(16).slice(2, 10);
  return `market_${timestamp}_${random}`;
}

/**
 * Validate a market ID format
 * @param {string} marketId - Market ID to validate
 * @returns {boolean} True if valid format
 */
function isValidMarketId(marketId) {
  if (!marketId || typeof marketId !== 'string') {
    return false;
  }
  
  // Check length (reasonable bounds)
  if (marketId.length < 8 || marketId.length > 100) {
    return false;
  }
  
  // Check format: alphanumeric with underscores/hyphens
  const validPattern = /^[a-zA-Z0-9_-]+$/;
  return validPattern.test(marketId);
}

/**
 * Parse market ID to extract type and hash
 * @param {string} marketId - Market ID to parse
 * @returns {Object} Parsed components
 */
function parseMarketId(marketId) {
  const parts = marketId.split('_');
  
  if (parts.length < 2) {
    return { type: 'unknown', hash: marketId };
  }
  
  return {
    type: parts[0],           // 'rss', 'ai', 'market', etc.
    prefix: parts.slice(0, -1).join('_'),
    hash: parts[parts.length - 1]
  };
}

// ============================================================================
// RSS EVENT PAYLOAD BUILDER
// ============================================================================

/**
 * Build a complete RSS event payload for market initialization
 * @param {Object} options - Event options
 * @returns {Promise<Object>} Complete RSS event payload
 */
async function buildRssEventPayload(options) {
  const {
    title,
    description,
    source,
    pubDate = new Date().toISOString(),
    resolutionDate,
    freezeDate,
    outcomes = ['Yes', 'No Change', 'No'],
    initialOdds = [0.49, 0.02, 0.49],
    category = 'general',
    confidence = 0.7,
    resolutionRules = null
  } = options;
  
  // Generate market ID
  const marketId = await generateMarketId({
    meta_title: title,
    source,
    pub_date: pubDate
  });
  
  // Validate odds sum to 1.0
  const oddsSum = initialOdds.reduce((a, b) => a + b, 0);
  if (Math.abs(oddsSum - 1.0) > 0.01) {
    throw new Error(`initial_odds must sum to 1.0 (got ${oddsSum})`);
  }
  
  // Validate outcomes match odds
  if (outcomes.length !== initialOdds.length) {
    throw new Error(`outcomes count (${outcomes.length}) must match initial_odds count (${initialOdds.length})`);
  }
  
  return {
    market_id: marketId,
    meta_title: title,
    meta_description: description,
    market_type: outcomes.length === 2 ? 'binary' : 'three_choice',
    outcomes,
    initial_odds: initialOdds,
    source,
    pub_date: pubDate,
    resolution_date: resolutionDate,
    freeze_date: freezeDate || resolutionDate, // Default freeze = resolution
    resolution_rules: resolutionRules,
    category,
    confidence
  };
}

// ============================================================================
// EXPORTS
// ============================================================================

// ES Module exports
export {
  generateMarketId,
  generateRandomMarketId,
  isValidMarketId,
  parseMarketId,
  buildRssEventPayload,
  sha256
};

// CommonJS exports
if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    generateMarketId,
    generateRandomMarketId,
    isValidMarketId,
    parseMarketId,
    buildRssEventPayload,
    sha256
  };
}

// Browser global
if (typeof window !== 'undefined') {
  window.MarketId = {
    generate: generateMarketId,
    generateRandom: generateRandomMarketId,
    isValid: isValidMarketId,
    parse: parseMarketId,
    buildPayload: buildRssEventPayload
  };
}
