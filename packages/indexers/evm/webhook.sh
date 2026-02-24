#!/bin/bash
set -euo pipefail

source .env

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🔧 Goldsky Webhook Setup - ShadowSwap${NC}"
echo ""

# Load env vars safely if .env exists
if [ -f .env ]; then
  echo -e "${YELLOW}📄 Loading environment variables from .env...${NC}"
  set -a
  while IFS= read -r line; do
    [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
    line=$(echo "$line" | sed 's/[[:space:]]*#.*$//')
    [[ -z "$line" ]] && continue
    export "$line"
  done < <(grep -v '^[[:space:]]*$' .env)
  set +a
  echo -e "${GREEN}✓ Environment variables loaded${NC}"
  echo ""
else
  echo -e "${RED}❌ Error: .env file not found!${NC}"
  echo ""
  exit 1
fi

# Validate required environment variables
if [ -z "${WEBHOOK_URL:-}" ] || [ -z "${GOLDSKY_WEBHOOK_SECRET:-}" ]; then
  echo -e "${RED}❌ Error: WEBHOOK_URL or GOLDSKY_WEBHOOK_SECRET is not set${NC}"
  echo -e "${YELLOW}💡 Add WEBHOOK_URL to your .env file (e.g., https://your-domain.com/webhook)${NC}"
  exit 1
fi

echo -e "${GREEN}✓ Environment validation passed${NC}"
echo -e "${BLUE}Webhook URL: ${WEBHOOK_URL}${NC}"
echo ""

# Function to create webhook with error handling
create_webhook() {
  local subgraph=$1
  local name=$2
  local entity=$3

  echo -e "${YELLOW}  Creating: ${name}...${NC}"

  if goldsky subgraph webhook create "$subgraph" \
    --name "$name" \
    --entity "$entity" \
    --url "$WEBHOOK_URL" \
    --secret "$GOLDSKY_WEBHOOK_SECRET" 2>&1; then
    echo -e "${GREEN}  ✓ ${name} created${NC}"
  else
    echo -e "${RED}  ✗ Failed to create ${name}${NC}"
    echo -e "${YELLOW}  (webhook may already exist or entity '${entity}' doesn't exist)${NC}"
  fi
  echo ""
}

################################
# Ethereum Mainnet Webhooks
################################
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📡 Creating Ethereum Mainnet Webhooks${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

SUBGRAPH="shadowswap-ethereum-mainnet/1.0.0"

# ShadowSwap Events (matching Solidity event names)
echo -e "${YELLOW}ShadowSwap Privacy Bridge Events:${NC}"
create_webhook "$SUBGRAPH" "ethereum-commitment-added" "commitment_added"
create_webhook "$SUBGRAPH" "ethereum-intent-settled" "intent_settled"
create_webhook "$SUBGRAPH" "ethereum-intent-marked-settled" "intent_marked_settled"
create_webhook "$SUBGRAPH" "ethereum-merkle-root-updated" "merkle_root_updated"
create_webhook "$SUBGRAPH" "ethereum-batch-processed" "batch_processed"

################################
# Done
################################
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}✅ Webhook setup completed!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "${BLUE}📋 Event Summary:${NC}"
echo "  • CommitmentAdded - Privacy commitment added to Merkle tree"
echo "  • IntentSettled - Intent settled on destination chain (token released)"
echo "  • IntentMarkedSettled - Settlement marked on source chain"
echo "  • MerkleRootUpdated - Merkle root synchronized"
echo "  • BatchProcessed - Pending commitments batch processed"
echo ""
echo -e "${YELLOW}Next Steps:${NC}"
echo "  1. Deploy your webhook server (Railway/Render/Fly.io)"
echo "  2. Update WEBHOOK_URL in .env with your deployed URL"
echo "  3. Verify webhooks in Goldsky dashboard"
echo ""
