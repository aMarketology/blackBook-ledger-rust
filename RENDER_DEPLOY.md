# Deploy BlackBook Blockchain to Render

## Quick Deploy to blackbook.id

### Option 1: Render Blueprint (Recommended)
1. Go to https://render.com/deploy
2. Connect your GitHub account
3. Select repository: `aMarketology/blackBook-ledger-rust`
4. Render will auto-detect `render.yaml` and deploy

### Option 2: Manual Deploy
1. Log in to https://render.com
2. Click **New** → **Web Service**
3. Connect your GitHub repository: `aMarketology/blackBook-ledger-rust`
4. Configure:
   - **Name**: `blackbook-blockchain`
   - **Runtime**: Docker
   - **Branch**: `master`
   - **Docker Context**: `.`
   - **Dockerfile Path**: `./Dockerfile`
5. Add Environment Variable:
   - `PORT=10000` (Render sets this automatically)
6. Click **Create Web Service**

## Custom Domain Setup (blackbook.id)

After deployment:

1. Go to your service settings
2. Click **Custom Domains**
3. Add your domain: `blackbook.id` or `api.blackbook.id`
4. Render will provide CNAME/ALIAS records
5. Add those DNS records at your domain registrar:
   - Type: `CNAME`
   - Name: `@` or `api`
   - Value: `your-service.onrender.com`

## Environment Variables

The app uses these environment variables:
- `PORT` - Server port (Render sets this automatically to 10000)
- `RUST_LOG` - Log level (optional, default: info)

## Health Check

Render will check: `https://blackbook.id/health`

Response:
```json
{
  "status": "healthy",
  "service": "BlackBook Prediction Market",
  "version": "1.0.0",
  "timestamp": "2024-01-01T00:00:00Z"
}
```

## API Endpoints

Once deployed, your blockchain will be live at:
- `https://blackbook.id/` - API info
- `https://blackbook.id/health` - Health check
- `https://blackbook.id/accounts` - Get all accounts
- `https://blackbook.id/markets` - Get all prediction markets
- `https://blackbook.id/stats` - Get blockchain stats
- `https://blackbook.id/leaderboard` - Get featured markets

## Troubleshooting

### Deployment fails
- Check logs in Render dashboard
- Ensure Cargo.toml and Cargo.lock are committed
- Verify Rust 1.82 compatibility

### Can't connect to API
- Check if service is "Live" in Render dashboard
- Verify custom domain DNS records
- Check health endpoint: `https://your-service.onrender.com/health`

### Database persistence
- Data stored in-memory (Sled DB)
- For production, consider adding persistent volume in Render settings

## What Gets Deployed

✅ Rust blockchain server (Layer 1)
✅ 8 pre-funded accounts (ALICE, BOB, etc.)
✅ 70+ prediction markets
✅ REST API (40+ endpoints)
✅ Live BTC/SOL price betting
✅ Leaderboard system
✅ Real-time transaction logging

## Cost

- Free tier: Service sleeps after 15 min inactivity
- Paid tier ($7/mo): Always running, custom domain support

## Next Steps

1. Deploy using Option 1 or 2 above
2. Wait 5-10 minutes for build
3. Test health endpoint
4. Configure custom domain
5. Start betting! 🎲
