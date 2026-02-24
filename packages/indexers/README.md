# ShadowSwap Indexers

Real-time event indexers for ShadowSwap privacy bridge on both EVM and StarkNet chains.

## Directory Structure

```
packages/indexers/
├── evm/          # Goldsky webhook indexer for Ethereum/Sepolia
└── starknet/     # Apibara indexer for StarkNet Mainnet
```

## EVM Indexer (Goldsky)

**Location:** `packages/indexers/evm/`

- **Technology:** Goldsky + Express webhook server
- **Chains:** Ethereum Mainnet, Sepolia
- **Contract:** `0x239365b4A26d947B9CE38ec88A804F03b8248042`
- **Architecture:** Goldsky → Webhook Server → Backend Relayer

### Setup

```bash
cd evm/
npm install
cp .env.example .env
# Edit .env with your values
npm run build
npm start
```

See [evm/README.md](evm/README.md) for detailed instructions.

## StarkNet Indexer (Apibara)

**Location:** `packages/indexers/starknet/`

- **Technology:** Apibara + Drizzle ORM + PostgreSQL
- **Chain:** StarkNet Mainnet
- **Contract:** `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114`
- **Architecture:** Apibara → PostgreSQL → Backend Relayer

### Setup

```bash
cd starknet/
pnpm install
cp .env.example .env
# Edit .env with your values
pnpm drizzle:migrate
pnpm start
```

See [starknet/README.md](starknet/README.md) for detailed instructions.

## Events Indexed

Both indexers track the same privacy-preserving bridge events:

| Event | Description |
|-------|-------------|
| `commitment_added` | Commitment added to Merkle tree |
| `settled` | Settlement completed on destination chain |
| `marked_settled` | Settlement marked on source chain |
| `merkle_root_updated` | Merkle root synchronized between chains |
| `batch_processed` | Pending commitments batch processed (StarkNet only) |

## Backend Integration

Both indexers forward events to the ShadowSwap backend via HMAC-authenticated webhooks:

**Endpoint:** `POST /api/v1/indexer/event`

**Authentication:** HMAC-SHA256 signature

```typescript
headers: {
  "X-HMAC-Signature": hmac_signature,
  "X-Timestamp": timestamp
}
```

**Payload:**
```json
{
  "event_type": "commitment_added",
  "chain": "ethereum" | "starknet",
  "transaction_hash": "0x...",
  "data": { /* event-specific data */ }
}
```

## Environment Variables

### Common Variables (Both Indexers)

```bash
RELAYER_BASE_URL=https://your-backend.com/api/v1
HMAC_SECRET=your_hmac_secret_here  # Must match backend
```

### EVM-Specific

```bash
GOLDSKY_WEBHOOK_SECRET=your_goldsky_secret
PORT=3000
```

### StarkNet-Specific

```bash
DNA_TOKEN=dna_your_apibara_token
POSTGRES_CONNECTION_STRING=postgresql://...
CONTRACT_ADDRESS=0x07576...
STREAM_URL=https://mainnet.starknet.a5a.ch
```

## Deployment

### EVM Indexer Deployment

1. Deploy webhook server to Railway/Render/Fly.io
2. Deploy indexer to Goldsky using `deploy.sh`
3. Configure webhook URL in Goldsky dashboard

### StarkNet Indexer Deployment

1. Set up PostgreSQL database
2. Deploy indexer to Railway/Render/Fly.io
3. Run database migrations
4. Indexer starts automatically and resumes from last block

## Monitoring

Both indexers provide health check endpoints:

- **EVM:** `GET http://localhost:3000/health`
- **StarkNet:** Check Apibara dashboard logs

## Support

For issues or questions:
- GitHub: https://github.com/shadowswap/indexers
- Docs: https://docs.shadowswap.io
