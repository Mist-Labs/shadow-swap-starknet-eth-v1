#!/bin/bash

set -e

source .env

echo "🚀 Deploying ShadowSwap Indexer to Goldsky..."

# Deploy Ethereum Mainnet
echo ""
echo "📦 Deploying Ethereum indexer..."
goldsky subgraph deploy shadowswap-ethereum/1.0.0 \
  --from-abi ./goldsky-config-ethereum.json

echo ""
echo "✅ Deployment complete!"
echo ""
echo "Next steps:"
echo "1. Set up webhook in Goldsky dashboard:"
echo "   - Webhook URL: https://your-indexer-domain.com/webhook"
echo "   - Add 'goldsky-webhook-secret' header with your secret"
echo "2. Update .env with your webhook secret"
echo "3. Deploy indexer webhook server (see README.md)"
