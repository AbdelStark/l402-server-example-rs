#!/bin/bash
# Script to test the full L402 payment flow with actual Lightning payments

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
BASE_URL=""
AUTH_TOKEN=""

# Helper functions
check_dependencies() {
    # Check if jq is installed
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}Error: jq is not installed${NC}"
        echo "Please install jq first:"
        echo "  brew install jq (Mac)"
        echo "  apt-get install jq (Linux)"
        exit 1
    fi

    # Check if curl is installed
    if ! command -v curl &> /dev/null; then
        echo -e "${RED}Error: curl is not installed${NC}"
        echo "Please install curl first"
        exit 1
    fi
}

check_server() {
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/info")
    
    if [ "$status_code" = "404" ] || [ "$status_code" = "200" ] || [ "$status_code" = "401" ]; then
        return 0
    else
        return 1
    fi
}

call_api() {
    local endpoint=$1
    local auth_header=$2
    local method=${3:-GET}
    local data=$4

    local response
    local curl_cmd

    # Build curl command
    if [ -n "$auth_header" ]; then
        if [ "$method" = "POST" ] && [ -n "$data" ]; then
            curl_cmd="curl -s -X $method -H 'Authorization: $auth_header' -H 'Content-Type: application/json' -d '$data'"
        else
            curl_cmd="curl -s -X $method -H 'Authorization: $auth_header'"
        fi
    else
        if [ "$method" = "POST" ] && [ -n "$data" ]; then
            curl_cmd="curl -s -X $method -H 'Content-Type: application/json' -d '$data'"
        else
            curl_cmd="curl -s -X $method"
        fi
    fi

    # Execute curl command and get response
    response=$(eval "$curl_cmd '$BASE_URL$endpoint'")
    local status=$?

    # Check if curl command succeeded
    if [ $status -ne 0 ]; then
        echo -e "${RED}Error: Failed to make request to $endpoint${NC}"
        echo "Curl command: $curl_cmd '$BASE_URL$endpoint'"
        echo "Exit status: $status"
        exit 1
    fi

    # Check if response is empty
    if [ -z "$response" ]; then
        echo -e "${RED}Error: Empty response from server${NC}"
        echo "Endpoint: $endpoint"
        echo "Curl command: $curl_cmd '$BASE_URL$endpoint'"
        exit 1
    fi

    # Check if response is valid JSON only if we expect JSON
    if [[ "$endpoint" =~ ^/(signup|info|block|l402/payment-request)$ ]]; then
        if ! echo "$response" | jq -e . >/dev/null 2>&1; then
            echo -e "${RED}Error: Invalid JSON response${NC}"
            echo "Endpoint: $endpoint"
            echo "Raw response:"
            echo "$response"
            echo
            echo "Curl command: $curl_cmd '$BASE_URL$endpoint'"
            exit 1
        fi
    fi

    echo "$response"
}

get_status_code() {
    local endpoint=$1
    local auth_header=$2
    local method=${3:-GET}
    local data=$4

    if [ -n "$auth_header" ]; then
        if [ "$method" = "POST" ] && [ -n "$data" ]; then
            curl -s -o /dev/null -w "%{http_code}" \
                -X "$method" \
                -H "Authorization: $auth_header" \
                -H "Content-Type: application/json" \
                -d "$data" \
                "$BASE_URL$endpoint"
        else
            curl -s -o /dev/null -w "%{http_code}" \
                -X "$method" \
                -H "Authorization: $auth_header" \
                "$BASE_URL$endpoint"
        fi
    else
        if [ "$method" = "POST" ] && [ -n "$data" ]; then
            curl -s -o /dev/null -w "%{http_code}" \
                -X "$method" \
                -H "Content-Type: application/json" \
                -d "$data" \
                "$BASE_URL$endpoint"
        else
            curl -s -o /dev/null -w "%{http_code}" \
                -X "$method" \
                "$BASE_URL$endpoint"
        fi
    fi
}

