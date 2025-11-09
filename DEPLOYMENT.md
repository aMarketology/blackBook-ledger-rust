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
- Docker installed ([Get Docker](https://docs.docker.com/get-docker/))
- Docker Compose installed (usually bundled with Docker Desktop)

### Quick Start - Single Command

```bash
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

## 🛠️ Manual Deployment (Without Docker)

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

### Deploy to Railway.app

1. Create a new project on [Railway](https://railway.app/)
2. Connect your GitHub repository
3. Railway will auto-detect the Dockerfile
4. Set environment variable: `PORT=3000`
5. Deploy - Railway provides a public URL automatically

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
