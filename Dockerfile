FROM rust:1.76-slim as builder

WORKDIR /app

# Copy the manifest files
COPY Cargo.toml ./

# Create a dummy source file to build dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies (this will be cached)
RUN cargo build --release

# Remove the dummy source file
RUN rm -rf src

# Copy the actual source code
COPY src ./src

# Force a rebuild with the actual source code
RUN touch src/main.rs

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install dependencies for SSL/TLS
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/l402-server-example-rs /usr/local/bin/l402-server

# Set the entrypoint
ENTRYPOINT ["l402-server"]

# Expose the default port
EXPOSE 8080