// ============================================================================
// BLACKBOOK UNIFIED WALLET SDK
// ============================================================================
//
// Single wallet experience for L1 (vault) and L2 (gaming/betting)
// Full control of both layers with Ed25519 cryptographic signing
//
// THE FLOW:
// ┌──────────────┐      "Bridge"      ┌──────────────┐      "Bet"       
// │ L1 Wallet    │ ─────────────────▶ │ L2 Session   │ ──────────────▶ WIN/LOSE
// │ 10,000 BB    │   Lock funds       │ 5,000 BB     │   Fast betting   
// └──────────────┘                    └──────────────┘                  
//        ▲                                                    │
//        └────────────────────────────────────────────────────┘
//                            "Withdraw" (profit returned!)
//
// FEATURES:
//   L1 BLOCKCHAIN:
//   - Balance & transfer with Ed25519 signatures
//   - Solana-compatible RPC (getRecentBlockhash, getSlot, etc.)
//   - Block operations (getBlock, getLatestBlock, getBlocks)
//   - Signature verification (verifySignature)
//   - Social mining (createSocialPost, getSocialStats)
//   - Bridge escrow (bridgeInitiate, bridgeRelease)
//
//   L2 PREDICTION MARKET:
//   - Signed betting (placeBet with Ed25519)
//   - Shares system (mintShares, redeemShares)
//   - Orderbook/CLOB (placeLimitOrder, getOrderbook)
//   - Market resolution (resolveMarket, claimWinnings)
//   - Oracle management (listOracles, addOracle)
//   - Session management (startSession, settleSession)
//
// USAGE:
//   import { UnifiedWallet } from './unified-wallet-sdk.js';
//   
//   // Connect to Alice or Bob (real accounts on L1/L2)
//   const alice = await UnifiedWallet.connect('alice');
//   const bob = await UnifiedWallet.connect('bob');
//   
//   // Or connect with custom credentials
//   const wallet = await UnifiedWallet.connect({
//     private_key: '...',
//     address: '...'
//   });
//   
//   console.log(alice.balance); // 10000 BB
//   
//   // Bridge to L2 for betting
//   await alice.bridgeToL2(5000);
//   
//   // Place bet on L2 (Ed25519 signed)
//   await alice.placeBet('btc_100k_2025', 'yes', 1000);
//   
//   // Check blockhash (Solana-compatible)
//   const { blockhash } = await alice.getRecentBlockhash();
//   
//   // Withdraw winnings back to L1
//   await alice.withdraw();
//
// RUN DEMO:
//   bun unified-wallet-sdk.js
//   node unified-wallet-sdk.js
//
// ============================================================================

import nacl from 'tweetnacl';
import * as bip39 from 'bip39';
import { derivePath } from 'ed25519-hd-key';
import CryptoJS from 'crypto-js';
import { createClient } from '@supabase/supabase-js';

// ============================================================================
// CONFIGURATION - Change these if your servers are on different ports
// ============================================================================

// Detect if we're running in browser dev mode (Vite)
const isBrowser = typeof window !== 'undefined';
const isDev = isBrowser && (import.meta?.env?.DEV || window.location.hostname === 'localhost');

const CONFIG = {
  // Use Vite proxy in development to avoid CORS, direct URLs in production/Node
  L1_URL: isDev ? '/api/l1' : (process.env.L1_URL || 'http://localhost:8080'),
  L2_URL: isDev ? '/api/l2' : (process.env.L2_URL || 'http://localhost:1234'),
  SUPABASE_URL: import.meta?.env?.VITE_SUPABASE_URL || '',
  SUPABASE_KEY: import.meta?.env?.VITE_SUPABASE_ANON_KEY || '',
};

// BIP-44 Derivation Path for BlackBook (SLIP-0010 compatible)
const L1_DERIVATION_PATH = "m/44'/1337'/0'/0'/0'";

// Password Fork Constants (must match backend)
const AUTH_CONSTANT = 'BLACKBOOK_AUTH_V1';
const WALLET_CONSTANT = 'BLACKBOOK_WALLET_V1';

// ============================================================================
// BUILT-IN ACCOUNTS - Alice & Bob (real accounts on L1/L2)
// ============================================================================

const ACCOUNTS = {
  alice: {
    name: 'Alice',
    username: 'alice_test',
    email: 'alice@blackbook.test',
    address: 'L1ALICE000000001',
    public_key: '4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57',
    private_key: '616c6963655f746573745f6163636f756e745f76310000000000000000000001',
  },
  bob: {
    name: 'Bob', 
    username: 'bob_test',
    email: 'bob@blackbook.test',
    address: 'L1BOB0000000001',
    public_key: 'b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f',
    private_key: '626f625f746573745f6163636f756e745f763100000000000000000000000002',
  }
};

// ============================================================================
// CRYPTO HELPERS
// ============================================================================

function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return bytes;
}

function bytesToHex(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

function generateNonce() {
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return bytesToHex(array);
}

/**
 * Generate cryptographically secure salt (32 bytes)
 * @returns {string} 64-character hex string
 */
function generateSalt() {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return bytesToHex(array);
}

// ============================================================================
// PASSWORD FORK - Split Password into Login + Wallet Secrets
// ============================================================================
//
// User Password + Salt
//         ↓
//    ┌────┴────┐
//    ↓         ↓
// Path A      Path B  
// (Fast)      (Slow)
//    ↓         ↓
// Login      Wallet
// Password   Key
//    ↓         ↓
// Supabase   Encrypt
// Auth       Mnemonic
//
// ============================================================================

/**
 * Derive Login Password (Path A) - sent to Supabase for authentication
 * Formula: SHA256(password + salt + AUTH_CONSTANT)
 * @param {string} password - User's plaintext password
 * @param {string} salt - User's unique salt (64 hex chars)
 * @returns {string} 64-character hex hash
 */
function deriveLoginPassword(password, salt) {
  const input = password + salt + AUTH_CONSTANT;
  return CryptoJS.SHA256(input).toString(CryptoJS.enc.Hex);
}

/**
 * Derive Wallet Key (Path B) - NEVER sent to server
 * Uses PBKDF2 with 600k iterations for security (OWASP 2024)
 * @param {string} password - User's plaintext password
 * @param {string} salt - User's unique salt (64 hex chars)
 * @returns {string} 64-character hex key
 */
function deriveWalletKey(password, salt) {
  const input = password + salt + WALLET_CONSTANT;
  const key = CryptoJS.PBKDF2(input, salt, {
    keySize: 256 / 32,
    iterations: 600000,  // OWASP 2024: 600k+ for PBKDF2-SHA256
    hasher: CryptoJS.algo.SHA256
  });
  return key.toString(CryptoJS.enc.Hex);
}

/**
 * Fork password into two separate secrets
 * @param {string} password - User's plaintext password
 * @param {string} [existingSalt] - Optional existing salt (for login)
 * @returns {Object} { loginPassword, walletKey, salt }
 */
function forkPassword(password, existingSalt = null) {
  const salt = existingSalt || generateSalt();
  return {
    loginPassword: deriveLoginPassword(password, salt),
    walletKey: deriveWalletKey(password, salt),
    salt
  };
}

// ============================================================================
// VAULT ENCRYPTION - AES-256-GCM (Authenticated Encryption)
// ============================================================================

/**
 * Encrypt vault contents with Wallet Key using AES-256-GCM
 * Uses Web Crypto API for authenticated encryption (prevents bit-flipping attacks)
 * @param {Object} contents - Vault data to encrypt (e.g., { mnemonic })
 * @param {string} walletKey - 64 hex char key from password fork
 * @returns {Promise<Object>} { encrypted_blob, nonce, auth_tag }
 */
async function encryptVault(contents, walletKey) {
  const plaintext = new TextEncoder().encode(JSON.stringify(contents));
  
  // Generate 12-byte nonce (96 bits - recommended for GCM)
  const nonce = new Uint8Array(12);
  if (typeof crypto === 'undefined' || !crypto.getRandomValues) {
    throw new Error('Secure random not available. Use a modern browser with Web Crypto API.');
  }
  crypto.getRandomValues(nonce);
  
  // Import key for AES-GCM
  const keyBytes = hexToBytes(walletKey.slice(0, 64));
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    keyBytes,
    { name: 'AES-GCM' },
    false,
    ['encrypt']
  );
  
  // Encrypt with AES-GCM (includes authentication tag)
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv: nonce, tagLength: 128 },
    cryptoKey,
    plaintext
  );
  
  return {
    encrypted_blob: btoa(String.fromCharCode(...new Uint8Array(ciphertext))),
    nonce: bytesToHex(nonce)
  };
}

/**
 * Decrypt vault contents with Wallet Key using AES-256-GCM
 * Uses Web Crypto API for authenticated decryption
 * @param {string} encryptedBlob - Base64 encoded ciphertext (includes auth tag)
 * @param {string} nonce - 24 hex char nonce
 * @param {string} walletKey - 64 hex char key
 * @returns {Promise<Object>} Decrypted vault contents
 */
