# 🔗 Blockchain Ledger Feed Integration Guide

## Overview
Connect your frontend to the BlackBook L1 blockchain activity feed in real-time.

## Available Endpoints

### 1. HTML View (Human-Readable)
```
GET http://localhost:8080/ledger
```
Beautiful styled HTML page with live activity feed - great for viewing in browser.

### 2. JSON API (For Apps & Frontend)
```
GET http://localhost:8080/ledger/json
```
Clean JSON response for programmatic access.

---

## JSON API Response Format

```json
{
  "success": true,
  "blockchain": {
    "network": "BlackBook L1",
    "token": "BB",
    "token_value_usd": 0.01
  },
  "stats": {
    "total_activities": 45,
    "active_markets": 12,
    "live_accounts": 9,
    "live_bets_active": 3
  },
  "activities": [
    {
      "timestamp": "22:35:45",
      "emoji": "🎲",
      "action_type": "BET_PLACED",
      "details": "L1_A673D58A6D6D4D4D9EB2DF2322E07CCA bet 10 BB on \"cooking_fail_videos_trend_2025\""
    },
    {
      "timestamp": "22:35:12",
      "emoji": "🔌",
      "action_type": "WALLET_CONNECTED",
      "details": "BOB connected from frontend | Balance: 970 BB ($9.70 USD)"
    }
  ],
  "metadata": {
    "max_stored": 100,
    "returned_count": 45,
    "description": "Real-time blockchain activity feed",
    "endpoints": {
      "html_view": "/ledger",
      "json_api": "/ledger/json"
    }
  },
  "timestamp": "2025-11-15T22:35:45Z"
}
```

---

## React/Next.js Integration

### 1. Create a Hook for Ledger Feed

```typescript
// hooks/useBlockchainFeed.ts
import { useState, useEffect } from 'react';

interface BlockchainActivity {
  timestamp: string;
  emoji: string;
  action_type: string;
  details: string;
}

interface BlockchainStats {
  total_activities: number;
  active_markets: number;
  live_accounts: number;
  live_bets_active: number;
}

interface LedgerResponse {
  success: boolean;
  stats: BlockchainStats;
  activities: BlockchainActivity[];
}

export const useBlockchainFeed = (refreshInterval: number = 3000) => {
  const [activities, setActivities] = useState<BlockchainActivity[]>([]);
  const [stats, setStats] = useState<BlockchainStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchLedger = async () => {
    try {
      const response = await fetch('http://localhost:8080/ledger/json');
      const data: LedgerResponse = await response.json();
      
      if (data.success) {
        setActivities(data.activities);
        setStats(data.stats);
        setError(null);
      }
    } catch (err) {
      setError('Failed to fetch blockchain feed');
      console.error('Ledger fetch error:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchLedger();
    const interval = setInterval(fetchLedger, refreshInterval);
    return () => clearInterval(interval);
  }, [refreshInterval]);

  return { activities, stats, loading, error, refresh: fetchLedger };
};
```

### 2. Create a Blockchain Feed Component

```typescript
// components/BlockchainFeed.tsx
import { useBlockchainFeed } from '@/hooks/useBlockchainFeed';

export const BlockchainFeed = () => {
  const { activities, stats, loading, error } = useBlockchainFeed(3000); // Refresh every 3 seconds

  if (loading) return <div>Loading blockchain feed...</div>;
  if (error) return <div>Error: {error}</div>;

  return (
    <div className="blockchain-feed">
      <div className="feed-header">
        <h2>🔗 Live Blockchain Activity</h2>
        <div className="stats">
          <span>📊 Activities: {stats?.total_activities}</span>
          <span>📈 Markets: {stats?.active_markets}</span>
          <span>👥 Accounts: {stats?.live_accounts}</span>
          <span>🎲 Live Bets: {stats?.live_bets_active}</span>
        </div>
      </div>

      <div className="activity-list">
        {activities.length === 0 ? (
          <p>No activity yet...</p>
        ) : (
          activities.map((activity, index) => (
            <div key={index} className="activity-item">
              <span className="timestamp">[{activity.timestamp}]</span>
              <span className="emoji">{activity.emoji}</span>
              <span className="action">{activity.action_type}</span>
              <span className="details">{activity.details}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
```

### 3. Add CSS Styling

```css
/* styles/BlockchainFeed.css */
.blockchain-feed {
  background: linear-gradient(135deg, #0f0f23 0%, #1a1a3e 100%);
  border: 2px solid #00ff41;
  border-radius: 10px;
  padding: 20px;
  font-family: 'Courier New', monospace;
  color: #00ff41;
}

.feed-header {
  text-align: center;
  margin-bottom: 20px;
}

.feed-header h2 {
  font-size: 24px;
  margin-bottom: 10px;
  text-shadow: 0 0 10px rgba(0, 255, 65, 0.8);
}

.stats {
  display: flex;
  justify-content: center;
  gap: 20px;
  flex-wrap: wrap;
  font-size: 14px;
  color: #00ccff;
}

.activity-list {
  max-height: 500px;
  overflow-y: auto;
}

.activity-item {
  padding: 12px;
  margin-bottom: 8px;
  background: rgba(0, 40, 20, 0.5);
  border-left: 4px solid #00ff41;
  border-radius: 5px;
  font-size: 13px;
  transition: all 0.3s ease;
}

.activity-item:hover {
  background: rgba(0, 60, 30, 0.7);
  transform: translateX(5px);
}

.timestamp {
  color: #888;
  font-weight: bold;
  margin-right: 10px;
}

.emoji {
  font-size: 18px;
  margin-right: 8px;
}

.action {
  color: #00ccff;
  font-weight: bold;
  margin-right: 8px;
}

.details {
  color: #aaffaa;
}

/* Scrollbar styling */
.activity-list::-webkit-scrollbar {
  width: 8px;
}

.activity-list::-webkit-scrollbar-track {
  background: rgba(0, 0, 0, 0.3);
  border-radius: 4px;
}

.activity-list::-webkit-scrollbar-thumb {
  background: #00ff41;
  border-radius: 4px;
}
```

