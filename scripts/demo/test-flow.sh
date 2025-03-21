#!/bin/bash
# Script to test the full L402 payment flow

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
SKIP_PAYMENT=false

# Parse command line arguments
while getopts ":h:p:s" opt; do
  case ${opt} in
    h )
      HOST=$OPTARG
      ;;
    p )
      PORT=$OPTARG
      ;;
    s )
      SKIP_PAYMENT=true
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

BASE_URL="http://$HOST:$PORT"
AUTH_TOKEN=""
PAYMENT_REQUEST_URL=""
PAYMENT_CONTEXT_TOKEN=""
OFFER_ID=""

echo -e "${BLUE}Testing L402 Payment Flow on $BASE_URL${NC}"
echo "----------------------------------------"

# Check if the server is running
echo -e "${BLUE}Step 1: Checking if the server is running...${NC}"
if ! curl -s "$BASE_URL" > /dev/null; then
  echo -e "${RED}Error: Server not running at $BASE_URL${NC}"
  echo "Please start the server first with:"
  echo "  cargo run"
  exit 1
fi
echo -e "${GREEN}✓ Server is running${NC}"
echo

# Step 1: Create a new user (signup)
echo -e "${BLUE}Step 2: Creating a new user...${NC}"
SIGNUP_RESPONSE=$(curl -s "$BASE_URL/signup")
AUTH_TOKEN=$(echo $SIGNUP_RESPONSE | jq -r '.id')
CREDITS=$(echo $SIGNUP_RESPONSE | jq -r '.credits')

if [ -z "$AUTH_TOKEN" ] || [ "$AUTH_TOKEN" == "null" ]; then
  echo -e "${RED}Error: Failed to create user${NC}"
  echo "Response: $SIGNUP_RESPONSE"
  exit 1
fi

echo -e "${GREEN}✓ User created successfully${NC}"
echo -e "  Auth Token: ${YELLOW}$AUTH_TOKEN${NC}"
echo -e "  Initial Credits: ${YELLOW}$CREDITS${NC}"
echo

# Step 2: Get user info
echo -e "${BLUE}Step 3: Getting user info...${NC}"
USER_INFO=$(curl -s -H "Authorization: Bearer $AUTH_TOKEN" "$BASE_URL/info")
CREDITS=$(echo $USER_INFO | jq -r '.credits')

echo -e "${GREEN}✓ User info retrieved${NC}"
echo -e "  Credits: ${YELLOW}$CREDITS${NC}"
echo

# Step 3: Use up any free credits by calling the API
if [ "$CREDITS" -gt 0 ]; then
  echo -e "${BLUE}Step 4: Using up free credits...${NC}"
  for (( i=1; i<=$CREDITS; i++ )); do
    echo -e "  Making API call $i of $CREDITS..."
    RESPONSE=$(curl -s -H "Authorization: Bearer $AUTH_TOKEN" "$BASE_URL/latest-block")
    echo -e "  - Got response: $(echo $RESPONSE | jq -r '.' | head -1)"
    
    # Check remaining credits
    REMAINING=$(curl -s -H "Authorization: Bearer $AUTH_TOKEN" "$BASE_URL/info" | jq -r '.credits')
    echo -e "  - Remaining credits: ${YELLOW}$REMAINING${NC}"
  done
  echo -e "${GREEN}✓ Used all free credits${NC}"
  echo
fi