async function decryptVault(encryptedBlob, nonce, walletKey) {
  // Decode ciphertext from base64
  const ciphertext = Uint8Array.from(atob(encryptedBlob), c => c.charCodeAt(0));
  const nonceBytes = hexToBytes(nonce);
  
  // Import key for AES-GCM
  const keyBytes = hexToBytes(walletKey.slice(0, 64));
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    keyBytes,
    { name: 'AES-GCM' },
    false,
    ['decrypt']
  );
  
  try {
    // Decrypt with AES-GCM (verifies authentication tag)
    const plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: nonceBytes, tagLength: 128 },
      cryptoKey,
      ciphertext
    );
    
    const decoded = new TextDecoder().decode(plaintext);
    return JSON.parse(decoded);
  } catch (e) {
    throw new Error('Decryption failed - wrong password, corrupted vault, or tampered data');
  }
}

// ============================================================================
// BIP-39 MNEMONIC - Human-readable wallet backup
// ============================================================================

/**
 * Generate a new BIP-39 mnemonic phrase
 * @param {number} wordCount - 12 or 24 words
 * @returns {string} Space-separated mnemonic words
 */
function generateMnemonic(wordCount = 12) {
  const strength = wordCount === 24 ? 256 : 128;
  return bip39.generateMnemonic(strength);
}

/**
 * Validate a mnemonic phrase
 * @param {string} mnemonic
 * @returns {boolean}
 */
function validateMnemonic(mnemonic) {
  return bip39.validateMnemonic(mnemonic);
}

/**
 * Derive ed25519 keypair from mnemonic using SLIP-0010
 * @param {string} mnemonic - BIP-39 mnemonic phrase
 * @param {string} [derivationPath] - BIP-44 path (default: L1_DERIVATION_PATH)
 * @returns {Object} { publicKey, privateKey, publicKeyHex, privateKeyHex }
 */
function deriveKeypairFromMnemonic(mnemonic, derivationPath = L1_DERIVATION_PATH) {
  const seed = bip39.mnemonicToSeedSync(mnemonic);
  const seedHex = seed.toString('hex');
  const { key } = derivePath(derivationPath, seedHex);
  const keypair = nacl.sign.keyPair.fromSeed(key);
  
  return {
    publicKey: keypair.publicKey,
    privateKey: keypair.secretKey,
    publicKeyHex: bytesToHex(keypair.publicKey),
    privateKeyHex: bytesToHex(keypair.secretKey)
  };
}

/**
 * Generate L1 address from Ed25519 public key
 * Format: L1 + 14 hex chars (e.g., L148F582A1BC8976)
 * @param {string} publicKeyHex - 64-character hex public key
 * @returns {string} L1 address (16 characters)
 */
function generateL1Address(publicKeyHex) {
  const hash = CryptoJS.SHA256(publicKeyHex).toString(CryptoJS.enc.Hex);
  const shortHash = hash.slice(0, 14).toUpperCase();
  return `L1${shortHash}`;
}

// ============================================================================
// UNIFIED WALLET CLASS
// ============================================================================

export class UnifiedWallet {
  constructor(account) {
    this.name = account.name || 'Wallet';
    this.address = account.address;
    this.publicKey = account.public_key;
    this.email = account.email || '';
    this.username = account.username || '';
    
    // Set up signing keys (if private_key provided)
    if (account.private_key) {
      const seed = hexToBytes(account.private_key);
      const keyPair = nacl.sign.keyPair.fromSeed(seed);
      this._privateKey = keyPair.secretKey;
      this._publicKey = keyPair.publicKey;
    } else {
      this._privateKey = null;
      this._publicKey = null;
    }
    
    // Balance state (will be refreshed from server)
    this._l1Available = 0;
    this._l1Locked = 0;
    this._l2Balance = 0;
    
    // Full wallet state (for BIP-39 mode)
    this._mnemonic = null;
    this._salt = null;
    this._walletKey = null;
    this._isUnlocked = false;
    
    // Supabase client (lazy initialized)
    this._supabase = null;
  }

  // ==========================================================================
  // SUPABASE INTEGRATION
  // ==========================================================================

  /** Get Supabase client (lazy init) */
  get supabase() {
    if (!this._supabase && CONFIG.SUPABASE_URL && CONFIG.SUPABASE_KEY) {
      this._supabase = createClient(CONFIG.SUPABASE_URL, CONFIG.SUPABASE_KEY);
    }
    return this._supabase;
  }

  /** Set Supabase client (inject from app) */
  set supabase(client) {
    this._supabase = client;
  }

  /** Get salt */
  get salt() {
    return this._salt;
  }

  /** Set salt */
  set salt(value) {
    this._salt = value;
  }

  /** Get wallet key */
  get walletKey() {
    return this._walletKey;
  }

  /** Set wallet key */
  set walletKey(value) {
    this._walletKey = value;
  }

  /** Get userId */
  get userId() {
    return this._userId || this.username;
  }

  /** Set userId */
  set userId(value) {
    this._userId = value;
  }

  // ==========================================================================
  // CONNECT - Main entry point
  // ==========================================================================

  /**
   * Connect to a wallet
   * @param {string|Object} input - 'alice', 'bob', or { private_key, address }
   * @returns {Promise<UnifiedWallet>}
   * 
   * @example
   *   const alice = await UnifiedWallet.connect('alice');
   *   const bob = await UnifiedWallet.connect('bob');
   *   const custom = await UnifiedWallet.connect({ private_key: '...', address: '...' });
   */
  static async connect(input) {
    let account;
    
    if (typeof input === 'string') {
      // Connect by name (alice, bob)
      const name = input.toLowerCase();
      account = ACCOUNTS[name];
      
      if (!account) {
        // Try fetching from server
        try {
          const response = await fetch(`${CONFIG.L1_URL}/auth/test-accounts`);
          const data = await response.json();
          account = data[name];
        } catch (e) {
          // Server not available, use built-in accounts only
        }
      }
      
      if (!account) {
        throw new Error(`Unknown account: "${input}". Use 'alice' or 'bob'.`);
      }
    } else {
      // Connect with credentials object
      account = input;
    }
    
    const wallet = new UnifiedWallet(account);
    await wallet.refresh();
    
    console.log(`✅ Connected: ${wallet.name}`);
    console.log(`   Address: ${wallet.address}`);
    console.log(`   Balance: ${wallet.balance} BB\n`);
    
    return wallet;
  }

  // ==========================================================================
  // CREATE NEW WALLET - For real users
  // ==========================================================================

