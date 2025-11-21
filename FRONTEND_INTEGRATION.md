# Frontend Integration Guide - BlackBook Blockchain

## 🔌 Connecting Your Frontend to BlackBook RPC

This guide shows how to connect your frontend to the BlackBook blockchain and allow users to access the 8 test accounts.

---

## 🚀 Quick Start

### RPC Endpoint
```
Local:      http://localhost:8080

```

### Test Accounts (God Mode)
Your blockchain has 8 pre-funded test accounts:
- ALICE, BOB, CHARLIE, DIANA, ETHAN, FIONA, GEORGE, HANNAH
- Each starts with **1000 BB tokens** ($10.00 USD)

---

## 📡 API Endpoints for Wallet Connection

### 1. Get All Test Accounts
**Endpoint:** `GET /wallet/test-accounts`

**Response:**
```json
{
  "success": true,
  "network": "BlackBook L1",
  "rpc_url": "http://0.0.0.0:8080",
  "test_accounts": [
    {
      "name": "ALICE",
      "address": "L1_A1B2C3D4E5F6...",
      "balance": 1000.0,
      "balance_usd": 10.0,
      "token": "BB",
      "transaction_count": 5,
      "avatar": "https://api.dicebear.com/7.x/avataaars/svg?seed=alice"
    },
    // ... 7 more accounts
  ]
}
```

### 2. Connect Wallet
**Endpoint:** `GET /wallet/connect/:account_name`

**Example:** `GET /wallet/connect/ALICE`

**Response:**
```json
{
  "success": true,
  "connected": true,
  "account": {
    "name": "ALICE",
    "address": "L1_A1B2C3D4E5F6...",
    "balance": 1000.0,
    "balance_usd": 10.0,
    "token": "BB",
    "network": "BlackBook L1"
  },
  "stats": {
    "transaction_count": 5,
    "markets_participated": 3
  },
  "recent_transactions": [...],
  "active_bets": [...]
}
```

### 3. Get Account Details
**Endpoint:** `GET /wallet/account-info/:account_name`

**Example:** `GET /wallet/account-info/BOB`

**Response:**
```json
{
  "success": true,
  "account": {
    "name": "BOB",
    "address": "L1_B2C3D4E5F6...",
    "balance": 950.5,
    "balance_usd": 9.51,
    "token": "BB",
    "network": "BlackBook L1"
  },
  "statistics": {
    "total_transactions": 12,
    "total_sent": 150.0,
    "total_received": 100.5,
    "bets_placed": 8,
    "net_flow": -49.5
  },
  "recent_transactions": [...]
}
```

---

## 🎨 Frontend Implementation (React/Next.js)

### Step 1: Create Wallet Context

```typescript
// contexts/WalletContext.tsx
import { createContext, useContext, useState, useEffect } from 'react';

interface Account {
  name: string;
  address: string;
  balance: number;
  balance_usd: number;
  token: string;
}

interface WalletContextType {
  account: Account | null;
  isConnected: boolean;
  testAccounts: Account[];
  connect: (accountName: string) => Promise<void>;
  disconnect: () => void;
  refreshBalance: () => Promise<void>;
}

const WalletContext = createContext<WalletContextType | undefined>(undefined);

const RPC_URL = process.env.NEXT_PUBLIC_RPC_URL || 'http://localhost:8080';

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [account, setAccount] = useState<Account | null>(null);
  const [testAccounts, setTestAccounts] = useState<Account[]>([]);

  // Fetch test accounts on mount
  useEffect(() => {
    fetchTestAccounts();
  }, []);

  const fetchTestAccounts = async () => {
    try {
      const response = await fetch(`${RPC_URL}/wallet/test-accounts`);
      const data = await response.json();
      if (data.success) {
        setTestAccounts(data.test_accounts);
      }
    } catch (error) {
      console.error('Failed to fetch test accounts:', error);
    }
  };

  const connect = async (accountName: string) => {
    try {
      const response = await fetch(`${RPC_URL}/wallet/connect/${accountName}`);
      const data = await response.json();
      
      if (data.success && data.connected) {
        setAccount(data.account);
        // Store in localStorage
        localStorage.setItem('connected_account', accountName);
        console.log(`✅ Connected to ${accountName}`);
      } else {
        throw new Error(data.error || 'Failed to connect');
      }
    } catch (error) {
      console.error('Connection failed:', error);
      throw error;
    }
  };

  const disconnect = () => {
    setAccount(null);
    localStorage.removeItem('connected_account');
    console.log('🔌 Disconnected');
  };

  const refreshBalance = async () => {
    if (!account) return;
    
    try {
      const response = await fetch(`${RPC_URL}/wallet/connect/${account.name}`);
      const data = await response.json();
      
      if (data.success) {
        setAccount(data.account);
      }
    } catch (error) {
      console.error('Failed to refresh balance:', error);
    }
  };

  return (
    <WalletContext.Provider
      value={{
        account,
        isConnected: !!account,
        testAccounts,
        connect,
        disconnect,
        refreshBalance,
      }}
    >
      {children}
    </WalletContext.Provider>
  );
}

export const useWallet = () => {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error('useWallet must be used within WalletProvider');
  }
  return context;
};
```

