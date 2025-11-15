# Casino & Wager API Documentation

## 🎰 General Wager Endpoints

These endpoints allow you to place wagers for casino games (blackjack, poker, roulette, etc.) and settle game outcomes.

---

## Endpoints

### 1. Place a Wager
**Endpoint:** `POST /wager`

Place a wager for any casino game or peer-to-peer bet.

**Request Body:**
```json
{
  "from": "ALICE",                    // Player placing the wager
  "to": null,                         // Optional opponent (null = house)
  "amount": 50.0,                     // Wager amount in BB
  "game_type": "blackjack",           // Game type
  "game_id": "bj_12345",             // Optional game session ID
  "description": "Blackjack hand #42" // Description
}
```

**Game Types:**
- `blackjack` - Blackjack game
- `poker` - Poker game
- `roulette` - Roulette spin
- `dice` - Dice roll
- `slots` - Slot machine
- `custom` - Custom game/bet

**Response:**
```json
{
  "success": true,
  "transaction_id": "tx_abc123",
  "game_id": "bj_12345",
  "wager": {
    "from": "ALICE",
    "to": "HOUSE",
    "amount": 50.0,
    "game_type": "blackjack",
    "description": "Blackjack hand #42"
  },
  "new_balance": 950.0,
  "message": "Wager placed: 50 BB on blackjack"
}
```

**Example (curl):**
```bash
curl -X POST http://localhost:8080/wager \
  -H "Content-Type: application/json" \
  -d '{
    "from": "ALICE",
    "to": null,
    "amount": 50.0,
    "game_type": "blackjack",
    "game_id": "bj_12345",
    "description": "Blackjack hand #42"
  }'
```

---

### 2. Settle a Wager
**Endpoint:** `POST /wager/settle`

Settle a game outcome and pay the winner.

**Request Body:**
```json
{
  "transaction_id": "tx_abc123",      // Original wager transaction ID
  "winner": "ALICE",                   // Winner account name
  "payout_amount": 100.0,              // Amount to pay out (includes original wager)
  "game_result": "Blackjack! 21 vs 19" // Game outcome description
}
```

**Response:**
```json
{
  "success": true,
  "settlement_tx_id": "tx_def456",
  "original_tx_id": "tx_abc123",
  "winner": "ALICE",
  "payout": 100.0,
  "new_balance": 1050.0,
  "game_result": "Blackjack! 21 vs 19",
  "message": "ALICE won 100 BB!"
}
```

**Example (curl):**
```bash
curl -X POST http://localhost:8080/wager/settle \
  -H "Content-Type: application/json" \
  -d '{
    "transaction_id": "tx_abc123",
    "winner": "ALICE",
    "payout_amount": 100.0,
    "game_result": "Blackjack! 21 vs 19"
  }'
```

---

### 3. Get Wager History
**Endpoint:** `GET /wager/history/:account`

Get all wagers for a specific account.

**Example:** `GET /wager/history/ALICE`

**Response:**
```json
{
  "success": true,
  "account": "ALICE",
  "wager_count": 15,
  "wagers": [
    {
      "from": "ALICE",
      "to": "HOUSE",
      "amount": 50.0,
      "tx_type": "transfer",
      "memo": "Blackjack wager",
      "timestamp": 1700000000
    },
    // ... more wagers
  ]
}
```

**Example (curl):**
```bash
curl http://localhost:8080/wager/history/ALICE
```

---

## 🎮 Integration Examples

### React/Next.js - Blackjack Game

```typescript
// hooks/useWager.ts
import { useWallet } from './useWallet';

const RPC_URL = process.env.NEXT_PUBLIC_RPC_URL || 'http://localhost:8080';

export function useWager() {
  const { account } = useWallet();

  const placeWager = async (amount: number, gameType: string, gameId: string) => {
    if (!account) throw new Error('No wallet connected');

    const response = await fetch(`${RPC_URL}/wager`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        from: account.name,
        to: null, // Betting against house
        amount,
        game_type: gameType,
        game_id: gameId,
        description: `${gameType} game ${gameId}`,
      }),
    });

    const data = await response.json();
    
    if (!data.success) {
      throw new Error(data.error || 'Failed to place wager');
    }

    return data;
  };

  const settleWager = async (
    txId: string, 
    winner: string, 
    payout: number, 
    result: string
  ) => {
    const response = await fetch(`${RPC_URL}/wager/settle`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        transaction_id: txId,
        winner,
        payout_amount: payout,
        game_result: result,
      }),
    });

    const data = await response.json();
    
    if (!data.success) {
      throw new Error(data.error || 'Failed to settle wager');
    }

    return data;
  };

  const getWagerHistory = async (accountName: string) => {
    const response = await fetch(`${RPC_URL}/wager/history/${accountName}`);
    const data = await response.json();
    return data;
  };

  return { placeWager, settleWager, getWagerHistory };
}
```

### Blackjack Component

