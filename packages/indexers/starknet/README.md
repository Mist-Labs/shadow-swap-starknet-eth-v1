# ShadowSwap Indexer (StarkNet)

Apibara indexer for ShadowSwap privacy bridge on StarkNet Mainnet.

## Architecture

```
StarkNet Contract → Apibara → PostgreSQL → Backend Relayer (via HMAC webhook)
```

- **Apibara**: Indexes StarkNet events in real-time
- **PostgreSQL**: Stores indexed events
- **Drizzle ORM**: Type-safe database operations
- **Backend Relayer**: Receives forwarded events via HMAC-authenticated webhooks

## Events Indexed

1. **CommitmentAdded** - When commitment added to Merkle tree
2. **IntentSettled** - When settlement completes on destination chain
3. **IntentMarkedSettled** - When settlement marked on source chain
4. **MerkleRootUpdated** - When Merkle root syncs
5. **BatchProcessed** - When pending batch is processed

## Setup

### 1. Install Dependencies

```bash
pnpm install
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env with your values
```

Required environment variables:
- `DNA_TOKEN` - Apibara DNA token (get from https://apibara.com)
- `POSTGRES_CONNECTION_STRING` - PostgreSQL connection string
- `CONTRACT_ADDRESS` - ShadowSwap contract address on StarkNet
- `STREAM_URL` - Apibara stream URL (mainnet)
- `RELAYER_BASE_URL` - Your relayer backend URL
- `HMAC_SECRET` - Must match backend HMAC_SECRET

### 3. Set Up Database

Generate migrations:
```bash
pnpm drizzle:generate
```

Run migrations:
```bash
pnpm drizzle:migrate
```

### 4. Update Starting Block

Edit `apibara.config.ts` and set `startingBlock` to the block where the contract was deployed.

To find the deployment block:
```bash
# Check StarkNet explorer or use starknet-rs
starkli block-number --rpc https://starknet-mainnet.g.alchemy.com/v2/YOUR_KEY
```

### 5. Run Indexer

Development:
```bash
pnpm dev
```

Production:
```bash
pnpm start
```

## Deployment

### Docker

Build:
```bash
docker build -t shadowswap-indexer-starknet .
```

Run:
```bash
docker run --env-file .env shadowswap-indexer-starknet
```

### Cloud Deployment

Deploy to your hosting platform (Railway, Render, Fly.io, etc.)

Required environment variables must be set in your cloud platform's dashboard.

## Database Schema

### commitments
- Stores commitment hashes from CommitmentAdded events

### intents_settled
- Stores settlement completions with recipient/amount details

### intents_marked_settled
- Stores settlement confirmations on source chain

### merkle_roots
- Stores Merkle root updates for cross-chain verification

### batches_processed
- Stores batch processing events (batch_id, count, reason)

## Contract Address

**StarkNet Mainnet:** `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114`

## Event Keys

Events are identified by their Starknet.keccak256 selector:

```typescript
const EVENT_KEYS = {
  CommitmentAdded: "0x01b4bb6b29c34a4e5c9b868d74c58d9e28c4cfaa62b8c4cc3ea5876b18c6b9b7",
  IntentSettled: "0x03cf89e5fcb8ae2ace80fcf6e83f0e03ae63e1be22e4c1a1d08ab49f8293b0f6",
  IntentMarkedSettled: "0x02b78c6fbe8ee2e86ebb1e29e3c3e2ff8b18d6e6f6bc2759d8c2a75e5ea8f7a9",
  MerkleRootUpdated: "0x01f42a07acee6e8e7fd03c4a7e8c0f8d9d4f7e60efd36ad9be1f8e4c6c5b2e8f",
  BatchProcessed: "0x036c9c5c5cdb4d1b0e5a8e6f3c1b8e7d2a4f6b9c7e8a1d3f5c7b9e2a4f6d8b0c",
};
```

## Troubleshooting

### Indexer not receiving events
1. Check `CONTRACT_ADDRESS` matches deployed contract
2. Verify `startingBlock` is set to deployment block (not before)
3. Check Apibara DNA token is valid
4. Ensure PostgreSQL is accessible

### Database connection errors
1. Verify `POSTGRES_CONNECTION_STRING` format
2. Check database exists and user has permissions
3. Run migrations if tables don't exist

### Events not forwarding to backend
1. Check `RELAYER_BASE_URL` is correct
2. Verify `HMAC_SECRET` matches backend configuration
3. Check backend logs for authentication errors

## Support

For issues or questions:
- GitHub: https://github.com/shadowswap/indexers
- Docs: https://docs.shadowswap.io