### Step 2: God Mode Wallet Selector Component

```typescript
// components/WalletSelector.tsx
import { useWallet } from '@/contexts/WalletContext';
import { useState } from 'react';

export function WalletSelector() {
  const { testAccounts, connect, disconnect, isConnected, account } = useWallet();
  const [showDropdown, setShowDropdown] = useState(false);

  const handleConnect = async (accountName: string) => {
    try {
      await connect(accountName);
      setShowDropdown(false);
    } catch (error) {
      alert(`Failed to connect: ${error}`);
    }
  };

  if (isConnected && account) {
    return (
      <div className="wallet-connected">
        <div className="account-info">
          <img 
            src={`https://api.dicebear.com/7.x/avataaars/svg?seed=${account.name.toLowerCase()}`}
            alt={account.name}
            className="avatar"
          />
          <div>
            <div className="account-name">{account.name}</div>
            <div className="account-balance">
              {account.balance.toFixed(2)} BB
              <span className="usd">(${account.balance_usd.toFixed(2)})</span>
            </div>
          </div>
        </div>
        <button onClick={disconnect} className="disconnect-btn">
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <div className="wallet-selector">
      <button 
        onClick={() => setShowDropdown(!showDropdown)}
        className="connect-btn"
      >
        🔓 God Mode - Select Account
      </button>

      {showDropdown && (
        <div className="account-dropdown">
          <div className="dropdown-header">
            <h3>🎮 Test Accounts (God Mode)</h3>
            <p>Connect to any pre-funded account</p>
          </div>
          
          <div className="account-grid">
            {testAccounts.map((acc) => (
              <button
                key={acc.name}
                onClick={() => handleConnect(acc.name)}
                className="account-card"
              >
                <img 
                  src={`https://api.dicebear.com/7.x/avataaars/svg?seed=${acc.name.toLowerCase()}`}
                  alt={acc.name}
                  className="avatar"
                />
                <div className="account-details">
                  <div className="name">{acc.name}</div>
                  <div className="balance">
                    {acc.balance.toFixed(2)} BB
                  </div>
                  <div className="address">
                    {acc.address.slice(0, 10)}...
                  </div>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
```

### Step 3: Use in Your App

```typescript
// pages/_app.tsx or app/layout.tsx
import { WalletProvider } from '@/contexts/WalletContext';

export default function App({ Component, pageProps }) {
  return (
    <WalletProvider>
      <Component {...pageProps} />
    </WalletProvider>
  );
}
```

```typescript
// components/Header.tsx
import { WalletSelector } from '@/components/WalletSelector';

export function Header() {
  return (
    <header>
      <h1>BlackBook Prediction Market</h1>
      <WalletSelector />
    </header>
  );
}
```

### Step 4: Place Bets with Connected Wallet

```typescript
// components/MarketCard.tsx
import { useWallet } from '@/contexts/WalletContext';

export function MarketCard({ market }) {
  const { account, isConnected, refreshBalance } = useWallet();
  const [betAmount, setBetAmount] = useState(10);

  const placeBet = async (outcome: number) => {
    if (!isConnected || !account) {
      alert('Please connect your wallet first');
      return;
    }

    try {
      const response = await fetch(`${RPC_URL}/bet`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          account: account.name,
          market: market.id,
          outcome: outcome,
          amount: betAmount,
        }),
      });

      const data = await response.json();

      if (data.success) {
        alert(`✅ Bet placed! New balance: ${data.new_balance} BB`);
        await refreshBalance(); // Update balance in UI
      } else {
        alert(`❌ ${data.message}`);
      }
    } catch (error) {
      alert(`Failed to place bet: ${error}`);
    }
  };

  return (
    <div className="market-card">
      <h3>{market.title}</h3>
      <p>{market.description}</p>
      
      {isConnected ? (
        <div className="bet-controls">
          <input 
            type="number" 
            value={betAmount}
            onChange={(e) => setBetAmount(Number(e.target.value))}
            min="1"
            max={account?.balance}
          />
          {market.options.map((option, idx) => (
            <button key={idx} onClick={() => placeBet(idx)}>
              Bet {betAmount} BB on "{option}"
            </button>
          ))}
        </div>
      ) : (
        <p>Connect wallet to place bets</p>
      )}
    </div>
  );
}
```

---

## 🎨 CSS Styling Example

```css
/* styles/wallet.css */
.wallet-connected {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.75rem 1rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border-radius: 12px;
  color: white;
}

.account-info {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.avatar {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 2px solid white;
}

.account-name {
  font-weight: 600;
  font-size: 14px;
}

.account-balance {
  font-size: 12px;
  opacity: 0.9;
}

.usd {
  margin-left: 0.25rem;
  opacity: 0.7;
}

.connect-btn {
  padding: 0.75rem 1.5rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.2s;
}

.connect-btn:hover {
  transform: scale(1.05);
}

.account-dropdown {
  position: absolute;
  top: 60px;
  right: 0;
  background: white;
  border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
  padding: 1.5rem;
  min-width: 400px;
  z-index: 1000;
}

.dropdown-header h3 {
  margin: 0 0 0.5rem 0;
  color: #333;
}

.account-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 1rem;
  margin-top: 1rem;
}

.account-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 1rem;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s;
  background: white;
}

.account-card:hover {
  border-color: #667eea;
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.2);
}

