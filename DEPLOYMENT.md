# 🚀 BlackBook Blockchain Deployment Guide

This guide covers deploying the BlackBook L1 blockchain with its HTML frontend.

## 📋 What Gets Deployed

When you build and deploy BlackBook, you get:
- ✅ **Rust Blockchain Backend** - Layer 1 blockchain running on port 3000
- ✅ **HTML Frontend** - Live dashboard accessible at `http://0.0.0.0:3000/`
- ✅ **REST API** - 40+ HTTP endpoints for market operations
- ✅ **WebSocket Support** - Real-time updates (via CORS-enabled endpoints)

---

## 🐳 Docker Deployment (Recommended)

### Prerequisites
You need Docker installed. Choose your installation method:

#### macOS - Install Docker Desktop
```bash
# Option 1: Using Homebrew (recommended)
brew install --cask docker

# Option 2: Download from Docker website
# Visit: https://docs.docker.com/desktop/install/mac-install/
```

After installation, **open Docker Desktop** from Applications. Wait for it to start (you'll see the Docker icon in your menu bar).

#### Linux - Install Docker
```bash
# Ubuntu/Debian
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker $USER
newgrp docker

# Verify installation
docker --version
```

### Quick Start - Single Command

```bash
# For modern Docker (includes compose)
docker compose up --build

# For older Docker installations
docker-compose up --build
```

This command will:
1. Build the Rust blockchain binary
2. Copy the HTML frontend into the container
3. Start the server on `http://0.0.0.0:3000`
4. Enable both blockchain API and frontend access

### Access Your Deployment

Once running, access your application at:
- **Frontend Dashboard**: http://localhost:3000/
- **Health Check**: http://localhost:3000/health
- **API Endpoints**: http://localhost:3000/markets, etc.

### Production Deployment

For production, run in detached mode:

```bash
docker-compose up -d --build
```

View logs:
```bash
docker-compose logs -f
```

Stop the service:
```bash
docker-compose down
```

### Environment Variables

You can customize the deployment via environment variables in `docker-compose.yml`:

```yaml
environment:
  - RUST_LOG=info          # Logging level: debug, info, warn, error
  - PORT=3000              # Server port (default: 3000)
```

---

## 🛠️ Manual Deployment (Without Docker) - EASIEST METHOD

### Prerequisites
- Rust 1.75+ installed ([Get Rust](https://rustup.rs/))

**Install Rust (if not already installed):**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build and Run - 2 Commands

```bash
# 1. Build in release mode (takes a few minutes first time)
cargo build --release

# 2. Run the server
./target/release/blackbook-prediction-market
```

**That's it!** Your blockchain and frontend are now live at:
- 🌐 **Frontend**: http://localhost:3000/
- 🔗 **API**: http://localhost:3000/health

### Run on Custom Port

```bash
PORT=8080 ./target/release/blackbook-prediction-market
```

---

## 🚀 Quick Start for Your Mac (No Docker Required)

If you want the **fastest** deployment without Docker, use this method:

### Step 1: Check if Rust is installed
```bash
rustc --version
```

If you see a version number, skip to Step 3. Otherwise, continue:

### Step 2: Install Rust (one command)
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
```

### Step 3: Build and Run
```bash
# Navigate to your project
cd /Users/thelegendofzjui/Documents/GitHub/BlackBook_TK/blackBook

# Build (first time takes 3-5 minutes)
cargo build --release

# Run
./target/release/blackbook-prediction-market
```

### Step 4: Access Your App
Open your browser to: **http://localhost:3000**

You'll see your BlackBook blockchain dashboard with all markets and the live ledger!

---

## 🛠️ Alternative: Development Mode (Faster Build)

For testing and development, skip the release build:

```bash
cargo run
```

This compiles faster but the binary isn't optimized for production.

---

## 🐳 Docker Deployment (If You Want Containerization)

### Prerequisites
- Rust 1.75+ installed ([Get Rust](https://rustup.rs/))
- OpenSSL development libraries

### Build the Application

```bash
# Build in release mode (optimized)
cargo build --release

# The binary will be at: target/release/blackbook-prediction-market
```

### Run the Application

```bash
# Run with default settings (port 3000)
./target/release/blackbook-prediction-market

# Or specify a custom port
PORT=8080 ./target/release/blackbook-prediction-market
```

### Access Points
- Frontend: http://0.0.0.0:3000/ (or your custom port)
- API: http://0.0.0.0:3000/health

---

## ☁️ Cloud Deployment

### Deploy to AWS EC2

1. **Launch an EC2 instance** (Amazon Linux 2 or Ubuntu)
2. **Install Docker**:
   ```bash
   sudo yum update -y
   sudo yum install docker -y
   sudo systemctl start docker
   sudo usermod -a -G docker ec2-user
   ```
3. **Clone your repository**:
   ```bash
   git clone https://github.com/aMarketology/blackBook-ledger-rust.git
   cd blackBook-ledger-rust/blackBook
   ```
4. **Deploy with Docker Compose**:
   ```bash
   docker-compose up -d --build
   ```
5. **Configure Security Group** - Open port 3000 for inbound traffic
6. **Access**: `http://<your-ec2-public-ip>:3000`

### Deploy to DigitalOcean Droplet

1. **Create a Droplet** (Ubuntu 22.04)
2. **SSH into your droplet**
3. **Install Docker**:
   ```bash
   curl -fsSL https://get.docker.com -o get-docker.sh
   sudo sh get-docker.sh
   sudo usermod -aG docker $USER
   ```
4. **Clone and deploy**:
   ```bash
   git clone https://github.com/aMarketology/blackBook-ledger-rust.git
   cd blackBook-ledger-rust/blackBook
   docker-compose up -d --build
   ```
5. **Configure Firewall**:
   ```bash
   sudo ufw allow 3000
   sudo ufw enable
   ```
6. **Access**: `http://<your-droplet-ip>:3000`

### Deploy to Render.com (RECOMMENDED - FREE TIER AVAILABLE)

Render will automatically build both your Rust blockchain AND serve your HTML frontend.

#### Option 1: One-Click Deploy with render.yaml (Easiest)

1. **Push your code to GitHub** (including the `render.yaml` file)
2. **Go to [Render Dashboard](https://dashboard.render.com/)**
3. **Click "New" → "Blueprint"**
4. **Connect your GitHub repository**: `aMarketology/blackBook-ledger-rust`
5. **Render will detect `render.yaml` and configure everything automatically**
6. **Click "Apply"** - Render will:
   - Build your Rust blockchain using the Dockerfile
   - Include your `index.html` frontend
   - Deploy to a public URL (e.g., `https://blackbook-blockchain.onrender.com`)
   - Set up health checks
   - Enable auto-deploy on git push

#### Option 2: Manual Setup

1. **Go to [Render Dashboard](https://dashboard.render.com/)**
2. **Click "New" → "Web Service"**
3. **Connect your GitHub repository**
4. **Configure:**
   - **Name**: `blackbook-blockchain`
   - **Region**: Oregon (or closest to you)
   - **Branch**: `master`
   - **Root Directory**: `blackBook`
   - **Environment**: `Docker`
   - **Dockerfile Path**: `./Dockerfile`
   - **Instance Type**: Free (or Starter for better performance)
5. **Add Environment Variable:**
   - `PORT` = `3000`
   - `RUST_LOG` = `info`
6. **Click "Create Web Service"**

#### What Render Deploys

✅ **Rust Blockchain Backend** - All 40+ API endpoints  
✅ **HTML Frontend** - Your live dashboard at the root URL  
✅ **Auto-SSL** - Automatic HTTPS certificate  
✅ **Auto-Deploy** - Updates on every git push  
✅ **Health Monitoring** - Uses `/health` endpoint  

#### Access Your Deployment

Once deployed (takes 5-10 minutes for first build):
- 🌐 **Frontend**: `https://your-app-name.onrender.com/`
- 🔗 **API**: `https://your-app-name.onrender.com/health`
- 📊 **Markets**: `https://your-app-name.onrender.com/markets`

#### Render Free Tier Notes

- ✅ **Included**: SSL, custom domains, auto-deploy
- ⚠️ **Limitation**: Service spins down after 15 min of inactivity (first request takes ~30s)
- 💡 **Solution**: Upgrade to Starter plan ($7/mo) for always-on service

---

### Deploy to Railway.app

1. Create a new project on [Railway](https://railway.app/)
2. Connect your GitHub repository
3. Set root directory to `blackBook`
4. Railway will auto-detect the Dockerfile
5. Set environment variable: `PORT=3000`
6. Deploy - Railway provides a public URL automatically

### Deploy to Fly.io

1. Install Fly CLI: `curl -L https://fly.io/install.sh | sh`
2. Login: `fly auth login`
3. Create `fly.toml`:
   ```toml
   app = "blackbook-blockchain"
   
   [build]
     dockerfile = "Dockerfile"
   
   [[services]]
     internal_port = 3000
     protocol = "tcp"
   
     [[services.ports]]
       port = 80
       handlers = ["http"]
     
     [[services.ports]]
       port = 443
       handlers = ["tls", "http"]
   ```
4. Deploy: `fly deploy`
5. Access: `https://blackbook-blockchain.fly.dev`

---

## 🔧 Configuration

### Port Configuration

The server listens on `0.0.0.0` to accept external connections. You can change the port:

**Docker**: Edit `docker-compose.yml`
```yaml
ports:
  - "8080:8080"  # Change both sides
environment:
  - PORT=8080
```

**Manual**: Use environment variable
```bash
PORT=8080 ./target/release/blackbook-prediction-market
```

### Database Persistence

The blockchain uses Sled database which stores data in:
- Docker: `/app/data` (persisted via volume)
- Manual: `./data` in the current directory

To reset the blockchain, delete the data directory.

---

## 🧪 Testing Your Deployment

### 1. Health Check
```bash
curl http://localhost:3000/health
```

Expected response:
```json
{
  "status": "healthy",
  "service": "BlackBook Prediction Market",
  "version": "1.0.0"
}
```

### 2. Test Frontend
Open in browser: http://localhost:3000/

You should see the BlackBook L1 Live Blockchain Ledger dashboard.

### 3. Test API
```bash
# Get all markets
curl http://localhost:3000/markets

# Get blockchain accounts
curl http://localhost:3000/accounts

# Get transaction statistics
curl http://localhost:3000/stats
```

---

## 📊 Monitoring

### View Logs (Docker)
```bash
# Follow logs in real-time
docker-compose logs -f

# View last 100 lines
docker-compose logs --tail=100
```

### View Logs (Manual)
Logs are output to stdout. Use systemd or a process manager like PM2 for log management.

---

## 🔒 Security Considerations

### For Production Deployments:

1. **Use HTTPS**: Put the application behind a reverse proxy (Nginx, Caddy) with SSL
2. **Firewall**: Only expose port 3000 (or use a reverse proxy on 80/443)
3. **Rate Limiting**: Implement rate limiting for API endpoints
4. **Admin Endpoints**: Restrict access to `/admin/*` endpoints
5. **Environment Variables**: Store sensitive config in env vars, not in code
6. **Database Backups**: Regularly backup the `/app/data` directory

### Example Nginx Reverse Proxy

```nginx
server {
    listen 80;
    server_name yourdomain.com;

    location / {
        proxy_pass http://localhost:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

---

## 🆘 Troubleshooting

### Port Already in Use
```bash
# Find process using port 3000
lsof -i :3000

# Kill the process
kill -9 <PID>
```

### Docker Build Fails
```bash
# Clear Docker cache and rebuild
docker-compose down
docker system prune -a
docker-compose up --build
```

### Can't Access from External Network
- Ensure you're using `0.0.0.0` (not `127.0.0.1`)
- Check firewall rules
- Verify security group settings (cloud deployments)

### Frontend Not Loading
- Verify `index.html` is in the root directory
- Check that `serve_live_ledger()` function is properly configured
- Ensure Docker COPY command includes `index.html`

---

## 🎯 Production Checklist

- [ ] Application builds successfully
- [ ] Docker image builds without errors
- [ ] Health check endpoint responds
- [ ] Frontend loads at root URL
- [ ] API endpoints are accessible
- [ ] Database persists between restarts
- [ ] Logs are being captured
- [ ] Firewall rules configured
- [ ] HTTPS configured (if production)
- [ ] Monitoring/alerting setup
- [ ] Backup strategy in place

---

## 📚 Additional Resources

- **Repository**: https://github.com/aMarketology/blackBook-ledger-rust
- **API Documentation**: Check `/health` endpoint for server info
- **Frontend**: Accessible at root URL (`/`)
- **Support**: Create an issue on GitHub

---

## 🚦 Quick Commands Reference

```bash
# Development
cargo run

# Build for production
cargo build --release

# Docker - Start
docker-compose up -d --build

# Docker - Stop
docker-compose down

# Docker - View logs
docker-compose logs -f

# Docker - Restart
docker-compose restart

# Health check
curl http://localhost:3000/health

# Test API
curl http://localhost:3000/markets
```

---

**Your blockchain is now ready to go live!** 🎉

Both your blockchain backend and HTML frontend will be accessible on `http://0.0.0.0:3000` when deployed.
