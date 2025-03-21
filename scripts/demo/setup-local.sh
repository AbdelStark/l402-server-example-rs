#!/bin/bash
# Script to set up a local development environment for L402 server

set -e

# Colors for better output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}Setting up L402 Server Environment${NC}"
echo "----------------------------------------"

# Check if Docker is installed
echo -e "${BLUE}Checking Docker installation...${NC}"
if ! command -v docker &> /dev/null; then
  echo -e "${RED}Error: Docker is not installed${NC}"
  echo "Please install Docker first: https://docs.docker.com/get-docker/"
  exit 1
fi

if ! command -v docker-compose &> /dev/null; then
  echo -e "${RED}Error: docker-compose is not installed${NC}"
  echo "Please install docker-compose: https://docs.docker.com/compose/install/"
  exit 1
fi
echo -e "${GREEN}✓ Docker installed${NC}"
echo

# Create .env file if it doesn't exist
echo -e "${BLUE}Setting up environment variables...${NC}"
if [ ! -f ./.env ]; then
  cat > ./.env << EOL
# Server configuration
HOST=0.0.0.0
PORT=8080

# Redis configuration
REDIS_URL=redis://localhost:6379

# LNBits configuration
LNBITS_URL=https://legend.lnbits.com
LNBITS_API_KEY=your_lnbits_api_key
LNBITS_WEBHOOK_KEY=your_lnbits_webhook_key
LNBITS_INVOICE_READ_KEY=your_lnbits_invoice_read_key
EOL
  echo -e "${GREEN}✓ Created default .env file${NC}"
  echo -e "${YELLOW}Please edit .env file to add your LNBits API keys${NC}"
else
  echo -e "${GREEN}✓ .env file already exists${NC}"
fi
echo

# Start Redis with Docker
echo -e "${BLUE}Starting Redis...${NC}"
if docker ps | grep -q "l402-redis"; then
  echo -e "${GREEN}✓ Redis is already running${NC}"
else
  docker-compose up -d redis
  echo -e "${GREEN}✓ Redis started${NC}"
fi
echo

# Verify Redis is working
echo -e "${BLUE}Verifying Redis connection...${NC}"
if ! curl -s localhost:6379 &> /dev/null; then
  echo -e "${YELLOW}⚠️  Redis connection check failed, but container may still be starting${NC}"
  echo -e "You can verify manually with: docker logs l402-redis"
else
  echo -e "${GREEN}✓ Redis is responding${NC}"
fi
echo

# Check if LNBits is configured
echo -e "${BLUE}Checking LNBits configuration...${NC}"
LNBITS_API_KEY=$(grep LNBITS_API_KEY .env | cut -d '=' -f2)
if [ "$LNBITS_API_KEY" = "your_lnbits_api_key" ]; then
  echo -e "${YELLOW}⚠️  LNBits API keys not configured in .env${NC}"
  echo -e "${YELLOW}To use Lightning payments, you need to:${NC}"
  echo -e "1. Create an account at https://legend.lnbits.com/"
  echo -e "2. Create a new wallet"
  echo -e "3. Copy the API keys (Admin key, Invoice/read key)"
  echo -e "4. Update the .env file with these keys"
else
  echo -e "${GREEN}✓ LNBits appears to be configured${NC}"
fi
echo

# Provide instructions
echo -e "${BLUE}Next Steps:${NC}"
echo -e "1. Start the server with: ${YELLOW}cargo run${NC}"
echo -e "2. Test the flow with: ${YELLOW}./scripts/demo/test-flow.sh${NC}"
echo
echo -e "${GREEN}Setup completed!${NC}"