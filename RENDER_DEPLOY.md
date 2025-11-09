# 🚀 Deploy BlackBook to Render.com

This guide shows you how to deploy your BlackBook blockchain + HTML frontend to Render.com.

## 🎯 What Gets Deployed

When you deploy to Render, you get:
- ✅ **Rust Blockchain Backend** - Running on Render's servers
- ✅ **HTML Frontend** - Your live dashboard accessible via web
- ✅ **Public URL** - Automatic HTTPS (e.g., `https://blackbook-blockchain.onrender.com`)
- ✅ **Auto-Deploy** - Updates automatically when you push to GitHub
- ✅ **Free Tier Available** - Start for free!

---

## 📋 Prerequisites

1. **GitHub Account** - Your code must be in a GitHub repository
2. **Render Account** - Sign up at [render.com](https://render.com) (free)
3. **Code Pushed to GitHub** - Make sure your latest code is committed and pushed

---

## 🚀 Deployment Steps

### Step 1: Prepare Your Repository

Make sure you have these files in your `blackBook` directory:

```
blackBook/
├── Dockerfile          ✅ (builds Rust + includes HTML)
├── render.yaml         ✅ (tells Render how to deploy)
├── index.html          ✅ (your frontend)
├── src/                ✅ (your Rust code)
├── Cargo.toml          ✅
└── Cargo.lock          ✅
```

All these files are already created! Just commit and push:

```bash
git add .
git commit -m "Add Render deployment configuration"
git push origin master
```

### Step 2: Deploy on Render (Two Methods)

#### Method A: Blueprint Deploy (Recommended - Automatic)

1. **Go to**: https://dashboard.render.com
2. **Click**: "New" → "Blueprint"
3. **Connect Repository**: 
   - Select `aMarketology/blackBook-ledger-rust`
   - Authorize Render to access your repo
4. **Render detects `render.yaml`** and shows:
   - Service Name: `blackbook-blockchain`
   - Type: Web Service
   - Environment: Docker
5. **Click "Apply"**
6. **Wait 5-10 minutes** for first build
7. **Access your app** at the provided URL!

#### Method B: Manual Setup

1. **Go to**: https://dashboard.render.com
2. **Click**: "New" → "Web Service"
3. **Connect your GitHub repository**
4. **Configure the service**:
   ```
   Name: blackbook-blockchain
   Region: Oregon (or closest to you)
   Branch: master
   Root Directory: blackBook
   Environment: Docker
   Dockerfile Path: ./Dockerfile
   Instance Type: Free (or Starter)
   ```
5. **Environment Variables** (click "Advanced"):
   ```
   PORT = 3000
   RUST_LOG = info
   ```
6. **Health Check Path**: `/health`
7. **Click "Create Web Service"**

### Step 3: Monitor the Build

Render will:
1. ✅ Clone your repository
2. ✅ Build the Rust blockchain binary (takes ~5-8 minutes)
3. ✅ Copy your `index.html` frontend into the image
4. ✅ Start the server on port 3000
5. ✅ Assign you a public URL

You can watch the build logs in real-time on the Render dashboard.

### Step 4: Access Your Live App

Once deployed, you'll get a URL like:
```
https://blackbook-blockchain.onrender.com
```

**Test it:**
- 🌐 **Frontend**: https://your-app.onrender.com/
- 🔗 **Health Check**: https://your-app.onrender.com/health
- 📊 **Markets API**: https://your-app.onrender.com/markets
- 👥 **Accounts**: https://your-app.onrender.com/accounts

---

## 🎉 You're Live!

Both your blockchain backend AND HTML frontend are now deployed and accessible to the world!

---

## 🔧 Configuration Details

### Environment Variables on Render

These are automatically set from `render.yaml`:

| Variable | Value | Purpose |
|----------|-------|---------|
| `PORT` | `3000` | Server port (required by Render) |
| `RUST_LOG` | `info` | Logging level |

### Dockerfile Behavior

The Dockerfile does this automatically:

1. **Stage 1 (Builder)**:
   - Uses Rust 1.75
   - Installs build dependencies
   - Compiles your blockchain in release mode
   - Produces optimized binary (~5MB)

2. **Stage 2 (Runtime)**:
   - Minimal Debian image
   - Copies compiled binary
   - **Copies `index.html` frontend**
   - Sets up data directory for blockchain persistence
   - Exposes port 3000
   - Starts server on `0.0.0.0:3000`

---

## 🔄 Auto-Deploy

After your first deployment, every time you push to GitHub:

```bash
git add .
git commit -m "Update blockchain or frontend"
git push
```

Render will **automatically**:
1. Detect the push
2. Rebuild the Docker image
3. Redeploy with zero downtime
4. Keep your blockchain data intact

---

## 💰 Pricing

### Free Tier (Perfect for Testing)
- ✅ 750 hours/month (always-on if < 750 hours)
- ✅ HTTPS included
- ✅ Custom domain support
- ⚠️ Spins down after 15 min inactivity
- ⚠️ Cold starts take ~30 seconds

### Starter Plan ($7/month)
- ✅ Always-on (no sleep)
- ✅ Instant responses
- ✅ More memory & CPU
- ✅ Perfect for production

---

## 🐛 Troubleshooting

### Build Fails

**Error**: "Cargo build failed"
- Check that `Cargo.toml` and `Cargo.lock` are committed
- Verify all source files are in `src/`
- Check build logs on Render dashboard

### Frontend Not Loading

**Error**: "Cannot GET /"
- Verify `index.html` is in the root of `blackBook/` directory
- Check that Dockerfile has: `COPY index.html /app/index.html`
- Verify `serve_live_ledger()` in `main.rs` serves the HTML

### Port Issues

**Error**: "Application failed to respond"
- Render automatically sets `PORT` environment variable
- Your app reads it correctly: `std::env::var("PORT")`
- Health check should respond at `/health`

### Database Persistence

**Note**: Render's free tier doesn't persist disk storage between deploys
- Consider using Render's Disk storage (paid)
- Or connect to external database if you need persistence

---

## 📊 Monitoring

### View Logs

On Render dashboard:
1. Click your service
2. Go to "Logs" tab
3. See real-time blockchain activity

### Health Checks

Render automatically checks `/health` every 30 seconds:
- ✅ **Healthy**: Your service is running
- ❌ **Unhealthy**: Render will restart automatically

### Metrics

View on Render dashboard:
- CPU usage
- Memory usage
- Request count
- Response times

---

## 🔒 Security Best Practices

1. **Environment Variables**: Store sensitive config in Render's environment variables, not in code
2. **Admin Endpoints**: Consider adding authentication to `/admin/*` routes
3. **Rate Limiting**: Add rate limiting for production use
4. **HTTPS Only**: Render provides automatic HTTPS - always use it

---

## 🎯 Quick Reference Commands

### Local Development
```bash
# Run locally
cargo run

# Build release
cargo build --release

# Test Docker build locally
docker build -t blackbook .
docker run -p 3000:3000 blackbook
```

### Git Deployment
```bash
# Deploy changes to Render
git add .
git commit -m "Your changes"
git push origin master
# Render auto-deploys!
```

### API Testing
```bash
# Replace with your Render URL
export API_URL="https://your-app.onrender.com"

# Test health
curl $API_URL/health

# Test markets
curl $API_URL/markets

# Test accounts
curl $API_URL/accounts
```

---

## ✅ Deployment Checklist

Before deploying:
- [ ] All code committed to GitHub
- [ ] `Dockerfile` exists in `blackBook/`
- [ ] `render.yaml` exists in `blackBook/`
- [ ] `index.html` exists in `blackBook/`
- [ ] `src/main.rs` binds to `0.0.0.0` (not `127.0.0.1`)
- [ ] Server reads `PORT` environment variable
- [ ] `/health` endpoint works

After deploying:
- [ ] Build succeeds on Render
- [ ] Service shows "Live"
- [ ] Health checks pass
- [ ] Frontend loads at root URL
- [ ] API endpoints respond
- [ ] Blockchain accounts initialized

---

## 🆘 Need Help?

- **Render Docs**: https://render.com/docs
- **Build Logs**: Check Render dashboard → Your Service → Logs
- **GitHub Issues**: Create issue in your repository
- **Render Support**: support@render.com

---

## 🎊 Success!

Your BlackBook blockchain and HTML frontend are now live on the internet! 🚀

Share your URL and start accepting bets on prediction markets!
