#!/usr/bin/env bash
set -e

# Resolve script directory and source .env
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/.env"

# Color output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== StarkNet Token Deployment (sncast) ===${NC}"
echo ""

# Check required env vars
if [ -z "$SNCAST_ACCOUNT" ]; then
    echo -e "${RED}Error: SNCAST_ACCOUNT not set in .env${NC}"
    exit 1
fi

if [ -z "$STARKNET_RPC" ]; then
    echo -e "${RED}Error: STARKNET_RPC not set in .env${NC}"
    exit 1
fi

RECIPIENT="${OWNER_ADDRESS:-$RELAYER_ADDRESS}"
if [ -z "$RECIPIENT" ]; then
    echo -e "${RED}Error: OWNER_ADDRESS or RELAYER_ADDRESS must be set${NC}"
    exit 1
fi

echo -e "${GREEN}Network:${NC} ${STARKNET_RPC}"
echo -e "${GREEN}Account:${NC} ${SNCAST_ACCOUNT}"
echo -e "${GREEN}Token recipient:${NC} ${RECIPIENT}"
echo ""

# Must run from the Scarb project directory
cd "$SCRIPT_DIR/shadow_swap"

# Step 1: Declare MockERC20
echo -e "${BLUE}Step 1: Declaring MockERC20 class...${NC}"
DECLARE_OUTPUT=$(sncast --account "$SNCAST_ACCOUNT" \
    declare --contract-name MockERC20 \
    --url "$STARKNET_RPC" 2>&1) || true

echo "$DECLARE_OUTPUT"

# Extract class hash from declare output
CLASS_HASH=$(echo "$DECLARE_OUTPUT" | sed -n 's/.*class_hash: *\(0x[a-fA-F0-9]*\).*/\1/p' | head -1)
if [ -z "$CLASS_HASH" ]; then
    CLASS_HASH=$(echo "$DECLARE_OUTPUT" | sed -n 's/.*already declared.*\(0x[a-fA-F0-9]\{50,\}\).*/\1/p')
fi
if [ -z "$CLASS_HASH" ]; then
    echo -e "${RED}Error: Could not determine MockERC20 class hash${NC}"
    exit 1
fi

echo -e "${GREEN}MockERC20 Class Hash:${NC} ${CLASS_HASH}"
echo ""

# Wait for declare to be accepted
echo -e "${YELLOW}Waiting for declare tx to confirm...${NC}"
sleep 8

# Step 2: Deploy USDC (6 decimals, 1M supply = 1_000_000 * 10^6 = 1_000_000_000_000)
echo -e "${BLUE}Step 2: Deploying USDC...${NC}"
USDC_OUTPUT=$(sncast --account "$SNCAST_ACCOUNT" \
    deploy --class-hash "$CLASS_HASH" \
    --arguments "\"USD Coin\", \"USDC\", 6, 1000000000000, $RECIPIENT" \
    --url "$STARKNET_RPC" 2>&1)

echo "$USDC_OUTPUT"
USDC_ADDRESS=$(echo "$USDC_OUTPUT" | sed -n 's/.*contract_address: *\(0x[a-fA-F0-9]*\).*/\1/p')
echo -e "${GREEN}USDC Address:${NC} ${USDC_ADDRESS}"
echo ""

# Step 3: Deploy USDT (6 decimals, 1M supply)
echo -e "${BLUE}Step 3: Deploying USDT...${NC}"
USDT_OUTPUT=$(sncast --account "$SNCAST_ACCOUNT" \
    deploy --class-hash "$CLASS_HASH" \
    --arguments "\"Tether USD\", \"USDT\", 6, 1000000000000, $RECIPIENT" \
    --url "$STARKNET_RPC" 2>&1)

echo "$USDT_OUTPUT"
USDT_ADDRESS=$(echo "$USDT_OUTPUT" | sed -n 's/.*contract_address: *\(0x[a-fA-F0-9]*\).*/\1/p')
echo -e "${GREEN}USDT Address:${NC} ${USDT_ADDRESS}"
echo ""

echo -e "${GREEN}=== All tokens deployed! ===${NC}"
echo ""
echo "Update your .env with:"
echo "export STARKNET_USDC_ADDRESS=\"${USDC_ADDRESS}\""
echo "export STARKNET_USDT_ADDRESS=\"${USDT_ADDRESS}\""