  /**
   * Create a brand new wallet with random keys
   * @returns {Promise<{wallet: UnifiedWallet, credentials: Object}>}
   * 
   * @example
   *   const { wallet, credentials } = await UnifiedWallet.create();
   *   // ⚠️ IMPORTANT: User must save credentials.private_key securely!
   *   console.log('Save this:', credentials.private_key);
   */
  static async create() {
    // Generate random seed (32 bytes) - MUST use cryptographic RNG
    const seed = new Uint8Array(32);
    if (typeof crypto === 'undefined' || !crypto.getRandomValues) {
      throw new Error(
        'Secure random number generator not available. ' +
        'Use a modern browser with Web Crypto API or Node.js 15+.'
      );
    }
    crypto.getRandomValues(seed);
    
    // Derive keypair from seed
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    
    // Create address from public key (L1 prefix + first 14 chars)
    const publicKeyHex = bytesToHex(keyPair.publicKey);
    const address = 'L1' + publicKeyHex.substring(0, 14).toUpperCase();
    
    const credentials = {
      address: address,
      public_key: publicKeyHex,
      private_key: bytesToHex(seed),  // 32-byte seed as hex
      created_at: new Date().toISOString()
    };
    
    // Try to register on L1 (for balance tracking)
    try {
      await fetch(`${CONFIG.L1_URL}/auth/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: credentials.address,
          public_key: credentials.public_key
        })
      });
    } catch {
      // Registration endpoint may not exist, that's OK
    }
    
    const wallet = await UnifiedWallet.connect({
      name: 'New Wallet',
      ...credentials
    });
    
    console.log('');
    console.log('⚠️  SAVE YOUR PRIVATE KEY - YOU CANNOT RECOVER IT!');
    console.log('═══════════════════════════════════════════════════════════');
    console.log(`   ${credentials.private_key}`);
    console.log('═══════════════════════════════════════════════════════════');
    console.log('');
    
    return { wallet, credentials };
  }

  /**
   * Import wallet from private key only
   * Derives address and public key automatically
   * @param {string} privateKeyHex - 64 hex characters (32 byte seed)
   * @returns {Promise<UnifiedWallet>}
   * 
   * @example
   *   const wallet = await UnifiedWallet.import('abc123def456...');
   */
  static async import(privateKeyHex) {
    // Clean input (remove whitespace, 0x prefix)
    const cleanKey = privateKeyHex.trim().toLowerCase().replace(/^0x/, '');
    
    if (cleanKey.length !== 64) {
      throw new Error(`Invalid private key. Expected 64 hex characters, got ${cleanKey.length}.`);
    }
    
    const seed = hexToBytes(cleanKey);
    
    if (seed.length !== 32) {
      throw new Error('Invalid private key. Must be 64 hex characters (32 bytes).');
    }
    
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    const publicKeyHex = bytesToHex(keyPair.publicKey);
    const address = 'L1' + publicKeyHex.substring(0, 14).toUpperCase();
    
    console.log(`🔑 Importing wallet: ${address}`);
    
    return UnifiedWallet.connect({
      name: 'Imported Wallet',
      address: address,
      public_key: publicKeyHex,
      private_key: cleanKey
    });
  }

  // ==========================================================================
  // FULL WALLET - Register/Login with Password Fork + BIP-39
  // ==========================================================================

  /**
   * Register a new user with Supabase + create encrypted wallet
   * @param {string} username - Unique username
   * @param {string} email - User's email
   * @param {string} password - User's plaintext password
   * @param {number} [wordCount=12] - Mnemonic word count (12 or 24)
   * @returns {Promise<{wallet: UnifiedWallet, mnemonic: string}>}
   * 
   * @example
   *   const { wallet, mnemonic } = await UnifiedWallet.register('alice', 'alice@example.com', 'MyPassword123!');
   *   // ⚠️ User MUST backup mnemonic!
   */
  static async register(username, email, password, wordCount = 12) {
    console.log(`📝 Registering new user: ${username}`);
    
    // Validate inputs
    if (!username || username.length < 3) {
      throw new Error('Username must be at least 3 characters');
    }
    if (!email || !email.includes('@')) {
      throw new Error('Invalid email address');
    }
    if (!password || password.length < 8) {
      throw new Error('Password must be at least 8 characters');
    }
    
    // Step 1: Fork password
    const { loginPassword, walletKey, salt } = forkPassword(password);
    console.log('🔐 Password forked into login + wallet keys');
    
    // Step 2: Generate BIP-39 mnemonic
    const mnemonic = generateMnemonic(wordCount);
    console.log(`📜 Generated ${wordCount}-word mnemonic`);
    
    // Step 3: Derive keypair from mnemonic
    const keypair = deriveKeypairFromMnemonic(mnemonic);
    const address = generateL1Address(keypair.publicKeyHex);
    console.log(`🔑 Derived address: ${address}`);
    
    // Step 4: Encrypt mnemonic with wallet key
    const vaultContents = {
      mnemonic: mnemonic,
      created_at: Date.now(),
      version: 2,
      public_key: keypair.publicKeyHex
    };
    const { encrypted_blob, nonce } = encryptVault(vaultContents, walletKey);
    const encryptedBlobJson = JSON.stringify({
      cipher: encrypted_blob,
      nonce,
      version: 2,
      created_at: Date.now()
    });
    
    // Create wallet instance
    const wallet = new UnifiedWallet({
      name: username,
      address: address,
      public_key: keypair.publicKeyHex,
      private_key: bytesToHex(keypair.privateKey.slice(0, 32)),
      email: email,
      username: username
    });
    
    // Store internal state
    wallet._mnemonic = mnemonic;
    wallet._salt = salt;
    wallet._walletKey = walletKey;
    wallet._isUnlocked = true;
    
    // Step 5: Register with Supabase (if available)
    if (wallet.supabase) {
      try {
        // Create auth user
        const { error: authError } = await wallet.supabase.auth.signUp({
          email,
          password: loginPassword,
          options: { data: { username } }
        });
        if (authError) throw new Error(`Auth failed: ${authError.message}`);
        
        // Create profile with salt + encrypted vault
        const { error: profileError } = await wallet.supabase
          .from('profiles')
          .insert({
            user_id: username,
            email: email,
            salt: salt,
            encrypted_blob: encryptedBlobJson,
            blackbook_address: address,
            reputation_score: 100.0
          });
        if (profileError) throw new Error(`Profile failed: ${profileError.message}`);
        
        console.log('✅ Registered with Supabase');
      } catch (err) {
        console.warn('⚠️ Supabase registration failed:', err.message);
        // Continue without Supabase - wallet still works locally
      }
    }
    
    // Try to register on L1
    try {
      await fetch(`${CONFIG.L1_URL}/auth/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: address,
          public_key: keypair.publicKeyHex
        })
      });
    } catch {
      // L1 may not be running
    }
    
    await wallet.refresh();
    
    console.log('');
    console.log('⚠️  BACKUP YOUR MNEMONIC - YOU CANNOT RECOVER IT!');
    console.log('═══════════════════════════════════════════════════════════');
    console.log(`   ${mnemonic}`);
    console.log('═══════════════════════════════════════════════════════════');
    console.log('');
    
    return { wallet, mnemonic };
  }

  /**
   * Login existing user with password
   * @param {string} usernameOrEmail - Username or email
   * @param {string} password - User's plaintext password
   * @param {Object} [supabaseClient] - Optional Supabase client
   * @returns {Promise<UnifiedWallet>}
   * 
   * @example
   *   const wallet = await UnifiedWallet.login('alice', 'MyPassword123!');
   */
  static async login(usernameOrEmail, password, supabaseClient = null) {
    console.log(`🔓 Logging in: ${usernameOrEmail}`);
    
    // Get Supabase client
    const supabase = supabaseClient || (CONFIG.SUPABASE_URL && CONFIG.SUPABASE_KEY 
      ? createClient(CONFIG.SUPABASE_URL, CONFIG.SUPABASE_KEY) 
      : null);
    
    if (!supabase) {
      throw new Error('Supabase client required for login');
    }
    
    // Step 1: Fetch salt from profiles
    const isEmail = usernameOrEmail.includes('@');
    const query = isEmail
      ? supabase.from('profiles').select('user_id, email, salt, encrypted_blob, blackbook_address').eq('email', usernameOrEmail).single()
      : supabase.from('profiles').select('user_id, email, salt, encrypted_blob, blackbook_address').eq('user_id', usernameOrEmail).single();
    
    const { data: profile, error: profileError } = await query;
    if (profileError || !profile) {
      throw new Error('User not found');
    }
    if (!profile.salt) {
      throw new Error('No salt found - corrupted profile');
    }
    
    console.log('📋 Fetched profile, forking password...');
    
    // Step 2: Fork password with stored salt
    const { loginPassword, walletKey } = forkPassword(password, profile.salt);
    
    // Step 3: Authenticate with Supabase
    const { error: authError } = await supabase.auth.signInWithPassword({
      email: profile.email,
      password: loginPassword
    });
    if (authError) {
      throw new Error('Invalid password');
    }
    
    console.log('✅ Authenticated with Supabase');
    
    // Step 4: Decrypt vault
    if (!profile.encrypted_blob) {
      throw new Error('No encrypted wallet found');
    }
    
    const parsed = JSON.parse(profile.encrypted_blob);
    const vaultContents = decryptVault(parsed.cipher, parsed.nonce, walletKey);
    console.log('🔓 Vault decrypted');
    
    // Step 5: Derive keypair from mnemonic
    const keypair = deriveKeypairFromMnemonic(vaultContents.mnemonic);
    const address = profile.blackbook_address || generateL1Address(keypair.publicKeyHex);
    
    // Create wallet instance
    const wallet = new UnifiedWallet({
      name: profile.user_id,
      address: address,
      public_key: keypair.publicKeyHex,
      private_key: bytesToHex(keypair.privateKey.slice(0, 32)),
      email: profile.email,
      username: profile.user_id
    });
    
    wallet._mnemonic = vaultContents.mnemonic;
    wallet._salt = profile.salt;
    wallet._walletKey = walletKey;
    wallet._isUnlocked = true;
    wallet._supabase = supabase;
    
    await wallet.refresh();
    
    console.log(`✅ Logged in as ${profile.user_id}`);
    wallet.print();
    
    return wallet;
  }

  /**
   * Import wallet from BIP-39 mnemonic phrase
   * @param {string} mnemonic - 12 or 24 word mnemonic
   * @returns {Promise<UnifiedWallet>}
   * 
   * @example
   *   const wallet = await UnifiedWallet.importMnemonic('abandon abandon abandon...');
   */
  static async importMnemonic(mnemonic) {
    // Validate mnemonic
    const cleanMnemonic = mnemonic.trim().toLowerCase();
    if (!validateMnemonic(cleanMnemonic)) {
      throw new Error('Invalid mnemonic phrase');
    }
    
    console.log('📜 Importing wallet from mnemonic...');
    
    // Derive keypair
    const keypair = deriveKeypairFromMnemonic(cleanMnemonic);
    const address = generateL1Address(keypair.publicKeyHex);
    
    const wallet = new UnifiedWallet({
      name: 'Recovered Wallet',
      address: address,
      public_key: keypair.publicKeyHex,
      private_key: bytesToHex(keypair.privateKey.slice(0, 32))
    });
    
    wallet._mnemonic = cleanMnemonic;
    wallet._isUnlocked = true;
    
    await wallet.refresh();
    
    console.log(`✅ Wallet recovered: ${address}`);
    
    return wallet;
  }

  /**
   * Lock wallet (clear sensitive data from memory)
   */
  lock() {
    this._mnemonic = null;
    this._walletKey = null;
    this._privateKey = null;
    this._isUnlocked = false;
    console.log('🔒 Wallet locked');
  }

  // ==========================================================================
  // INSTANCE WALLET OPERATIONS (for existing logged-in users)
  // ==========================================================================

  /**
   * Create a new wallet for an existing logged-in user
   * Requires salt and walletKey to be set on the instance
   * @param {number} [wordCount=12] - Mnemonic word count (12 or 24)
   * @returns {Promise<{mnemonic: string, l1Address: string, publicKey: string}>}
   */
  async createWallet(wordCount = 12) {
    if (!this._salt) {
      throw new Error('Salt not set. Set wallet.salt before calling createWallet()');
    }
    if (!this._walletKey) {
      throw new Error('Wallet key not set. Set wallet.walletKey before calling createWallet()');
    }

    console.log(`📜 Generating ${wordCount}-word mnemonic...`);
    
    // Generate BIP-39 mnemonic
    const mnemonic = generateMnemonic(wordCount);
    
    // Derive keypair from mnemonic
    const keypair = deriveKeypairFromMnemonic(mnemonic);
    const address = generateL1Address(keypair.publicKeyHex);
    
    console.log(`🔑 Derived address: ${address}`);
    
    // Store on instance
    this._mnemonic = mnemonic;
    this.address = address;
    this.publicKey = keypair.publicKeyHex;
    this._privateKey = keypair.privateKey;
    this._publicKey = keypair.publicKey;
    this._isUnlocked = true;
    
    return {
      mnemonic: mnemonic,
      l1Address: address,
      publicKey: keypair.publicKeyHex
    };
  }

  /**
   * Encrypt and store the wallet vault in Supabase
   * Requires mnemonic and walletKey to be set
   * @returns {Promise<{success: boolean, l1Address: string, encryptedBlob: string}>}
   */
  async storeEncryptedVault() {
    if (!this._mnemonic) {
      throw new Error('No mnemonic set. Call createWallet() first.');
    }
    if (!this._walletKey) {
      throw new Error('Wallet key not set.');
    }
    if (!this.supabase) {
      throw new Error('Supabase client not set.');
    }

    console.log('🔐 Encrypting vault...');
    
    // Create vault contents
    const vaultContents = {
      mnemonic: this._mnemonic,
      created_at: Date.now(),
      version: 2,
      public_key: this.publicKey
    };
    
    // Encrypt with wallet key
    const { encrypted_blob, nonce } = encryptVault(vaultContents, this._walletKey);
    const encryptedBlobJson = JSON.stringify({
      cipher: encrypted_blob,
      nonce,
      version: 2,
      created_at: Date.now()
    });
    
    console.log('💾 Storing encrypted vault...');
    
    // Update profile with encrypted blob and address
    const { error } = await this.supabase
      .from('profiles')
      .update({
        encrypted_blob: encryptedBlobJson,
        blackbook_address: this.address
      })
      .eq('user_id', this.userId || this.username);
    
    if (error) {
      console.error('Failed to store vault:', error);
      throw new Error(`Failed to store encrypted vault: ${error.message}`);
    }
    
    console.log('✅ Vault stored successfully');
    
    return {
      success: true,
      l1Address: this.address,
      encryptedBlob: encryptedBlobJson
    };
  }

  /**
   * Unlock wallet by decrypting the vault from Supabase
   * Requires salt and walletKey to be set
   * @returns {Promise<{blackbookAddress: string, l1Address: string}>}
   */
  async unlockWallet() {
    if (!this._walletKey) {
      throw new Error('Wallet key not set. Set wallet.walletKey before calling unlockWallet()');
    }
    if (!this.supabase) {
      throw new Error('Supabase client not set.');
    }

    console.log('🔓 Unlocking wallet...');
    
    // Fetch encrypted blob from profile
    const { data: profile, error } = await this.supabase
      .from('profiles')
      .select('encrypted_blob, blackbook_address, user_id')
      .eq('user_id', this.userId || this.username)
      .single();
    
    if (error || !profile) {
      throw new Error('Could not find profile');
    }
    
    if (!profile.encrypted_blob) {
      throw new Error('No encrypted vault found. Please create a wallet first.');
    }
    
    // Parse and decrypt
    const parsed = JSON.parse(profile.encrypted_blob);
    const vaultContents = decryptVault(parsed.cipher, parsed.nonce, this._walletKey);
    
    console.log('🔓 Vault decrypted');
    
    // Derive keypair from mnemonic
    const keypair = deriveKeypairFromMnemonic(vaultContents.mnemonic);
    const address = profile.blackbook_address || generateL1Address(keypair.publicKeyHex);
    
    // Set instance properties
    this._mnemonic = vaultContents.mnemonic;
    this.address = address;
    this.publicKey = keypair.publicKeyHex;
    this._privateKey = keypair.privateKey;
    this._publicKey = keypair.publicKey;
    this._isUnlocked = true;
    this.name = profile.user_id;
    
    console.log(`✅ Wallet unlocked: ${address}`);
    
    // Refresh balance
    try {
      await this.refresh();
    } catch (e) {
      // Balance refresh may fail if server is down
      console.warn('Could not refresh balance:', e.message);
    }
    
    return {
      blackbookAddress: address,
      l1Address: address
    };
  }

  /**
   * Get balance from L1 (convenience method)
   * @returns {Promise<{balance: number}>}
   */
  async getBalance() {
    await this.refresh();
    return { balance: this.balance };
  }

  /** Check if wallet is unlocked */
  get isUnlocked() {
    return this._isUnlocked && this._privateKey !== null;
  }

  /** Get mnemonic (only if unlocked) */
  get mnemonic() {
    return this._isUnlocked ? this._mnemonic : null;
  }

  /** Get private key bytes (only if unlocked) */
  get privateKey() {
    return this._isUnlocked ? this._privateKey : null;
  }

  // ==========================================================================
  // BALANCE GETTERS (Unified View)
  // ==========================================================================

  /** Total balance (what user sees in UI) = available + locked */
  get balance() {
    return this._l1Available + this._l1Locked;
  }

  /** Alias for balance */
  get totalBalance() {
    return this.balance;
  }

  /** L1 available (can transfer or bridge) */
  get available() {
    return this._l1Available;
  }

  /** Alias */
  get l1Available() {
    return this._l1Available;
  }

  /** L1 locked (bridged to L2) */
  get locked() {
    return this._l1Locked;
  }

  /** L2 balance (for betting) */
  get l2Balance() {
    return this._l2Balance;
  }

  /** Check if user has active L2 session */
  get hasL2Session() {
    return this._l2Balance > 0 || this._l1Locked > 0;
  }

  // ==========================================================================
  // SIGNING
  // ==========================================================================

  /**
   * Sign a message with Ed25519
   * @param {string} message
   * @returns {string} 128-char hex signature
   */
  sign(message) {
    const messageBytes = new TextEncoder().encode(message);
    const signatureBytes = nacl.sign.detached(messageBytes, this._privateKey);
    return bytesToHex(signatureBytes);
  }

  /**
   * Create a signed request for L1 API
   * @param {Object} payload
   * @returns {Object} SignedRequest
   */
  createSignedRequest(payload) {
    const timestamp = Math.floor(Date.now() / 1000);
    const nonce = generateNonce();
    const payloadJson = JSON.stringify(payload);
    const signedContent = `${payloadJson}\n${timestamp}\n${nonce}`;
    
    return {
      public_key: this.publicKey,
      wallet_address: this.address,
      payload: payloadJson,
      timestamp,
      nonce,
      signature: this.sign(signedContent)
    };
  }

  // ==========================================================================
  // BALANCE OPERATIONS
  // ==========================================================================

  /**
   * Refresh balance from servers
   */
  async refresh() {
    try {
      // Get L1 balance
      const l1Response = await fetch(`${CONFIG.L1_URL}/balance/${this.address}`);
      const l1Data = await l1Response.json();
      this._l1Available = l1Data.balance || 0;
      
      // Get L2 balance (may fail if L2 not running)
      try {
        const l2Response = await fetch(`${CONFIG.L2_URL}/balance/${this.address}`);
        if (l2Response.ok) {
          const l2Data = await l2Response.json();
          this._l2Balance = l2Data.balance || 0;
        }
      } catch {
        this._l2Balance = 0;
      }
      
      return this.getBalanceBreakdown();
    } catch (error) {
      console.error('Failed to refresh balance:', error.message);
      throw error;
    }
  }

  /** Alias for refresh */
  async refreshBalance() {
    return this.refresh();
  }

  /**
   * Get balance breakdown (formatted for UI)
   */
  getBalanceBreakdown() {
    return {
      total: this.balance,
      available: this._l1Available,
      locked: this._l1Locked,
      l2_gaming: this._l2Balance,
      canBridge: this._l1Available > 0,
      canWithdraw: this._l2Balance > 0
    };
  }

  // ==========================================================================
  // L1 OPERATIONS
  // ==========================================================================

  /**
   * Transfer tokens on L1
   * @param {string} to - Recipient address
   * @param {number} amount - Amount to transfer
   */
  async transfer(to, amount) {
    if (amount > this._l1Available) {
      throw new Error(`Insufficient balance. Available: ${this._l1Available} BB, Need: ${amount} BB`);
    }
    
    const signedRequest = this.createSignedRequest({ to, amount });
    
    const response = await fetch(`${CONFIG.L1_URL}/transfer`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });
    
    const result = await response.json();
    
    if (result.success) {
      await this.refresh();
      console.log(`✅ Transferred ${amount} BB to ${to}`);
    }
    
    return result;
  }

  // ==========================================================================
  // BRIDGE OPERATIONS (L1 ↔ L2)
  // ==========================================================================

  /**
   * Bridge funds from L1 to L2
   * Locks funds on L1, credits L2 for betting
   * @param {number} amount - Amount to bridge
   */
  async bridgeToL2(amount) {
    if (amount > this._l1Available) {
      throw new Error(`Insufficient balance. Available: ${this._l1Available} BB, Need: ${amount} BB`);
    }
    
    console.log(`🌉 Bridging ${amount} BB to L2...`);
    
    const signedRequest = this.createSignedRequest({
      target_address: this.address,
      amount: amount,
      target_layer: 'L2'
    });
    
    const response = await fetch(`${CONFIG.L1_URL}/bridge/initiate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });
    
    const result = await response.json();
    
    if (result.success) {
      // Update local state
      this._l1Available -= amount;
      this._l1Locked += amount;
      this._l2Balance += amount;
      
      // Also credit L2 directly
      await this._creditL2(amount);
      
      console.log(`✅ Bridge complete!`);
      console.log(`   L1 Available: ${this._l1Available} BB`);
      console.log(`   L1 Locked:    ${this._l1Locked} BB`);
      console.log(`   L2 Balance:   ${this._l2Balance} BB`);
    } else {
      console.error(`❌ Bridge failed:`, result.error);
    }
    
    return result;
  }

  /**
   * Withdraw funds from L2 back to L1
   * Settles L2 balance (with profit/loss) and unlocks on L1
   * @param {number} [amount] - Amount to withdraw (default: all)
   */
  async withdraw(amount = null) {
    const withdrawAmount = amount || this._l2Balance;
    
    if (withdrawAmount <= 0) {
      throw new Error('Nothing to withdraw. L2 balance is 0.');
    }
    
    if (withdrawAmount > this._l2Balance) {
      throw new Error(`Insufficient L2 balance. Available: ${this._l2Balance} BB`);
    }
    
    console.log(`🏦 Withdrawing ${withdrawAmount} BB from L2...`);
    
    // Calculate profit/loss
    const profitLoss = withdrawAmount - this._l1Locked;
    
    const response = await fetch(`${CONFIG.L1_URL}/bridge/withdraw`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        user_address: this.address,
        amount: withdrawAmount,
        l2_burn_tx: `burn_${Date.now()}`,
        l2_signature: 'dev_mode'
      })
    });
    
    const result = await response.json();
    
    if (result.success) {
      // Update local state
      this._l2Balance -= withdrawAmount;
      this._l1Locked = 0;
      
      await this.refresh();
      
      console.log(`✅ Withdrawal complete!`);
      console.log(`   Profit/Loss: ${profitLoss >= 0 ? '+' : ''}${profitLoss} BB`);
      console.log(`   L1 Balance:  ${this.balance} BB`);
    } else {
      console.error(`❌ Withdrawal failed:`, result.error);
    }
    
    return result;
  }

  /** Alias for withdraw */
  async withdrawToL1(amount = null) {
    return this.withdraw(amount);
  }

  /**
   * Credit L2 balance (called after successful bridge)
   * @private
   * @deprecated L2 credit is now handled by the bridge endpoint itself
   */
  async _creditL2(amount) {
    // No-op: L2 credit is handled by the bridge endpoint on the backend
    // This function is kept for backwards compatibility
    return;
  }

  // ==========================================================================
  // L2 BETTING OPERATIONS
  // ==========================================================================

  /**
   * Get all available markets
   * @returns {Promise<Array>} - Array of market objects
   */
  async getMarkets() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/markets`);
      const data = await response.json();
      return data.markets || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Get a specific market by ID
   * @param {string} marketId - Market identifier
   * @returns {Promise<Object|null>} - Market data or null
   */
  async getMarket(marketId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/market/${marketId}`);
      if (!response.ok) return null;
      return await response.json();
    } catch {
      return null;
    }
  }

  /**
   * Get the nonce for a wallet (for replay protection)
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<number>} - Current nonce
   */
  async getNonce(address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L2_URL}/nonce/${addr}`);
      const data = await response.json();
      return data.nonce || 0;
    } catch {
      return 0;
    }
  }

  /**
   * Place a signed bet on L2 (Ed25519 authenticated)
   * @param {string} marketId - Market to bet on
   * @param {string} outcome - 'yes' or 'no' (or market-specific option)
   * @param {number} amount - Amount to bet in BB
   * @returns {Promise<Object>} - Bet result with bet_id
   */
  async placeBet(marketId, outcome, amount) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to place bets');
    }

    if (amount > this._l2Balance) {
      throw new Error(`Insufficient L2 balance. Have: ${this._l2Balance} BB, Need: ${amount} BB`);
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId,
      outcome: outcome,
      amount: amount
    });

    const response = await fetch(`${CONFIG.L2_URL}/bet`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    
    if (result.success || result.bet_id) {
      await this.refresh();
      console.log(`✅ Bet placed: ${amount} BB on ${outcome} for ${marketId}`);
    }
    
    return result;
  }

  /**
   * Get a specific bet by ID
   * @param {string} betId - Bet identifier
   * @returns {Promise<Object|null>} - Bet data
   */
  async getBetById(betId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/bet/${betId}`);
      if (!response.ok) return null;
      return await response.json();
    } catch {
      return null;
    }
  }

  /**
   * Get user's bet history
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Array>} - Array of bets
   */
  async getBets(address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L2_URL}/bets/${addr}`);
      const data = await response.json();
      return data.bets || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Get all bets for a specific market
   * @param {string} marketId - Market identifier
   * @returns {Promise<Array>} - Array of bets on the market
   */
  async getMarketBets(marketId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/market/${marketId}/bets`);
      const data = await response.json();
      return data.bets || data || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // L2 SHARES SYSTEM (Outcome Token Trading)
  // ==========================================================================

  /**
   * Mint outcome shares (convert BB to YES/NO shares)
   * @param {string} marketId - Market identifier
   * @param {number} amount - Amount of BB to convert to shares
   * @returns {Promise<Object>} - Minting result with shares received
   */
  async mintShares(marketId, amount) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to mint shares');
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId,
      amount: amount
    });

    const response = await fetch(`${CONFIG.L2_URL}/shares/mint`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
    }
    return result;
  }

  /**
   * Redeem shares (convert YES/NO shares back to BB after resolution)
   * @param {string} marketId - Market identifier
   * @param {number} shares - Number of shares to redeem
   * @returns {Promise<Object>} - Redemption result with BB received
   */
  async redeemShares(marketId, shares) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to redeem shares');
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId,
      shares: shares
    });

    const response = await fetch(`${CONFIG.L2_URL}/shares/redeem`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
    }
    return result;
  }

  /**
   * Get share position for a specific market
   * @param {string} marketId - Market identifier
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Object>} - Position with yes_shares, no_shares
   */
  async getSharePosition(marketId, address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L2_URL}/shares/position/${marketId}/${addr}`);
      return await response.json();
    } catch {
      return { yes_shares: 0, no_shares: 0 };
    }
  }

  /**
   * Get all share positions for a wallet
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Array>} - Array of positions across all markets
   */
  async getAllSharePositions(address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L2_URL}/shares/positions/${addr}`);
      const data = await response.json();
      return data.positions || data || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // L2 ORDERBOOK / CLOB (Central Limit Order Book)
  // ==========================================================================

  /**
   * Place a limit order on the orderbook
   * @param {string} marketId - Market identifier
   * @param {string} side - 'buy' or 'sell'
   * @param {string} outcome - 'yes' or 'no'
   * @param {number} price - Price per share (0.01 to 0.99)
   * @param {number} amount - Number of shares
   * @returns {Promise<Object>} - Order result with order_id
   */
  async placeLimitOrder(marketId, side, outcome, price, amount) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to place orders');
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId,
      side: side,
      outcome: outcome,
      price: price,
      amount: amount
    });

    const response = await fetch(`${CONFIG.L2_URL}/orderbook/place`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
    }
    return result;
  }

  /**
   * Cancel an open order
   * @param {string} orderId - Order identifier
   * @returns {Promise<Object>} - Cancellation result
   */
  async cancelOrder(orderId) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to cancel orders');
    }

    const signedRequest = this.createSignedRequest({
      order_id: orderId
    });

    const response = await fetch(`${CONFIG.L2_URL}/orderbook/cancel`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  /**
   * Get the orderbook for a market
   * @param {string} marketId - Market identifier
   * @returns {Promise<Object>} - Orderbook with bids and asks
   */
  async getOrderbook(marketId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/orderbook/${marketId}`);
      return await response.json();
    } catch {
      return { bids: [], asks: [] };
    }
  }

  /**
   * Get user's open orders
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Array>} - Array of open orders
   */
  async getOpenOrders(address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L2_URL}/orderbook/orders/${addr}`);
      const data = await response.json();
      return data.orders || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Get current market odds (from orderbook mid-price)
   * @param {string} marketId - Market identifier
   * @returns {Promise<Object>} - Market odds with yes_price, no_price
   */
  async getMarketOdds(marketId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/market/${marketId}/odds`);
      return await response.json();
    } catch {
      return { yes_price: 0.5, no_price: 0.5 };
    }
  }

  // ==========================================================================
  // MARKET RESOLUTION
  // ==========================================================================

  /**
   * Resolve a market (Oracle/Admin only)
   * @param {string} marketId - Market identifier
   * @param {string} winningOutcome - 'yes' or 'no'
   * @param {string} [reason] - Resolution reason/evidence
   * @returns {Promise<Object>} - Resolution result
   */
  async resolveMarket(marketId, winningOutcome, reason = '') {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to resolve markets');
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId,
      winning_outcome: winningOutcome,
      reason: reason
    });

    const response = await fetch(`${CONFIG.L2_URL}/market/resolve`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  /**
   * Get market resolution details
   * @param {string} marketId - Market identifier
   * @returns {Promise<Object|null>} - Resolution details or null if not resolved
   */
  async getMarketResolution(marketId) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/market/${marketId}/resolution`);
      if (!response.ok) return null;
      return await response.json();
    } catch {
      return null;
    }
  }

  /**
   * Claim winnings from a resolved market
   * @param {string} marketId - Market identifier
   * @returns {Promise<Object>} - Claim result with amount won
   */
  async claimWinnings(marketId) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to claim winnings');
    }

    const signedRequest = this.createSignedRequest({
      market_id: marketId
    });

    const response = await fetch(`${CONFIG.L2_URL}/market/claim`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
      console.log(`✅ Claimed ${result.amount || 0} BB from market ${marketId}`);
    }
    return result;
  }

  /**
   * Get markets pending resolution
   * @returns {Promise<Array>} - Array of markets awaiting resolution
   */
  async getPendingResolutions() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/markets/pending`);
      const data = await response.json();
      return data.markets || data || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // ORACLE MANAGEMENT
  // ==========================================================================

  /**
   * List authorized oracles
   * @returns {Promise<Array>} - Array of oracle addresses/keys
   */
  async listOracles() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/oracles`);
      const data = await response.json();
      return data.oracles || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Add an authorized oracle (Admin only)
   * @param {string} oracleAddress - Address to authorize as oracle
   * @returns {Promise<Object>} - Result
   */
  async addOracle(oracleAddress) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to add oracles');
    }

    const signedRequest = this.createSignedRequest({
      oracle_address: oracleAddress
    });

    const response = await fetch(`${CONFIG.L2_URL}/oracle/add`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  /**
   * Remove an authorized oracle (Admin only)
   * @param {string} oracleAddress - Oracle address to remove
   * @returns {Promise<Object>} - Result
   */
  async removeOracle(oracleAddress) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to remove oracles');
    }

    const signedRequest = this.createSignedRequest({
      oracle_address: oracleAddress
    });

    const response = await fetch(`${CONFIG.L2_URL}/oracle/remove`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  // ==========================================================================
  // L1 SETTLEMENT
  // ==========================================================================

  /**
   * Submit market settlements to L1
   * @param {Array} settlements - Array of settlement objects
   * @returns {Promise<Object>} - Settlement result
   */
  async submitSettlements(settlements) {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to submit settlements');
    }

    const signedRequest = this.createSignedRequest({
      settlements: settlements
    });

    const response = await fetch(`${CONFIG.L2_URL}/settlement/submit`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  /**
   * Get markets pending L1 settlement
   * @returns {Promise<Array>} - Markets ready for L1 settlement
   */
  async getPendingSettlements() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/settlement/pending`);
      const data = await response.json();
      return data.markets || data || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // L2 LEDGER & ACTIVITY
  // ==========================================================================

  /**
   * Get L2 ledger state (JSON)
   * @returns {Promise<Object>} - Full ledger state
   */
  async getLedger() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/ledger`);
      return await response.json();
    } catch {
      return {};
    }
  }

  /**
   * Get L2 ledger statistics
   * @returns {Promise<Object>} - Ledger stats
   */
  async getLedgerStats() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/ledger/stats`);
      return await response.json();
    } catch {
      return {};
    }
  }

  /**
   * Get activity feed (recent bets, trades, resolutions)
   * @param {number} [limit=50] - Maximum entries
   * @returns {Promise<Array>} - Activity feed
   */
  async getActivityFeed(limit = 50) {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/activity?limit=${limit}`);
      const data = await response.json();
      return data.activity || data || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // TRANSACTION HISTORY
  // ==========================================================================

  /**
   * Get transaction history from L1
   * @param {number} [limit=50] - Maximum transactions to return
   * @returns {Promise<Array>} Transaction list
   */
  async getTransactions(limit = 50) {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/transactions/${this.address}?limit=${limit}`);
      if (!response.ok) {
        return [];
      }
      const data = await response.json();
      return data.transactions || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Get transaction details by ID
   * @param {string} txId - Transaction ID
   * @returns {Promise<Object|null>}
   */
  async getTransaction(txId) {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/transaction/${txId}`);
      if (!response.ok) return null;
      return await response.json();
    } catch {
      return null;
    }
  }

  // ==========================================================================
  // BRIDGE STATUS & SETTLEMENTS
  // ==========================================================================

  /**
   * Get bridge transaction status
   * @param {string} bridgeId - Bridge transaction ID
   * @returns {Promise<Object>}
   */
  async getBridgeStatus(bridgeId) {
    const response = await fetch(`${CONFIG.L1_URL}/bridge/status/${bridgeId}`);
    return await response.json();
  }

  /**
   * Get bridge statistics
   * @returns {Promise<Object>} Bridge stats
   */
  async getBridgeStats() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/bridge/stats`);
      return await response.json();
    } catch {
      return { total_bridged: 0, total_withdrawn: 0, active_sessions: 0 };
    }
  }

  /**
   * Get pending bridge transactions
   * @returns {Promise<Object>} - Pending bridge operations
   */
  async getBridgePending() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/bridge/pending`);
      return await response.json();
    } catch {
      return { pending_count: 0, pending: [] };
    }
  }

  /**
   * Get bridge history for this wallet
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Array>} - Array of bridge transactions
   */
  async getBridgeHistory(address = null) {
    try {
      const addr = address || this.address;
      const response = await fetch(`${CONFIG.L1_URL}/bridge/history/${addr}`);
      const data = await response.json();
      return data.history || data || [];
    } catch {
      return [];
    }
  }

  /**
   * Initiate a bridge lock (L1 → L2 escrow)
   * Used for P2P escrow or cross-layer transfers with settlement
   * @param {string} targetAddress - Beneficiary address
   * @param {number} amount - Amount to lock
   * @param {string} [targetLayer='L2'] - Target layer
   * @returns {Promise<Object>} - Lock result with lock_id, bridge_id
   */
  async bridgeInitiate(targetAddress, amount, targetLayer = 'L2') {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to initiate bridge');
    }

    const signedRequest = this.createSignedRequest({
      target_address: targetAddress,
      amount: amount,
      target_layer: targetLayer
    });

    const response = await fetch(`${CONFIG.L1_URL}/bridge/initiate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
      console.log(`🔒 Bridge initiated: ${amount} BB locked (ID: ${result.lock_id || result.bridge_id})`);
    }
    return result;
  }

  /**
   * Verify a settlement (L2 → L1 bridge release authorization)
   * @param {Object} settlementProof - Settlement proof from L2
   * @returns {Promise<Object>} - Verification result
   */
  async bridgeVerifySettlement(settlementProof) {
    const response = await fetch(`${CONFIG.L1_URL}/bridge/verify-settlement`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settlementProof)
    });
    return await response.json();
  }

  /**
   * Release escrowed funds (after settlement verification)
   * @param {string} lockId - Lock ID to release
   * @returns {Promise<Object>} - Release result
   */
  async bridgeRelease(lockId) {
    const response = await fetch(`${CONFIG.L1_URL}/bridge/release`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ lock_id: lockId })
    });

    const result = await response.json();
    if (result.success) {
      await this.refresh();
      console.log(`🔓 Bridge released: ${result.amount || 0} BB to ${result.recipient || 'beneficiary'}`);
    }
    return result;
  }

  /**
   * Claim a Merkle settlement from L2
   * @param {string} rootHash - Merkle root hash
   * @param {Array<string>} proof - Merkle proof
   * @param {number} amount - Amount to claim
   * @returns {Promise<Object>}
   */
  async claimSettlement(rootHash, proof, amount) {
    const signedRequest = this.createSignedRequest({
      root_hash: rootHash,
      proof: proof,
      amount: amount,
      claimer: this.address
    });

    const response = await fetch(`${CONFIG.L1_URL}/bridge/claim`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    
    if (result.success) {
      await this.refresh();
      console.log(`✅ Claimed settlement: ${amount} BB`);
    }
    
    return result;
  }

  // ==========================================================================
  // WALLET & SYSTEM INFO
  // ==========================================================================

  /**
   * Get authenticated wallet info from server
   * @returns {Promise<Object>}
   */
  async getWalletInfo() {
    const signedRequest = this.createSignedRequest({ action: 'get_info' });
    
    const response = await fetch(`${CONFIG.L1_URL}/wallet/info`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });
    
    return await response.json();
  }

  /**
   * Get blockchain statistics
   * @returns {Promise<Object>}
   */
  async getStats() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/stats`);
      return await response.json();
    } catch {
      return { blocks: 0, transactions: 0, accounts: 0 };
    }
  }

  /**
   * Health check - verify L1 connection
   * @returns {Promise<boolean>}
   */
  async health() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Health check for L2
   * @returns {Promise<boolean>}
   */
  async healthL2() {
    try {
      const response = await fetch(`${CONFIG.L2_URL}/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  // ==========================================================================
  // SESSION MANAGEMENT (Alternative to Bridge)
  // ==========================================================================

  /**
   * Start an L2 session (mirrors L1 balance for instant L2 play)
   * Unlike bridge, this doesn't lock funds - it creates a "session"
   * @param {number} [depositAmount=0] - Optional initial deposit
   * @returns {Promise<Object>}
   */
  async startSession(depositAmount = 0) {
    const signedRequest = this.createSignedRequest({
      deposit: depositAmount,
      session_type: 'gaming'
    });

    const response = await fetch(`${CONFIG.L1_URL}/session/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    
    if (result.success || result.session_id) {
      console.log(`🎮 Session started: ${result.session_id || 'active'}`);
      await this.refresh();
    }
    
    return result;
  }

  /**
   * Get current session status
   * @param {string} [address] - Address to check (default: this wallet)
   * @returns {Promise<Object>}
   */
  async getSessionStatus(address = null) {
    const addr = address || this.address;
    try {
      const response = await fetch(`${CONFIG.L1_URL}/session/status/${addr}`);
      return await response.json();
    } catch {
      return { active: false, balance: 0 };
    }
  }

  /**
   * Settle L2 session - write profit/loss back to L1
   * @returns {Promise<Object>}
   */
  async settleSession() {
    const signedRequest = this.createSignedRequest({
      action: 'settle',
      final_balance: this._l2Balance
    });

    const response = await fetch(`${CONFIG.L1_URL}/session/settle`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    const result = await response.json();
    
    if (result.success) {
      console.log(`📊 Session settled. P/L: ${result.profit_loss || 0} BB`);
      await this.refresh();
    }
    
    return result;
  }

  /**
   * List all active sessions (admin/debugging)
   * @returns {Promise<Array>}
   */
  async listSessions() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/session/list`);
      const data = await response.json();
      return data.sessions || [];
    } catch {
      return [];
    }
  }

  // ==========================================================================
  // ADVANCED: POH & RPC
  // ==========================================================================

  /**
   * Get Proof of History status
   * @returns {Promise<Object>}
   */
  async getPoHStatus() {
    try {
      const response = await fetch(`${CONFIG.L1_URL}/poh/status`);
      return await response.json();
    } catch {
      return { tick: 0, hash: null, running: false };
    }
  }

  /**
   * Make raw RPC call to L1
   * @param {string} method - RPC method name
   * @param {Object} [params={}] - RPC parameters
   * @returns {Promise<Object>}
   */
  async rpc(method, params = {}) {
    const response = await fetch(`${CONFIG.L1_URL}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: Date.now(),
        method: method,
        params: params
      })
    });

    const result = await response.json();
    
    if (result.error) {
      throw new Error(result.error.message || JSON.stringify(result.error));
    }
    
    return result.result;
  }

  // ==========================================================================
  // L1 RPC WRAPPER METHODS (Solana-Compatible)
  // ==========================================================================

  /**
   * Get recent blockhash (Solana-compatible)
   * @returns {Promise<{blockhash: string, lastValidBlockHeight: number}>}
   */
  async getRecentBlockhash() {
    return await this.rpc('getRecentBlockhash');
  }

  /**
   * Get latest blockhash (Solana-compatible)
   * @returns {Promise<{blockhash: string, lastValidBlockHeight: number}>}
   */
  async getLatestBlockhash() {
    return await this.rpc('getLatestBlockhash');
  }

  /**
   * Check if a blockhash is still valid
   * @param {string} blockhash - The blockhash to validate
   * @returns {Promise<{valid: boolean}>}
   */
  async isBlockhashValid(blockhash) {
    return await this.rpc('isBlockhashValid', [blockhash]);
  }

  /**
   * Get current block height
   * @returns {Promise<number>}
   */
  async getBlockHeight() {
    return await this.rpc('getBlockHeight');
  }

  /**
   * Get current slot (same as block height for BlackBook)
   * @returns {Promise<number>}
   */
  async getSlot() {
    return await this.rpc('getSlot');
  }

  /**
   * Get slot leader for a given slot
   * @param {number} [slot] - Optional slot number (default: current)
   * @returns {Promise<string>} - Leader public key or validator ID
   */
  async getSlotLeader(slot = null) {
    return await this.rpc('getSlotLeader', slot ? [slot] : []);
  }

  /**
   * Get fee estimate for a message/transaction
   * @returns {Promise<{fee: number, message: string}>}
   */
  async getFeeForMessage() {
    return await this.rpc('getFeeForMessage');
  }

  /**
   * Get minimum balance for rent exemption
   * @param {number} dataSize - Size of data in bytes
   * @returns {Promise<number>} - Minimum balance in BB
   */
  async getMinimumBalanceForRentExemption(dataSize = 0) {
    return await this.rpc('getMinimumBalanceForRentExemption', [dataSize]);
  }

  /**
   * Get chain statistics
   * @returns {Promise<Object>} - Chain stats including block count, tx count, etc.
   */
  async getChainStats() {
    return await this.rpc('getChainStats');
  }

  /**
   * Get account info for an address
   * @param {string} address - L1 address to query
   * @returns {Promise<Object>} - Account information
   */
  async getAccountInfo(address) {
    return await this.rpc('getAccountInfo', [address || this.address]);
  }

  /**
   * Verify an Ed25519 signature via L1 RPC
   * @param {string} publicKey - 64-char hex public key
   * @param {string} message - Original message that was signed
   * @param {string} signature - 128-char hex signature
   * @returns {Promise<{valid: boolean}>}
   */
  async verifySignature(publicKey, message, signature) {
    return await this.rpc('verifyL1Signature', [publicKey, message, signature]);
  }

  /**
   * Get balance via RPC (alternative to REST endpoint)
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<number>}
   */
  async getBalanceRpc(address = null) {
    return await this.rpc('getBalance', [address || this.address]);
  }

  /**
   * Get transactions for an address via RPC
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Array>}
   */
  async getTransactionsRpc(address = null) {
    return await this.rpc('getTransactions', [address || this.address]);
  }

  // ==========================================================================
  // L1 BLOCK OPERATIONS
  // ==========================================================================

  /**
   * Get a specific block by height
   * @param {number} height - Block height (0 = genesis)
   * @returns {Promise<Object>} - Block data
   */
  async getBlock(height) {
    return await this.rpc('getBlock', [height]);
  }

  /**
   * Get the latest block
   * @returns {Promise<Object>} - Latest block data
   */
  async getLatestBlock() {
    return await this.rpc('getLatestBlock');
  }

  /**
   * Get multiple blocks starting from a height
   * @param {number} startHeight - Starting block height
   * @param {number} count - Number of blocks to retrieve
   * @returns {Promise<Array>} - Array of blocks
   */
  async getBlocks(startHeight, count = 10) {
    return await this.rpc('getBlocks', [startHeight, count]);
  }

  /**
   * Get block by hash
   * @param {string} hash - Block hash
   * @returns {Promise<Object>} - Block data
   */
  async getBlockByHash(hash) {
    return await this.rpc('getBlockByHash', [hash]);
  }

  // ==========================================================================
  // SOCIAL MINING
  // ==========================================================================

  /**
   * Create a social post (earns BB tokens in social mining)
   * @param {string} content - Post content
   * @param {string} [mediaType='text'] - Media type (text, image, video)
   * @returns {Promise<Object>} - Post result with earned tokens
   */
  async createSocialPost(content, mediaType = 'text') {
    if (!this._privateKey) {
      throw new Error('Wallet must be unlocked to create social posts');
    }

    const signedRequest = this.createSignedRequest({
      content,
      media_type: mediaType
    });

    const response = await fetch(`${CONFIG.L1_URL}/social/post`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(signedRequest)
    });

    return await response.json();
  }

  /**
   * Get social mining statistics
   * @returns {Promise<Object>} - Social mining stats
   */
  async getSocialStats() {
    const response = await fetch(`${CONFIG.L1_URL}/social/stats`);
    return await response.json();
  }

  /**
   * Get user's social mining history
   * @param {string} [address] - Address to query (default: this wallet)
   * @returns {Promise<Object>} - User's social stats and history
   */
  async getSocialHistory(address = null) {
    const addr = address || this.address;
    const response = await fetch(`${CONFIG.L1_URL}/social/history/${addr}`);
    return await response.json();
  }

  // ==========================================================================
  // ADMIN / DEV MODE OPERATIONS
  // ==========================================================================

  /**
   * Mint tokens (DEV MODE ONLY)
   * @param {string} to - Recipient address
   * @param {number} amount - Amount to mint
   * @returns {Promise<Object>}
   */
  async mint(to, amount) {
    const response = await fetch(`${CONFIG.L1_URL}/admin/mint`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address: to, amount })
    });
    const result = await response.json();
    if (result.success) {
      await this.refresh();
    }
    return result;
  }

  /**
   * Set initial liquidity for a market (DEV MODE ONLY)
   * @param {string} marketId - Market ID
   * @param {number} amount - Liquidity amount
   * @param {boolean} [houseFunded=false] - Whether house-funded
   * @returns {Promise<Object>}
   */
  async setInitialLiquidity(marketId, amount, houseFunded = false) {
    const response = await fetch(`${CONFIG.L1_URL}/markets/initial-liquidity/${marketId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ amount, house_funded: houseFunded })
    });
    return await response.json();
  }

  // ==========================================================================
  // STATIC HELPER METHODS
  // ==========================================================================

  /**
   * Generate a random keypair (without creating full wallet)
   * @returns {{address: string, publicKey: string, privateKey: string}}
   */
  static generateRandomKeypair() {
    if (typeof crypto === 'undefined' || !crypto.getRandomValues) {
      throw new Error(
        'Secure random number generator not available. ' +
        'Use a modern browser with Web Crypto API or Node.js 15+.'
      );
    }
    const seed = new Uint8Array(32);
    crypto.getRandomValues(seed);
    
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    const publicKeyHex = bytesToHex(keyPair.publicKey);
    const address = generateL1Address(publicKeyHex);
    
    return {
      address: address,
      publicKey: publicKeyHex,
      privateKey: bytesToHex(seed)
    };
  }

  /**
   * Derive public key from private key
   * @param {string} privateKeyHex - 64-char hex private key (32-byte seed)
   * @returns {string} 64-char hex public key
   */
  static derivePublicKey(privateKeyHex) {
    const seed = hexToBytes(privateKeyHex);
    const keyPair = nacl.sign.keyPair.fromSeed(seed);
    return bytesToHex(keyPair.publicKey);
  }

  /**
   * Generate L1 address from public key
   * @param {string} publicKeyHex - 64-char hex public key
   * @returns {string} L1 address (L1 + 14 hex chars)
   */
  static generateAddress(publicKeyHex) {
    return generateL1Address(publicKeyHex);
  }

  /**
   * Validate a mnemonic phrase
   * @param {string} mnemonic - 12 or 24 word phrase
   * @returns {boolean}
   */
  static validateMnemonic(mnemonic) {
    return validateMnemonic(mnemonic);
  }

  /**
   * Get the SDK configuration
   * @returns {Object}
   */
  static getConfig() {
    return { ...CONFIG };
  }

  /**
   * Update SDK configuration
   * @param {Object} newConfig - New config values
   */
  static setConfig(newConfig) {
    Object.assign(CONFIG, newConfig);
  }

  // ==========================================================================
  // DISPLAY HELPERS
  // ==========================================================================

  /**
   * Print wallet status to console
   */
  print() {
    console.log(`
╔═══════════════════════════════════════════════════════════╗
║  ${this.name.padEnd(52)} ║
║  ${this.address.padEnd(52)} ║
╠═══════════════════════════════════════════════════════════╣
║  💰 TOTAL BALANCE: ${String(this.balance + ' BB').padEnd(35)} ║
║  ─────────────────────────────────────────────────────── ║
║  L1 Available:  ${String(this._l1Available + ' BB').padEnd(38)} ║
║  L1 Locked:     ${String(this._l1Locked + ' BB').padEnd(38)} ║
║  L2 Gaming:     ${String(this._l2Balance + ' BB').padEnd(38)} ║
╚═══════════════════════════════════════════════════════════╝
`);
  }

  /** Alias for print */
  printStatus() {
    this.print();
  }

  /**
   * Export wallet info as JSON
   */
  toJSON() {
    return {
      name: this.name,
      address: this.address,
      username: this.username,
      email: this.email,
      balance: {
        total: this.balance,
        l1_available: this._l1Available,
        l1_locked: this._l1Locked,
        l2_gaming: this._l2Balance
      }
    };
  }
}

