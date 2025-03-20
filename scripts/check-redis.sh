#!/bin/bash
# Script to check if Redis is available before running tests

set -eu

echo "Checking Redis connection..."

# Get Redis URL from environment or use default
REDIS_URL=${REDIS_URL:-"redis://localhost:6379"}

# Extract host and port from URL
if [[ $REDIS_URL =~ redis://([^:]+):([0-9]+) ]]; then
  REDIS_HOST=${BASH_REMATCH[1]}
  REDIS_PORT=${BASH_REMATCH[2]}
else
  echo "Error: Could not parse Redis URL '$REDIS_URL'"
  exit 1
fi

# Check if Redis is running using redis-cli ping
timeout=30
count=0
echo "Waiting for Redis at $REDIS_HOST:$REDIS_PORT..."

while ! redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping > /dev/null 2>&1; do
  count=$((count+1))
  if [ $count -ge $timeout ]; then
    echo "Error: Redis not available after $timeout seconds"
    exit 1
  fi
  echo "Waiting for Redis... ($count/$timeout)"
  sleep 1
done

echo "Redis is available!"
exit 0