```typescript
// components/BlackjackGame.tsx
import { useState } from 'react';
import { useWallet } from '@/hooks/useWallet';
import { useWager } from '@/hooks/useWager';

export function BlackjackGame() {
  const { account, refreshBalance } = useWallet();
  const { placeWager, settleWager } = useWager();
  const [gameState, setGameState] = useState<'idle' | 'betting' | 'playing'>('idle');
  const [betAmount, setBetAmount] = useState(10);
  const [currentWager, setCurrentWager] = useState<any>(null);
  const [gameId, setGameId] = useState('');

  const startGame = async () => {
    if (!account) return;

    const newGameId = `bj_${Date.now()}`;
    setGameId(newGameId);

    try {
      // Place wager
      const wagerResult = await placeWager(betAmount, 'blackjack', newGameId);
      setCurrentWager(wagerResult);
      setGameState('playing');
      
      console.log('✅ Wager placed:', wagerResult);
      await refreshBalance();

      // TODO: Start actual blackjack game logic here
      
    } catch (error) {
      alert(`Failed to place wager: ${error}`);
    }
  };

  const finishGame = async (playerWon: boolean) => {
    if (!currentWager || !account) return;

    try {
      const payout = playerWon ? betAmount * 2 : 0; // 2x on win, 0 on loss
      const winner = playerWon ? account.name : 'HOUSE';
      const result = playerWon 
        ? `Player wins! 21 vs 19` 
        : `House wins! 22 (bust)`;

      const settlement = await settleWager(
        currentWager.transaction_id,
        winner,
        payout,
        result
      );

      console.log('✅ Game settled:', settlement);
      await refreshBalance();

      alert(playerWon 
        ? `🎉 You won ${payout} BB!` 
        : `💀 House wins. Better luck next time!`
      );

      // Reset game
      setGameState('idle');
      setCurrentWager(null);
      
    } catch (error) {
      alert(`Failed to settle game: ${error}`);
    }
  };

  return (
    <div className="blackjack-game">
      <h2>♠️ Blackjack</h2>
      
      {gameState === 'idle' && (
        <div className="bet-controls">
          <label>
            Bet Amount:
            <input 
              type="number" 
              value={betAmount}
              onChange={(e) => setBetAmount(Number(e.target.value))}
              min="1"
              max={account?.balance || 0}
            />
            BB
          </label>
          <button onClick={startGame}>
            Place Bet & Deal
          </button>
        </div>
      )}

      {gameState === 'playing' && (
        <div className="game-area">
          <p>Game ID: {gameId}</p>
          <p>Current Wager: {betAmount} BB</p>
          
          {/* Your blackjack game UI here */}
          
          <div className="game-controls">
            <button onClick={() => finishGame(true)}>
              Simulate Win
            </button>
            <button onClick={() => finishGame(false)}>
              Simulate Loss
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
```

---

## 🏦 HOUSE Account

The blockchain now includes a **HOUSE** account for casino operations:

- **Account Name:** `HOUSE`
- **Initial Balance:** 10,000 BB (10x normal accounts)
- **Purpose:** Acts as the casino bankroll
- **Address:** Dynamically generated `L1_` address

When players place wagers without specifying an opponent (`to: null`), they automatically bet against the HOUSE.

---

## 📊 Blockchain Activity Log

All wagers are logged in real-time:

```
[12:34:56] 🎰 WAGER_PLACED | ALICE wagered 50 BB on blackjack | Game ID: bj_12345 | To: HOUSE | Balance: 950 BB
[12:35:12] 🏆 WAGER_SETTLED | ALICE won 100 BB | Result: Blackjack! 21 vs 19 | Balance: 1050 BB
```

---

## 🔄 Workflow

### Standard Casino Game Flow

1. **Player connects wallet** → `GET /wallet/connect/ALICE`
2. **Player places wager** → `POST /wager` 
   - Deducts bet from player balance
   - Transfers to HOUSE account
3. **Game resolves** → `POST /wager/settle`
   - If player wins: HOUSE pays out to player
   - If house wins: No additional transfer needed
4. **Player checks balance** → `GET /wallet/connect/ALICE`

### Peer-to-Peer Bet Flow

1. **Player 1 places wager** → `POST /wager` with `to: "BOB"`
2. **Player 2 accepts** → (handled by your frontend logic)
3. **Game resolves** → `POST /wager/settle`
4. **Winner receives payout** from escrow

---

## ✅ Testing

### Test placing a wager:
```bash
curl -X POST http://localhost:8080/wager \
  -H "Content-Type: application/json" \
  -d '{
    "from": "ALICE",
    "to": null,
    "amount": 50.0,
    "game_type": "blackjack",
    "description": "Test game"
  }'
```

### Test settling a wager:
```bash
curl -X POST http://localhost:8080/wager/settle \
  -H "Content-Type: application/json" \
  -d '{
    "transaction_id": "YOUR_TX_ID_HERE",
    "winner": "ALICE",
    "payout_amount": 100.0,
    "game_result": "Player wins!"
  }'
```

### View wager history:
```bash
curl http://localhost:8080/wager/history/ALICE | jq .
```

---

## 🚀 Production Deployment

The wager endpoints work on both local and production:

- **Local:** `http://localhost:8080`
- **Production:** `https://blackbook.id`

All CORS headers are already configured for cross-origin requests.

---

## 🎯 Supported Game Types

| Game Type | Description |
|-----------|-------------|
| `blackjack` | Blackjack / 21 |
| `poker` | Poker games |
| `roulette` | Roulette wheel |
| `dice` | Dice rolls |
| `slots` | Slot machines |
| `baccarat` | Baccarat |
| `craps` | Craps table |
| `custom` | Custom game logic |

---

**Happy Gaming! 🎰🃏🎲**
