#!/bin/bash
# Script to run CI checks locally

set -e

echo "🔍 Running CI checks locally..."

# Check if Redis is available
if [ -f ./scripts/check-redis.sh ]; then
  echo "📋 Checking Redis availability..."
  ./scripts/check-redis.sh
fi

# Format check
echo "📋 Checking code formatting..."
cargo fmt --all -- --check

# Clippy
echo "📋 Running Clippy lints..."
cargo clippy -- -D warnings

# Compilation check
echo "📋 Checking compilation..."
cargo check --all-features

# Run tests
echo "📋 Running tests..."
cargo test --all-features

# Security audit (if cargo-audit is installed)
if command -v cargo-audit &> /dev/null; then
  echo "📋 Running security audit..."
  cargo audit
else
  echo "⚠️  cargo-audit not installed, skipping security checks"
  echo "   To install: cargo install cargo-audit"
fi

echo "✅ All checks passed!"