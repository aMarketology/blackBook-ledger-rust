# 🎲 BlackBook L1 Blockchain - Prediction Markets

A Layer 1 blockchain with prediction markets, built in Rust with a live HTML dashboard.

## 🚀 What's Included

- ✅ **Rust Blockchain Backend** - Full L1 blockchain with 40+ API endpoints
- ✅ **HTML Frontend Dashboard** - Live ledger visualization
- ✅ **Prediction Markets** - Create and bet on markets
- ✅ **Real-time Updates** - Live blockchain activity feed
- ✅ **8 Pre-funded Accounts** - Ready to use (1000 BB each)

## 🌐 Deploy to Render.com

**Your blockchain + frontend will be live in 5 minutes!**

1. **Push to GitHub**:
   ```bash
   git add .
   git commit -m "Deploy to Render"
   git push origin master
   ```

2. **Deploy on Render**:
   - Go to https://dashboard.render.com
   - Click "New" → "Blueprint"
   - Connect your repo: `aMarketology/blackBook-ledger-rust`
   - Click "Apply"
   - Wait 5-10 minutes for build

3. **Access Your Live App**:
   - Frontend: `https://your-app.onrender.com/`
   - API: `https://your-app.onrender.com/health`

📖 **Full Guide**: [RENDER_DEPLOY.md](RENDER_DEPLOY.md)

## 💻 Run Locally

**Quick Start (2 commands)**:

```bash
# Build the blockchain
cargo build --release

# Run the server
../target/release/blackbook-prediction-market
```

Then open: http://localhost:3000

📖 **Full Guide**: [DEPLOYMENT.md](DEPLOYMENT.md)

## 📂 Project Structure

```
blackBook/
├── src/
│   ├── main.rs          # Blockchain server (binds to 0.0.0.0:3000)
│   ├── ledger.rs        # L1 blockchain ledger
│   ├── markets.rs       # Prediction markets
│   └── escrow.rs        # Market escrow system
├── index.html           # Frontend dashboard
├── Dockerfile           # Builds both blockchain + frontend
├── render.yaml          # Render.com configuration
└── docker-compose.yml   # Local Docker deployment
```

## 🔗 API Endpoints

Access at `http://localhost:3000` or your Render URL:

- `GET /` - Live dashboard (HTML frontend)
- `GET /health` - Health check
- `GET /accounts` - All blockchain accounts
- `GET /markets` - All prediction markets
- `POST /bet` - Place a bet
- `GET /leaderboard` - Featured markets
- `GET /stats` - Blockchain statistics

## 🎯 Features

- **Layer 1 Blockchain** - Custom L1 with unique addresses (L1_xxxxx)
- **BlackBook Token (BB)** - Stable at $0.01
- **Prediction Markets** - Sports, crypto, tech, politics, business
- **Market Leaderboard** - Featured markets with 10+ bettors
- **Live Price Betting** - 1-min and 15-min BTC/SOL bets
- **AI Event Integration** - RSS feed for AI-generated markets
- **Real-time Activity Feed** - Live blockchain monitoring

## 🛠️ Tech Stack

- **Backend**: Rust + Axum web framework
- **Frontend**: Vanilla HTML/CSS/JavaScript
- **Database**: Sled (embedded blockchain storage)
- **Deployment**: Docker + Render.com
- **APIs**: CoinGecko (crypto prices)

## 📊 Blockchain Info

- **Network**: Layer 1 (L1)
- **Token**: BlackBook (BB)
- **Initial Supply**: 8,000 BB (8 accounts × 1,000 BB)
- **Accounts**: ALICE, BOB, CHARLIE, DIANA, ETHAN, FIONA, GEORGE, HANNAH
- **Consensus**: Centralized (suitable for prediction markets)

## 🚀 Deployment Options

| Method | Difficulty | Time | Cost |
|--------|-----------|------|------|
| **Render.com** | ⭐ Easy | 5-10 min | Free |
| **Docker** | ⭐⭐ Medium | 2 min | Free |
| **Manual** | ⭐⭐⭐ Hard | 5 min | Free |

## 📖 Documentation

- [RENDER_DEPLOY.md](RENDER_DEPLOY.md) - Deploy to Render.com (recommended)
- [DEPLOYMENT.md](DEPLOYMENT.md) - All deployment methods
- [BLOCKCHAIN_ARCHITECTURE.md](../BLOCKCHAIN_ARCHITECTURE.md) - Architecture details

## 🤝 Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

## 📝 License

MIT License - feel free to use for your own projects!

## 🆘 Support

- **Issues**: Create a GitHub issue
- **Render Help**: Check [RENDER_DEPLOY.md](RENDER_DEPLOY.md)
- **Local Setup**: Check [DEPLOYMENT.md](DEPLOYMENT.md)

---

**Built with ❤️ by aMarketology**
