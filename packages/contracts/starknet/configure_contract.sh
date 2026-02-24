#!/usr/bin/env bash
set -e

# Resolve script directory and source .env
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/.env"

# Color output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Configuring ShadowSettlement (sncast) ===${NC}"
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

if [ -z "$STARKNET_CONTRACT_ADDRESS" ]; then
    echo -e "${RED}Error: STARKNET_CONTRACT_ADDRESS not set in .env${NC}"
    exit 1
fi

echo -e "${GREEN}Settlement Contract:${NC} ${STARKNET_CONTRACT_ADDRESS}"
echo ""

# Whitelist USDC
if [ -n "$STARKNET_USDC_ADDRESS" ]; then
    echo -e "${BLUE}Whitelisting USDC...${NC}"
    sncast --account "$SNCAST_ACCOUNT" \
        invoke --contract-address "$STARKNET_CONTRACT_ADDRESS" \
        --function set_token_whitelist \
        --calldata "$STARKNET_USDC_ADDRESS" 1 \
        --url "$STARKNET_RPC"
    echo -e "${GREEN}USDC whitelisted${NC}"
else
    echo -e "${RED}STARKNET_USDC_ADDRESS not set, skipping${NC}"
fi

# Whitelist USDT
if [ -n "$STARKNET_USDT_ADDRESS" ]; then
    echo -e "${BLUE}Whitelisting USDT...${NC}"
    sncast --account "$SNCAST_ACCOUNT" \
        invoke --contract-address "$STARKNET_CONTRACT_ADDRESS" \
        --function set_token_whitelist \
        --calldata "$STARKNET_USDT_ADDRESS" 1 \
        --url "$STARKNET_RPC"
    echo -e "${GREEN}USDT whitelisted${NC}"
else
    echo -e "${RED}STARKNET_USDT_ADDRESS not set, skipping${NC}"
fi

# Whitelist STRK
if [ -n "$STARKNET_STRK_ADDRESS" ]; then
    echo -e "${BLUE}Whitelisting STRK...${NC}"
    sncast --account "$SNCAST_ACCOUNT" \
        invoke --contract-address "$STARKNET_CONTRACT_ADDRESS" \
        --function set_token_whitelist \
        --calldata "$STARKNET_STRK_ADDRESS" 1 \
        --url "$STARKNET_RPC"
    echo -e "${GREEN}STRK whitelisted${NC}"
else
    echo -e "${RED}STARKNET_STRK_ADDRESS not set, skipping${NC}"
fi

# Whitelist ETH
if [ -n "$STARKNET_ETH_ADDRESS" ]; then
    echo -e "${BLUE}Whitelisting ETH...${NC}"
    sncast --account "$SNCAST_ACCOUNT" \
        invoke --contract-address "$STARKNET_CONTRACT_ADDRESS" \
        --function set_token_whitelist \
        --calldata "$STARKNET_ETH_ADDRESS" 1 \
        --url "$STARKNET_RPC"
    echo -e "${GREEN}ETH whitelisted${NC}"
else
    echo -e "${RED}STARKNET_ETH_ADDRESS not set, skipping${NC}"
fi

echo ""
echo -e "${GREEN}=== Configuration complete! ===${NC}"
