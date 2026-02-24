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

echo -e "${BLUE}=== StarkNet ShadowSettlement Deployment (sncast) ===${NC}"
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

if [ -z "$OWNER_ADDRESS" ]; then
    echo -e "${YELLOW}Warning: OWNER_ADDRESS not set, using RELAYER_ADDRESS${NC}"
    OWNER_ADDRESS="$RELAYER_ADDRESS"
fi

if [ -z "$RELAYER_ADDRESS" ]; then
    echo -e "${RED}Error: RELAYER_ADDRESS not set${NC}"
    exit 1
fi

echo -e "${GREEN}Network:${NC} ${STARKNET_RPC}"
echo -e "${GREEN}Account:${NC} ${SNCAST_ACCOUNT}"
echo -e "${GREEN}Owner:${NC} ${OWNER_ADDRESS}"
echo -e "${GREEN}Relayer:${NC} ${RELAYER_ADDRESS}"
echo ""

# Must run from the Scarb project directory
cd "$SCRIPT_DIR/shadow_swap"

# Step 1: Declare the contract
echo -e "${BLUE}Step 1: Declaring ShadowSettlement contract...${NC}"
DECLARE_OUTPUT=$(sncast --account "$SNCAST_ACCOUNT" \
    declare --contract-name ShadowSettlement \
    --url "$STARKNET_RPC" 2>&1) || true

echo "$DECLARE_OUTPUT"

# Extract class hash
CLASS_HASH=$(echo "$DECLARE_OUTPUT" | sed -n 's/.*class_hash: *\(0x[a-fA-F0-9]*\).*/\1/p' | head -1)
if [ -z "$CLASS_HASH" ]; then
    CLASS_HASH=$(echo "$DECLARE_OUTPUT" | sed -n 's/.*already declared.*\(0x[a-fA-F0-9]\{50,\}\).*/\1/p')
fi
if [ -z "$CLASS_HASH" ]; then
    echo -e "${RED}Error: Could not determine ShadowSettlement class hash${NC}"
    exit 1
fi

echo -e "${GREEN}Class Hash:${NC} ${CLASS_HASH}"
echo ""

# Wait for declare to be accepted
echo -e "${YELLOW}Waiting for declare tx to confirm...${NC}"
sleep 8

# Step 2: Deploy the contract
# Constructor: (owner: ContractAddress, initial_relayer: ContractAddress)
echo -e "${BLUE}Step 2: Deploying ShadowSettlement...${NC}"
DEPLOY_OUTPUT=$(sncast --account "$SNCAST_ACCOUNT" \
    deploy --class-hash "$CLASS_HASH" \
    --constructor-calldata "$OWNER_ADDRESS" "$RELAYER_ADDRESS" \
    --url "$STARKNET_RPC" 2>&1)

echo "$DEPLOY_OUTPUT"

# Extract contract address
CONTRACT_ADDRESS=$(echo "$DEPLOY_OUTPUT" | sed -n 's/.*contract_address: \(0x[a-fA-F0-9]*\).*/\1/p')

echo ""
echo -e "${GREEN}=== DEPLOYMENT COMPLETE ===${NC}"
echo ""
echo -e "${YELLOW}Contract Address:${NC} ${CONTRACT_ADDRESS}"
echo ""
echo "Update your .env with:"
echo "export STARKNET_CONTRACT_ADDRESS=\"${CONTRACT_ADDRESS}\""
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo "1. Update .env with the contract address above"
echo "2. Whitelist tokens: ./configure_contract.sh"
echo "3. Fund contract with tokens for settlements"
