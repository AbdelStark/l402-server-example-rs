#!/bin/bash
# Script to simulate a successful Lightning payment 

set -e

# Colors for better output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Default values
HOST="localhost"
PORT="8080"
USER_ID=""
PAYMENT_HASH=""

# Parse command line arguments
while getopts ":h:p:u:i:" opt; do
  case ${opt} in
    h )
      HOST=$OPTARG
      ;;
    p )
      PORT=$OPTARG
      ;;
    u )
      USER_ID=$OPTARG
      ;;
    i )
      PAYMENT_HASH=$OPTARG
      ;;
    \? )
      echo "Invalid option: $OPTARG" 1>&2
      exit 1
      ;;
    : )
      echo "Invalid option: $OPTARG requires an argument" 1>&2
      exit 1
      ;;
  esac
done

# Check if required parameters are provided
if [ -z "$USER_ID" ] || [ -z "$PAYMENT_HASH" ]; then
  echo -e "${RED}Error: Missing required parameters${NC}"
  echo "Usage: $0 -u USER_ID -i PAYMENT_HASH [-h HOST] [-p PORT]"
  echo ""
  echo "Required:"
  echo "  -u USER_ID       The user ID to credit"
  echo "  -i PAYMENT_HASH  The payment hash of the Lightning invoice"
  echo ""
  echo "Optional:"
  echo "  -h HOST          Server host (default: localhost)"
  echo "  -p PORT          Server port (default: 8080)"
  exit 1
fi

BASE_URL="http://$HOST:$PORT"

echo -e "${BLUE}Simulating a Lightning Payment Webhook${NC}"
echo "----------------------------------------"

# Step 1: Check if the server is running
echo -e "${BLUE}Checking if the server is running...${NC}"
if ! curl -s "$BASE_URL" > /dev/null; then
  echo -e "${RED}Error: Server not running at $BASE_URL${NC}"
  echo "Please start the server first with:"
  echo "  cargo run"
  exit 1
fi
echo -e "${GREEN}✓ Server is running${NC}"
echo

# Step 2: Get initial user info to check credits
echo -e "${BLUE}Checking initial user credits...${NC}"
USER_INFO=$(curl -s -H "Authorization: Bearer $USER_ID" "$BASE_URL/info")
INITIAL_CREDITS=$(echo $USER_INFO | jq -r '.credits')

if [ -z "$INITIAL_CREDITS" ] || [ "$INITIAL_CREDITS" == "null" ]; then
  echo -e "${RED}Error: User not found or invalid response${NC}"
  echo "Response: $USER_INFO"
  exit 1
fi

echo -e "${GREEN}✓ Initial credits: ${YELLOW}$INITIAL_CREDITS${NC}${GREEN} for user ${YELLOW}$USER_ID${NC}"
echo

# Step 3: Create a webhook payload
echo -e "${BLUE}Creating webhook payload...${NC}"
# This is a simplified version of what LNBits sends in a webhook
WEBHOOK_PAYLOAD=$(cat <<EOF
{
  "payment_hash": "$PAYMENT_HASH",
  "payment_request": "lnbc...",
  "amount": 1000,
  "memo": "Payment for L402 credits",
  "time": $(date +%s),
  "webhook_status": "SUCCESS",
  "webhook_url": "http://$HOST:$PORT/webhook/lightning",
  "checking_id": "$PAYMENT_HASH",
  "user_id": "$USER_ID" 
}
EOF
)

echo -e "${GREEN}✓ Created webhook payload${NC}"
echo -e "  Payload: $(echo $WEBHOOK_PAYLOAD | jq -c '.')"
echo

# Step 4: Send the webhook
echo -e "${BLUE}Sending webhook to simulate payment...${NC}"
WEBHOOK_RESPONSE=$(curl -s -X POST \
  -H "Content-Type: application/json" \
  -H "X-Lightning-Signature: mock-signature" \
  -d "$WEBHOOK_PAYLOAD" \
  "$BASE_URL/webhook/lightning")

echo -e "${GREEN}✓ Webhook sent${NC}"
echo -e "  Response: ${YELLOW}$WEBHOOK_RESPONSE${NC}"
echo

# Step 5: Check if credits were added
echo -e "${BLUE}Checking if credits were added...${NC}"
sleep 2  # Wait a moment for processing
USER_INFO=$(curl -s -H "Authorization: Bearer $USER_ID" "$BASE_URL/info")
NEW_CREDITS=$(echo $USER_INFO | jq -r '.credits')

if [ "$NEW_CREDITS" -gt "$INITIAL_CREDITS" ]; then
  echo -e "${GREEN}✓ Payment simulation successful!${NC}"
  echo -e "  Credits before: ${YELLOW}$INITIAL_CREDITS${NC}"
  echo -e "  Credits after: ${YELLOW}$NEW_CREDITS${NC}"
  echo -e "  Credits added: ${YELLOW}$((NEW_CREDITS - INITIAL_CREDITS))${NC}"
else
  echo -e "${RED}✕ Payment simulation failed. Credits not updated.${NC}"
  echo -e "  Credits before: ${YELLOW}$INITIAL_CREDITS${NC}"
  echo -e "  Credits after: ${YELLOW}$NEW_CREDITS${NC}"
fi
echo

# Step 6: Test API access with new credits
echo -e "${BLUE}Testing API access with new credits...${NC}"
API_RESPONSE=$(curl -s -H "Authorization: Bearer $USER_ID" "$BASE_URL/latest-block")
echo -e "${GREEN}✓ API access test${NC}"
echo -e "  Response: $(echo $API_RESPONSE | jq -c '.' | head -1)"
echo

echo -e "${GREEN}Simulation completed!${NC}"