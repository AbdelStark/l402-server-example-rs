FROM rust:1.85-slim as builder

WORKDIR /app

# Install OpenSSL dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock* ./

# Copy the actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install dependencies for SSL/TLS
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/l402-server-example-rs /usr/local/bin/l402-server

# Set the entrypoint
ENTRYPOINT ["l402-server"]

# Expose the default port
EXPOSE 8080