signup() {
    echo -e "${BLUE}Creating a new user...${NC}"
    local response=$(call_api "/signup")
    
    # Make sure we can parse the response as JSON
    if echo "$response" | jq -e '.' >/dev/null 2>&1; then
        local user_id=$(echo "$response" | jq -r '.id // empty')
        local credits=$(echo "$response" | jq -r '.credits // "1"')

        if [ -z "$user_id" ] || [ "$user_id" = "null" ]; then
            echo -e "${RED}Error: Failed to create user${NC}"
            echo "Response: $response"
            exit 1
        fi
    else
        echo -e "${RED}Error: Could not parse JSON from signup response${NC}"
        echo "Raw response: $response"
        exit 1
    fi

    echo -e "${GREEN}✓ User created successfully${NC}"
    echo -e "  Auth Token: ${YELLOW}$user_id${NC}"
    echo -e "  Initial Credits: ${YELLOW}$credits${NC}"
    echo

    # Return the user ID
    echo "$user_id"
}

# Get user info
# Args:
#   $1: auth token
#   $2: optional flag to skip logging
# Returns:
#   user info JSON
get_user_info() {
    local auth_token=$1
    local skip_logging=$2

    if [ -z "$skip_logging" ]; then
        echo -e "${BLUE}Getting user info...${NC}"
    fi

    local response=$(call_api "/info" "Bearer $auth_token")
    
    # Try to parse the response as JSON to get credits
    local credits="unknown"
    if echo "$response" | jq -e '.' >/dev/null 2>&1; then
        credits=$(echo "$response" | jq -r '.credits // "unknown"')
    else
        echo -e "${YELLOW}Warning: Could not parse JSON from user info response${NC}"
        # Try to extract credits if it's in the response somewhere
        if [[ "$response" =~ \"credits\":([0-9]+) ]]; then
            credits="${BASH_REMATCH[1]}"
        fi
    fi

    if [ -z "$skip_logging" ]; then
        echo -e "${GREEN}✓ User info retrieved${NC}"
        echo -e "  Credits: ${YELLOW}$credits${NC}"
        echo
    fi

    echo "$response"
}

get_block() {
    local auth_token=$1
    echo -e "${BLUE}Getting block info...${NC}"
    
    # First get the response body
    local response=$(call_api "/block" "Bearer $auth_token")
    # Then get the status code separately
    local status_code=$(get_status_code "/block" "Bearer $auth_token")

    if [ "$status_code" = "402" ]; then
        echo -e "${YELLOW}Payment required (402)${NC}"
        local payment_context_token=$(echo "$response" | jq -r '.payment_context_token // "N/A"')
        local payment_request_url=$(echo "$response" | jq -r '.payment_request_url // "N/A"')
        
        # Handle case where jq might error on parsing the nested offers array
        local offer_info=$(echo "$response" | jq -r '.offers[0] // empty')
        local offer_id="N/A"
        local offer_title="N/A"
        local offer_amount="N/A"
        local offer_currency="N/A"
        
        if [ -n "$offer_info" ]; then
            offer_id=$(echo "$offer_info" | jq -r '.id // "N/A"')
            offer_title=$(echo "$offer_info" | jq -r '.title // "N/A"')
            offer_amount=$(echo "$offer_info" | jq -r '.amount // "N/A"')
            offer_currency=$(echo "$offer_info" | jq -r '.currency // "N/A"')
        fi

        echo -e "  Payment Context Token: ${YELLOW}$payment_context_token${NC}"
        echo -e "  Payment Request URL: ${YELLOW}$payment_request_url${NC}"
        echo -e "  Offer: ${YELLOW}$offer_title - $offer_amount $offer_currency${NC}"
        echo -e "  Offer ID: ${YELLOW}$offer_id${NC}"
    elif [ "$status_code" = "200" ]; then
        echo -e "${GREEN}✓ Block info retrieved${NC}"
        if echo "$response" | jq -e '.' >/dev/null 2>&1; then
            echo -e "  Response: $(echo "$response" | jq -c '.' | head -1)"
        else
            echo -e "  Response: $response"
        fi
    else
        echo -e "${RED}Error: Unexpected status code $status_code${NC}"
        echo "Response: $response"
    fi
    echo

    # Return both status code and response
    echo "STATUS:$status_code"
    echo "RESPONSE:$response"
}

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

# Check dependencies first
check_dependencies

echo -e "${BLUE}Testing L402 Payment Flow on $BASE_URL${NC}"
echo "----------------------------------------"

# Check if the server is running
echo -e "${BLUE}Step 1: Checking if the server is running...${NC}"
if ! check_server; then
    echo -e "${RED}Error: Server not running or not responding correctly at $BASE_URL${NC}"
    echo "Please start the server first with:"
    echo "  cargo run"
    exit 1
fi
echo -e "${GREEN}✓ Server is running${NC}"
echo

# Step 1: Create a new user
SIGNUP_RESPONSE=$(signup)

# Extract just the UUID token if present
if [[ "$SIGNUP_RESPONSE" =~ ([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}) ]]; then
    AUTH_TOKEN="${BASH_REMATCH[1]}"
    echo -e "${GREEN}✓ Extracted clean user ID: ${YELLOW}$AUTH_TOKEN${NC}"
else
    # Try to use the response as-is
    AUTH_TOKEN="$SIGNUP_RESPONSE"
    
    # Ensure AUTH_TOKEN is set to something usable
    if [ -z "$AUTH_TOKEN" ] || [[ "$AUTH_TOKEN" == *"DEBUG:"* ]]; then
        echo -e "${RED}Error: Failed to get clean auth token from signup${NC}"
        exit 1
    fi
fi

# Step 2: Get user info
USER_INFO=$(get_user_info "$AUTH_TOKEN")

# Try to extract credits safely
if echo "$USER_INFO" | jq -e '.' >/dev/null 2>&1; then
    CREDITS=$(echo "$USER_INFO" | jq -r '.credits // "0"')
else
    # Try regex extraction as a fallback
    if [[ "$USER_INFO" =~ \"credits\":([0-9]+) ]]; then
        CREDITS="${BASH_REMATCH[1]}"
    else
        CREDITS="0"
        echo -e "${YELLOW}Warning: Could not extract credits from user info response${NC}"
    fi
fi

# Step 3: Use up any free credits by calling the API
if [[ "$CREDITS" =~ ^[0-9]+$ ]] && [ "$CREDITS" -gt 0 ]; then
    echo -e "${BLUE}Step 3: Using up free credits...${NC}"
    for (( i=1; i<=$CREDITS; i++ )); do
        echo -e "  Making API call $i of $CREDITS..."
        BLOCK_RESULT=$(get_block "$AUTH_TOKEN")
        
        # Get remaining credits, handling potential JSON parsing issues
        USER_INFO_RESP=$(get_user_info "$AUTH_TOKEN")
        if echo "$USER_INFO_RESP" | jq -e '.' >/dev/null 2>&1; then
            REMAINING=$(echo "$USER_INFO_RESP" | jq -r '.credits // "0"')
        else
            # Try to extract credits with regex if JSON parsing fails
            if [[ "$USER_INFO_RESP" =~ \"credits\":([0-9]+) ]]; then
                REMAINING="${BASH_REMATCH[1]}"
            else
                REMAINING="unknown"
            fi
        fi
        
        echo -e "  - Remaining credits: ${YELLOW}$REMAINING${NC}"
    done
    echo -e "${GREEN}✓ Used all free credits${NC}"
    echo
else
    echo -e "${YELLOW}Skipping credit usage step - no credits available or invalid credit count: $CREDITS${NC}"
    echo
fi

# Step 4: Try to access the API with no credits (should get 402)
echo -e "${BLUE}Step 4: Trying to access API with no credits...${NC}"
BLOCK_RESULT=$(get_block "$AUTH_TOKEN")
STATUS_CODE=$(echo "$BLOCK_RESULT" | grep "^STATUS:" | cut -d':' -f2)
# Extract RESPONSE and properly format it for JSON parsing
RESPONSE_LINE=$(echo "$BLOCK_RESULT" | grep "^RESPONSE:")
RESPONSE=${RESPONSE_LINE#RESPONSE:}

if [ "$STATUS_CODE" != "402" ]; then
    echo -e "${RED}Error: Expected HTTP 402, got $STATUS_CODE${NC}"
    echo "Response: $RESPONSE"
    exit 1
fi

echo -e "${GREEN}✓ Received 402 Payment Required as expected${NC}"
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

# Extract payment details from the 402 response
if echo "$RESPONSE" | jq -e '.' >/dev/null 2>&1; then
    # If response is valid JSON, extract fields normally
    PAYMENT_CONTEXT_TOKEN=$(echo "$RESPONSE" | jq -r '.payment_context_token // empty')
    PAYMENT_REQUEST_URL=$(echo "$RESPONSE" | jq -r '.payment_request_url // empty')
    OFFER_ID=$(echo "$RESPONSE" | jq -r '.offers[0].id // empty')
else
    # If parsing fails, log error and exit
    echo -e "${RED}Error: Could not parse payment details from 402 response${NC}"
    echo "Response content: $RESPONSE"
    exit 1
fi

# Step 5: Request payment invoice (choose Lightning)
echo -e "${BLUE}Step 5: Requesting Lightning payment invoice...${NC}"
PAYMENT_REQUEST=$(call_api "/l402/payment-request" "" "POST" \
    "{\"offer_id\":\"$OFFER_ID\",\"payment_method\":\"lightning\",\"payment_context_token\":\"$PAYMENT_CONTEXT_TOKEN\"}")

# Check if the response contains a valid payment request
if echo "$PAYMENT_REQUEST" | jq -e '.' >/dev/null 2>&1; then
    # Try to extract lightning invoice
    LIGHTNING_INVOICE=$(echo "$PAYMENT_REQUEST" | jq -r '.lightning_invoice // empty')
    if [ -z "$LIGHTNING_INVOICE" ] || [ "$LIGHTNING_INVOICE" = "null" ]; then
        echo -e "${RED}Error: Could not extract Lightning invoice from response${NC}"
        echo "Response: $PAYMENT_REQUEST"
        exit 1
    fi
    
    # Extract other useful information
    EXPIRES_AT=$(echo "$PAYMENT_REQUEST" | jq -r '.expires_at // "unknown"')
else
    echo -e "${RED}Error: Invalid JSON in payment request response${NC}"
    echo "Response: $PAYMENT_REQUEST"
    exit 1
fi

echo -e "${GREEN}✓ Lightning invoice generated${NC}"
echo -e "  Lightning Invoice: ${YELLOW}$LIGHTNING_INVOICE${NC}"
echo -e "  Amount: ${YELLOW}$AMOUNT_SATS sats${NC}"
echo -e "  Expires at: ${YELLOW}$EXPIRES_AT${NC}"
echo -e "  Auth Token: ${YELLOW}$AUTH_TOKEN${NC}"
echo

# Step 6: Display QR code for payment
if command -v qrencode &> /dev/null; then
    echo -e "${BLUE}Step 6: Generating QR code for payment...${NC}"
    echo "$LIGHTNING_INVOICE" | qrencode -t ANSIUTF8
    echo -e "${GREEN}✓ Scan this QR code with your Lightning wallet to pay${NC}"
else
    echo -e "${YELLOW}QR code generation skipped (qrencode not installed)${NC}"
    echo -e "${YELLOW}To install: brew install qrencode (Mac) or apt-get install qrencode (Linux)${NC}"
    echo -e "${YELLOW}Alternatively, copy this invoice to your Lightning wallet:${NC}"
    echo -e "${LIGHTNING_INVOICE}"
fi
echo

# Step 7: Wait for payment to be confirmed
echo -e "${BLUE}Step 7: Waiting for payment confirmation...${NC}"
echo -e "${YELLOW}====================== PAYMENT INSTRUCTIONS ======================${NC}"
echo -e "${YELLOW}1. Open your Lightning wallet (Phoenix, Muun, BlueWallet, etc.)${NC}"
echo -e "${YELLOW}2. Scan the QR code above or paste the Lightning invoice${NC}"
echo -e "${YELLOW}3. Send the payment (amount: $AMOUNT_SATS sats)${NC}"
echo -e "${YELLOW}4. Wait for confirmation${NC}"
echo -e "${YELLOW}=================================================================${NC}"
echo

echo -e "${BLUE}Checking payment status...${NC}"
echo -e "Press Ctrl+C to exit if you don't want to wait for payment"
echo

# Poll for payment confirmation by checking if credits increased
INITIAL_CREDITS="0"
USER_INFO=$(get_user_info "$AUTH_TOKEN")
if echo "$USER_INFO" | jq -e '.' >/dev/null 2>&1; then
    INITIAL_CREDITS=$(echo "$USER_INFO" | jq -r '.credits // "0"')
fi

PAYMENT_CONFIRMED=false
MAX_TRIES=30  # Check for up to 60 seconds (30 * 2 seconds)
for i in $(seq 1 $MAX_TRIES); do
    echo -e "Checking payment status ($i/$MAX_TRIES)..."
    
    # Check current credits
    USER_INFO=$(get_user_info "$AUTH_TOKEN" "true")
    if echo "$USER_INFO" | jq -e '.' >/dev/null 2>&1; then
        CURRENT_CREDITS=$(echo "$USER_INFO" | jq -r '.credits // "0"')
       
        # Compare with initial credits
        if [ "$CURRENT_CREDITS" -gt "$INITIAL_CREDITS" ]; then
            PAYMENT_CONFIRMED=true
            echo -e "${GREEN}✓ Payment confirmed!${NC}"
            echo -e "  Credits before: ${YELLOW}$INITIAL_CREDITS${NC}"
            echo -e "  Credits now: ${YELLOW}$CURRENT_CREDITS${NC}"
            break
        fi
    fi
    # Wait before checking again
    sleep 2
done

if [ "$PAYMENT_CONFIRMED" = false ]; then
    echo -e "${RED}Payment not confirmed after maximum wait time.${NC}"
    echo -e "${YELLOW}You can manually check your credits and continue using the API with:${NC}"
    echo -e "  ${GREEN}curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/info${NC}"
    exit 1
fi

# Step 8: Test API access with new credits
echo -e "${BLUE}Step 8: Testing API access with new credits...${NC}"
BLOCK_RESULT=$(get_block "$AUTH_TOKEN")
STATUS_CODE=$(echo "$BLOCK_RESULT" | grep "^STATUS:" | cut -d':' -f2)
RESPONSE_LINE=$(echo "$BLOCK_RESULT" | grep "^RESPONSE:")
RESPONSE=${RESPONSE_LINE#RESPONSE:}

if [ "$STATUS_CODE" = "200" ]; then
    echo -e "${GREEN}✓ API access successful with paid credits!${NC}"
    if echo "$RESPONSE" | jq -e '.' >/dev/null 2>&1; then
        echo -e "  Response: $(echo "$RESPONSE" | jq -c '.' | head -1)"
    else
        echo -e "  Response: $RESPONSE"
    fi
else
    echo -e "${RED}✕ API access failed with status code $STATUS_CODE${NC}"
    echo "Response: $RESPONSE"
fi
echo

# Step 9: Final summary
echo -e "${GREEN}====== L402 Payment Flow Test Completed Successfully ======${NC}"
echo -e "  Auth Token: ${YELLOW}$AUTH_TOKEN${NC}"
echo -e "  Credits: ${YELLOW}$CURRENT_CREDITS${NC}"
echo
echo -e "${BLUE}You can continue to use the API with:${NC}"
echo -e "  ${GREEN}curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/block${NC}"
echo
echo -e "${BLUE}Check your credit balance with:${NC}"
echo -e "  ${GREEN}curl -H \"Authorization: Bearer $AUTH_TOKEN\" $BASE_URL/info${NC}"