---

## Vanilla JavaScript Integration

```javascript
// Simple fetch example
async function fetchBlockchainFeed() {
  try {
    const response = await fetch('http://localhost:8080/ledger/json');
    const data = await response.json();
    
    console.log('Stats:', data.stats);
    console.log('Activities:', data.activities);
    
    // Display activities
    const container = document.getElementById('blockchain-feed');
    container.innerHTML = data.activities.map(activity => `
      <div class="activity">
        <span>[${activity.timestamp}]</span>
        <span>${activity.emoji}</span>
        <strong>${activity.action_type}</strong>
        <span>${activity.details}</span>
      </div>
    `).join('');
    
  } catch (error) {
    console.error('Failed to fetch ledger:', error);
  }
}

// Poll every 3 seconds
setInterval(fetchBlockchainFeed, 3000);
fetchBlockchainFeed(); // Initial fetch
```

---

## Activity Types You'll See

| Emoji | Action Type | Description |
|-------|-------------|-------------|
| 🔌 | WALLET_CONNECTED | User connected their wallet |
| 📊 | ACCOUNT_INFO_VIEWED | User viewed account details |
| 🎲 | BET_PLACED | Bet placed on prediction market |
| ❌ | BET_FAILED | Bet attempt failed |
| 🎯 | BET_REQUEST | Initial bet request received |
| 🎰 | WAGER_PLACED | Casino game wager placed |
| 🏆 | WAGER_SETTLED | Casino game outcome settled |
| 💰 | TRANSFER | Funds transferred between accounts |
| 🏦 | DEPOSIT | Funds deposited |
| 📈 | MARKET_CREATED | New prediction market created |
| ✅ | MARKET_RESOLVED | Prediction market resolved |

---

## Real-Time Updates Strategy

### Option 1: Polling (Recommended for MVP)
```typescript
// Fetch every 3 seconds
useEffect(() => {
  const interval = setInterval(fetchLedger, 3000);
  return () => clearInterval(interval);
}, []);
```

**Pros:**
- Simple to implement
- Works with current API
- No WebSocket setup needed

**Cons:**
- 3-second delay for updates
- More HTTP requests

### Option 2: WebSocket (Future Enhancement)
For true real-time updates, you could add WebSocket support to the Rust backend:

```rust
// Future TODO: Add WebSocket endpoint
.route("/ws/ledger", get(ledger_websocket))
```

---

## Integration Example: Dashboard Widget

```typescript
// pages/dashboard.tsx
import { BlockchainFeed } from '@/components/BlockchainFeed';
import { WalletSelector } from '@/components/WalletSelector';

export default function Dashboard() {
  return (
    <div className="dashboard">
      <div className="left-panel">
        <WalletSelector />
        <MarketsList />
      </div>
      
      <div className="right-panel">
        {/* Live blockchain feed */}
        <BlockchainFeed />
      </div>
    </div>
  );
}
```

---

## Testing the Endpoints

### Using curl
```bash
# Get JSON feed
curl http://localhost:8080/ledger/json | jq

# View in browser
open http://localhost:8080/ledger
```

### Using JavaScript console
```javascript
fetch('http://localhost:8080/ledger/json')
  .then(r => r.json())
  .then(console.log);
```

---

## CORS Configuration

The backend already has CORS enabled for all origins:
```rust
.layer(CorsLayer::permissive())
```

So your frontend can call from any domain during development!

---

## Performance Tips

1. **Cache the feed**: Store in React state to avoid re-fetching
2. **Limit display**: Show only last 20-30 activities in UI
3. **Debounce updates**: Don't update UI on every single activity
4. **Virtual scrolling**: For long lists, use react-window or react-virtualized

---

## Next Steps

1. ✅ Add `<BlockchainFeed />` component to your app
2. ✅ Set refresh interval (3-5 seconds recommended)
3. ✅ Style it to match your design system
4. ✅ Add filters (show only bets, transfers, etc.)
5. ✅ Add sound notifications for new activities (optional)
6. ✅ Add export to CSV feature (optional)

---

## Questions?

The blockchain is running at `http://localhost:8080`

- HTML view: `GET /ledger`
- JSON API: `GET /ledger/json`
- Max stored activities: 100 (automatically pruned)
- Update frequency: Real-time (polls every 3 seconds in frontend)

Happy building! 🚀