// ============================================================================
// DEMO FUNCTION - Run: bun unified-wallet-sdk.js
// ============================================================================

export async function demo() {
  console.log(`
╔═══════════════════════════════════════════════════════════╗
║         UNIFIED WALLET SDK - DEMO                         ║
╚═══════════════════════════════════════════════════════════╝
`);

  // 1. Connect Alice
  console.log('📱 Step 1: Connecting Alice...\n');
  const alice = await UnifiedWallet.connect('alice');
  alice.print();
  
  // 2. Bridge to L2
  console.log('🌉 Step 2: Bridging 5,000 BB to L2...\n');
  await alice.bridgeToL2(5000);
  alice.print();
  
  // 3. Simulate betting win
  console.log('🎰 Step 3: Simulating bet + win (+1,500 BB)...\n');
  alice._l2Balance += 1500;  // Simulated win
  console.log('   Alice won 1500 BB!');
  alice.print();
  
  // 4. Withdraw
  console.log('🏦 Step 4: Withdrawing to L1...\n');
  await alice.withdraw();
  alice.print();
  
  // 5. Transfer to Bob
  console.log('💸 Step 5: Transferring 1,000 BB to Bob...\n');
  const bob = await UnifiedWallet.connect('bob');
  await alice.transfer(bob.address, 1000);
  
  console.log('Final balances:');
  await alice.refresh();
  await bob.refresh();
  alice.print();
  bob.print();
  
  console.log('✅ Demo complete!\n');
}

// ============================================================================
// EXPORTS
// ============================================================================

// Export accounts for direct use
export { ACCOUNTS };
export const ALICE = ACCOUNTS.alice;
export const BOB = ACCOUNTS.bob;

// Export helper functions for advanced use
export {
  forkPassword,
  generateSalt,
  deriveLoginPassword,
  deriveWalletKey,
  encryptVault,
  decryptVault,
  generateMnemonic,
  validateMnemonic,
  deriveKeypairFromMnemonic,
  generateL1Address,
  hexToBytes,
  bytesToHex
};

// Browser globals
if (typeof window !== 'undefined') {
  window.UnifiedWallet = UnifiedWallet;
  window.ALICE = ALICE;
  window.BOB = BOB;
}

// Auto-run demo if executed directly
const isMainModule = typeof process !== 'undefined' && 
  process.argv && 
  process.argv[1]?.includes('unified-wallet-sdk');
  
if (isMainModule) {
  demo().catch(console.error);
}

export default UnifiedWallet;
