# ShadowSwap Indexer (StarkNet)

Apibara v2 indexer for the ShadowSwap privacy bridge on StarkNet Mainnet. Streams contract events in real-time, writes to PostgreSQL, and forwards to the settlement relayer via HMAC-authenticated webhooks.

## Architecture

```
StarkNet Contract → Apibara v2 → PostgreSQL → Backend Relayer (HMAC webhook)
```

- **Apibara v2**: Streams StarkNet events in real-time via DNA protocol
- **PostgreSQL**: Persists indexed events (Neon, hosted)
- **Drizzle ORM**: Type-safe database operations
- **Backend Relayer**: Receives forwarded events via HMAC-signed requests

## Events Indexed

| Event | Description |
|-------|-------------|
| `CommitmentAdded` | Commitment added to on-chain Merkle tree |
| `IntentSettled` | Settlement completed on destination chain |
| `IntentMarkedSettled` | Settlement marked on source chain |
| `MerkleRootUpdated` | Merkle root updated after batch processed |
| `BatchProcessed` | Pending batch sealed on-chain |

## Contract Address

**StarkNet Mainnet:** `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114`

**Starting block:** `6946374`

---

## Setup

### 1. Install Dependencies

```bash
pnpm install
```

### 2. Configure Environment

```bash
cp .env.example .env
```

```env
DNA_TOKEN=...                          # Apibara DNA token — get from https://app.apibara.com
DATABASE_URL=postgresql://...          # PostgreSQL connection string (Neon or self-hosted)
RELAYER_BASE_URL=https://your-backend.com/api/v1
HMAC_SECRET=...                        # Must match backend HMAC_SECRET exactly
```

### 3. Run Database Migrations

```bash
pnpm drizzle:generate
pnpm drizzle:migrate
```

### 4. Verify Starting Block

Check `apibara.config.ts` — `startingBlock` must be set to the contract deployment block:

```typescript
startingBlock: 6946374  // ShadowSwap deployment block
```

Do not set this earlier than the deployment block — Apibara will scan unnecessarily.

### 5. Run

Development:
```bash
pnpm dev
```

Production:
```bash
pnpm start
```

---

## Deployment

### Koyeb

Build from **repo root** (monorepo — build context must be root):

```bash
docker build -t shadowswap-starknet-indexer -f packages/indexers/starknet/Dockerfile .
```

- Dockerfile location: `packages/indexers/starknet/Dockerfile`
- Deploy as: **Worker** (no HTTP endpoints — no health check required)

### Docker

```bash
# Build from repo root
docker build -t shadowswap-starknet-indexer -f packages/indexers/starknet/Dockerfile .
docker run --env-file .env shadowswap-starknet-indexer
```

### Railway / Render / Fly.io

Set all environment variables in the platform dashboard and deploy normally.

---

## Database Schema

All tables are written by the indexer and read by the relayer.

| Table | Source Event | Contents |
|-------|-------------|----------|
| `commitments` | `CommitmentAdded` | commitment hash, near_intents_id, view_key, block/log index |
| `intents_settled` | `IntentSettled` | intent_id, nullifier_hash, token, amount, timestamp |
| `intents_marked_settled` | `IntentMarkedSettled` | commitment, nullifier_hash |
| `merkle_roots` | `MerkleRootUpdated` | root, leaf_count, timestamp |
| `batches_processed` | `BatchProcessed` | batch_id, count, reason |

### Reindex from Scratch

To replay all events from the starting block:

```sql
DELETE FROM airfoil.checkpoints WHERE id = 'indexer_shadowswap_default';
```

The indexer uses Apibara's `airfoil` schema to track its checkpoint. Deleting it causes a full reindex on next startup.

---

## Event Keys

Starknet Keccak256 selectors used to filter events:

```typescript
const EVENT_KEYS = {
  CommitmentAdded:     "0x01b4bb6b29c34a4e5c9b868d74c58d9e28c4cfaa62b8c4cc3ea5876b18c6b9b7",
  IntentSettled:       "0x03cf89e5fcb8ae2ace80fcf6e83f0e03ae63e1be22e4c1a1d08ab49f8293b0f6",
  IntentMarkedSettled: "0x02b78c6fbe8ee2e86ebb1e29e3c3e2ff8b18d6e6f6bc2759d8c2a75e5ea8f7a9",
  MerkleRootUpdated:   "0x01f42a07acee6e8e7fd03c4a7e8c0f8d9d4f7e60efd36ad9be1f8e4c6c5b2e8f",
  BatchProcessed:      "0x036c9c5c5cdb4d1b0e5a8e6f3c1b8e7d2a4f6b9c7e8a1d3f5c7b9e2a4f6d8b0c",
};
```

---

## Troubleshooting

**Indexer not receiving events**
- Verify `CONTRACT_ADDRESS` matches deployed contract
- Confirm `startingBlock` is at or after the deployment block (`6946374`)
- Check Apibara DNA token is valid at https://app.apibara.com
- Ensure `DATABASE_URL` is accessible from the indexer host

**Database connection errors**
- Verify `DATABASE_URL` format: `postgresql://user:password@host/dbname`
- Run `pnpm drizzle:migrate` if tables don't exist
- Check the Neon console for connection limits

**Events not forwarding to relayer**
- Confirm `RELAYER_BASE_URL` is correct and reachable
- Verify `HMAC_SECRET` exactly matches the backend env var
- Check relayer logs for `401` authentication errors

**Stuck / stale indexer**
- Delete the Apibara checkpoint and restart (see Reindex above)
- Check Apibara stream status at https://status.apibara.com