.account-details {
  text-align: center;
  margin-top: 0.5rem;
}

.account-details .name {
  font-weight: 600;
  color: #333;
  margin-bottom: 0.25rem;
}

.account-details .balance {
  color: #667eea;
  font-weight: 500;
}

.account-details .address {
  font-size: 11px;
  color: #999;
  font-family: monospace;
}
```

---

## 🧪 Testing Your Integration

### 1. Start your blockchain:
```bash
cargo run
```

### 2. Test endpoints with curl:

```bash
# Get all test accounts
curl http://localhost:8080/wallet/test-accounts

# Connect to ALICE
curl http://localhost:8080/wallet/connect/ALICE

# Get BOB's account info
curl http://localhost:8080/wallet/account-info/BOB

# Place a bet
curl -X POST http://localhost:8080/bet \
  -H "Content-Type: application/json" \
  -d '{
    "account": "ALICE",
    "market": "crypto_bitcoin_100k",
    "outcome": 0,
    "amount": 10
  }'
```

---

## 🔐 Production Deployment

### Environment Variables

```bash
# .env.local (Frontend)
NEXT_PUBLIC_RPC_URL=https://blackbook.id
NEXT_PUBLIC_NETWORK_NAME=BlackBook L1
NEXT_PUBLIC_CHAIN_ID=1
```

### CORS Configuration
Your blockchain already has CORS enabled:
```rust
.layer(
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any),
)
```

---

## 📊 God Mode Features

Users can:
- ✅ View all 8 test accounts
- ✅ Switch between accounts instantly
- ✅ See real-time balance updates
- ✅ View transaction history per account
- ✅ See active bets and market participation
- ✅ No wallet extension needed (it's a testnet)

---

## 🚀 Next Steps

1. **Add transaction signing** (optional for production)
2. **Add private keys** for real wallet connections
3. **Implement MetaMask** for production wallets
4. **Add WebSocket** for real-time updates
5. **Add notifications** for bet outcomes

---

## 📞 Support

- Blockchain Repo: https://github.com/aMarketology/blackBook-ledger-rust
- API Docs: https://blackbook.id/
- Issues: Create an issue on GitHub

---

**Happy Building! 🎉**
