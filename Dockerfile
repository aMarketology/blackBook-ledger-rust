# Multi-stage Dockerfile for BlackBook Blockchain + Frontend
# Stage 1: Build the Rust binary
FROM rust:1.75-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Stage 2: Create minimal runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary from builder stage
COPY --from=builder /app/target/release/blackbook-prediction-market /app/blackbook

# Copy the HTML frontend
COPY index.html /app/index.html

# Create data directory for sled database
RUN mkdir -p /app/data

# Expose the port
EXPOSE 3000

# Set environment variables
ENV RUST_LOG=info
ENV PORT=3000

# Run the blockchain server
CMD ["/app/blackbook"]
