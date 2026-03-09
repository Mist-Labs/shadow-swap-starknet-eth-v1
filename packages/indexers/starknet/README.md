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
| `IntentSettled` | Settlement completed — direct token delivery, no swap |
| `IntentSettledWithSwap` | Settlement completed — STRK delivered and swapped to dest token via AVNU |
| `IntentMarkedSettled` | Settlement marked on source chain |
| `MerkleRootUpdated` | Merkle root updated after batch processed |
| `BatchProcessed` | Pending batch sealed on-chain |

## Contract Address

**StarkNet Mainnet:** `0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0`

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

| Table | Source Event(s) | Contents |
|-------|----------------|----------|
| `commitments` | `CommitmentAdded` | commitment hash, near_intents_id, view_key, block/log index |
| `intents_settled` | `IntentSettled`, `IntentSettledWithSwap` | intent_id, nullifier_hash, token, amount, timestamp. For swap events: `is_swap=true`, `delivered_token`, `delivered_amount_low/high` also populated |
| `intents_marked_settled` | `IntentMarkedSettled` | commitment, nullifier_hash |
| `merkle_roots` | `MerkleRootUpdated` | root, leaf_count, timestamp |
| `batches_processed` | `BatchProcessed` | batch_id, count, reason |

### Reindex from Scratch

Stop the indexer first, then run in the `shadow-swap-indexer` DB:

```sql
-- Reset Apibara cursor
DELETE FROM airfoil.checkpoints;
DELETE FROM airfoil.filters;

-- Clear all indexed data
TRUNCATE commitments RESTART IDENTITY CASCADE;
TRUNCATE intents_settled RESTART IDENTITY CASCADE;
TRUNCATE intents_marked_settled RESTART IDENTITY CASCADE;
TRUNCATE merkle_roots RESTART IDENTITY CASCADE;
TRUNCATE batches_processed RESTART IDENTITY CASCADE;
```

Then restart the indexer — it will replay all events from `startingBlock` and forward them to the relayer in block order.

---

## Event Keys

Selectors are computed at runtime via `starknet.js` `hash.getSelectorFromName`. The indexer does not hardcode selector hex values — it computes them on startup from event names, ensuring correctness regardless of Cairo version.

```typescript
const EVENT_KEYS = {
  CommitmentAdded:       pad64(hash.getSelectorFromName("CommitmentAdded")),
  IntentSettled:         pad64(hash.getSelectorFromName("IntentSettled")),
  IntentSettledWithSwap: pad64(hash.getSelectorFromName("IntentSettledWithSwap")),
  IntentMarkedSettled:   pad64(hash.getSelectorFromName("IntentMarkedSettled")),
  MerkleRootUpdated:     pad64(hash.getSelectorFromName("MerkleRootUpdated")),
  BatchProcessed:        pad64(hash.getSelectorFromName("BatchProcessed")),
};
```

---

## Troubleshooting

**Indexer not receiving events**
- Verify `CONTRACT_ADDRESS` matches deployed contract (`0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0`)
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
- Stop the indexer, run the full reindex SQL above, then restart (see Reindex section)
- Check Apibara stream status at https://status.apibara.com