# Step 4: Try to access the API with no credits (should get 402)
echo -e "${BLUE}Step 5: Trying to access API with no credits...${NC}"
RESPONSE=$(curl -s -w "%{http_code}" -H "Authorization: Bearer $AUTH_TOKEN" "$BASE_URL/latest-block")
HTTP_CODE=$(echo "$RESPONSE" | tail -1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" != "402" ]; then
  echo -e "${RED}Error: Expected HTTP 402, got $HTTP_CODE${NC}"
  echo "Response: $BODY"
  exit 1
fi

echo -e "${GREEN}✓ Received 402 Payment Required as expected${NC}"
echo

# Step 5: Parse the 402 response to get payment options
echo -e "${BLUE}Step 6: Parsing payment options...${NC}"
PAYMENT_CONTEXT_TOKEN=$(echo $BODY | jq -r '.payment_context_token')
PAYMENT_REQUEST_URL=$(echo $BODY | jq -r '.payment_request_url')
OFFER_ID=$(echo $BODY | jq -r '.offers[0].offer_id')
OFFER_TITLE=$(echo $BODY | jq -r '.offers[0].title')
OFFER_AMOUNT=$(echo $BODY | jq -r '.offers[0].amount')
OFFER_CURRENCY=$(echo $BODY | jq -r '.offers[0].currency')

echo -e "${GREEN}✓ Parsed payment details${NC}"
echo -e "  Payment Context Token: ${YELLOW}$PAYMENT_CONTEXT_TOKEN${NC}"
echo -e "  Payment Request URL: ${YELLOW}$PAYMENT_REQUEST_URL${NC}"
echo -e "  Offer: ${YELLOW}$OFFER_TITLE - $OFFER_AMOUNT $OFFER_CURRENCY${NC}"
echo -e "  Offer ID: ${YELLOW}$OFFER_ID${NC}"
echo

# Skip payment if requested
if [ "$SKIP_PAYMENT" = true ]; then
  echo -e "${YELLOW}Skipping payment step as requested${NC}"
  echo -e "${BLUE}To complete the flow manually:${NC}"
  echo -e "1. Make a payment request with:"
  echo -e "   curl -X POST -H \"Content-Type: application/json\" \\"
  echo -e "        -d '{\"offer_id\":\"$OFFER_ID\",\"payment_method\":\"lightning\",\"payment_context_token\":\"$PAYMENT_CONTEXT_TOKEN\"}' \\"
  echo -e "        $PAYMENT_REQUEST_URL"
  echo -e "2. Pay the Lightning invoice"
  echo -e "3. Check your credits with:"
  echo -e "   curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/info"
  exit 0
fi

# Step 6: Request payment invoice (choose Lightning)
echo -e "${BLUE}Step 7: Requesting Lightning payment invoice...${NC}"
PAYMENT_REQUEST=$(curl -s -X POST \
  -H "Content-Type: application/json" \
  -d "{\"offer_id\":\"$OFFER_ID\",\"payment_method\":\"lightning\",\"payment_context_token\":\"$PAYMENT_CONTEXT_TOKEN\"}" \
  "$PAYMENT_REQUEST_URL")

LIGHTNING_INVOICE=$(echo $PAYMENT_REQUEST | jq -r '.payment_request.lightning_invoice')
if [ -z "$LIGHTNING_INVOICE" ] || [ "$LIGHTNING_INVOICE" == "null" ]; then
  echo -e "${RED}Error: Failed to get Lightning invoice${NC}"
  echo "Response: $PAYMENT_REQUEST"
  exit 1
fi

echo -e "${GREEN}✓ Lightning invoice generated${NC}"
echo -e "  Lightning Invoice: ${YELLOW}$LIGHTNING_INVOICE${NC}"
echo

# Step 7: Display QR code for payment (if qrencode is installed)
if command -v qrencode &> /dev/null; then
  echo -e "${BLUE}Step 8: Generating QR code for payment...${NC}"
  echo "$LIGHTNING_INVOICE" | qrencode -t ANSIUTF8
  echo -e "${GREEN}✓ Scan this QR code with your Lightning wallet to pay${NC}"
else
  echo -e "${YELLOW}QR code generation skipped (qrencode not installed)${NC}"
  echo -e "${YELLOW}To install: brew install qrencode (Mac) or apt-get install qrencode (Linux)${NC}"
fi
echo

# Step 8: Instructions for completing the flow
echo -e "${BLUE}Next Steps:${NC}"
echo -e "1. Pay the Lightning invoice using a Lightning wallet"
echo -e "2. Once paid, the webhook will automatically add credits to your account"
echo -e "3. Check your updated credit balance with:"
echo -e "   ${YELLOW}curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/info${NC}"
echo -e "4. Try the API again with:"
echo -e "   ${YELLOW}curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/latest-block${NC}"
echo
echo -e "${GREEN}For your convenience, here is your auth token: ${YELLOW}$AUTH_TOKEN${NC}${GREEN} (save it for later)${NC}"