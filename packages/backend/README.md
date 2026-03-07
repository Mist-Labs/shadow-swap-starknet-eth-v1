# shadow-settlement-relayer

Rust backend for the ShadowSwap Privacy Bridge. Handles commitment batching, NEAR 1Click polling, Merkle proof verification, and cross-chain settlement between Ethereum Mainnet and StarkNet Mainnet.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  HTTP Server (Actix)                │
│         POST /api/v1/bridge/initiate                │
│         GET  /api/v1/bridge/status/:id              │
└──────────────────────┬──────────────────────────────┘
                       │
          ┌────────────▼─────────────┐
          │      Background Services │
          ├──────────────────────────┤
          │  RelayCoordinator        │  10s interval
          │  SettlementCoordinator   │  15s interval
          │  RootSyncCoordinator     │  30s interval
          └────────────┬─────────────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
   EvmRelayer   StarkNetRelayer   NearClient
   (Ethereum)   (StarkNet)        (1Click API)
```

### Intent Lifecycle

```
pending → batched → near_submitted → tokens_delivered → settled → marked_settled
                                                      ↘ (rescue after 1hr)
```

| Status | Description |
|--------|-------------|
| `pending` | Received from frontend, awaiting on-chain batching |
| `batched` | Commitment added to on-chain pending batch |
| `near_submitted` | Watching NEAR 1Click for delivery |
| `tokens_delivered` | NEAR confirmed, tokens at destination contract |
| `settled` | `settle_and_release` called on destination chain |
| `marked_settled` | **FINAL** — source chain marked settled |
| `failed` | NEAR swap failed or verification failed |
| `refunded` | NEAR refunded to user source address |
| `settlement_failed` | Settlement tx failed after retries — manual intervention required |

---

## Contract Addresses (Mainnet)

| Chain | Contract | Address |
|-------|----------|---------|
| Ethereum Mainnet | ShadowSettlement | `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468` |
| StarkNet Mainnet | ShadowSwapSettlement | `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114` |

---

## Environment Variables

```env
# ── Database ──────────────────────────────────────────────────────
DATABASE_URL=postgresql://user:password@host/dbname
DATABASE_MAX_CONNECTIONS=20

# ── EVM (Ethereum Mainnet) ────────────────────────────────────────
EVM_RPC_URL=https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY
EVM_SETTLEMENT_ADDRESS=0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468
EVM_PRIVATE_KEY=0x...          # Relayer wallet — must be authorized on contract
EVM_OWNER_PRIVATE_KEY=0x...    # Owner wallet — for rescue_tokens fallback only

# ── StarkNet ──────────────────────────────────────────────────────
STARKNET_RPC_URL=https://starknet-mainnet.g.alchemy.com/starknet/version/rpc/v0_10/YOUR_KEY
STARKNET_CONTRACT_ADDRESS=0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114
STARKNET_PRIVATE_KEY=0x...
STARKNET_ACCOUNT_ADDRESS=0x...
STARKNET_CHAIN_ID=SN_MAIN

# ── NEAR 1Click ───────────────────────────────────────────────────
NEAR_API_KEY=your_jwt_token

# ── Server ────────────────────────────────────────────────────────
HOST=0.0.0.0
PORT=8080
HMAC_SECRET=your_hmac_secret           # Shared with frontend for request auth
RELAYER_PRIVATE_KEY=0x...              # ECIES private key for decrypting settlement params
CORS_ORIGIN=https://your-frontend.com
```

> **Never commit private keys.** Use `.env` locally and secret injection in production (Koyeb secrets / Doppler / AWS Secrets Manager).

---

## Running Locally

```bash
# Prerequisites: Rust stable, PostgreSQL

# 1. Clone and install
git clone https://github.com/your-org/shadow-swap-starknet-eth-v1
cd packages/backend

# 2. Set up environment
cp .env.example .env
# Edit .env with your values

# 3. Run migrations (auto-runs on startup, or manually)
cargo run

# 4. Build for production
cargo build --release
```

---

## Docker

Build from **repo root** (monorepo — build context must be root):

```bash
# Build
docker build -t shadow-settlement-relayer -f packages/backend/Dockerfile .

# Run
docker run --env-file packages/backend/.env -p 8080:8080 shadow-settlement-relayer
```

---

## Database

PostgreSQL (tested on Neon). Migrations run automatically on startup via `diesel_migrations`.

### Key Tables

| Table | Purpose |
|-------|---------|
| `shadow_intents` | Intent lifecycle state |
| `merkle_leaves` | Ordered commitment leaves per tree |
| `merkle_roots` | Root history per tree |
| `commitments` | On-chain commitment event log |
| `transaction_logs` | Audit trail of all relayer txs |

### Reindex Merkle Tree

If leaves get out of sync with on-chain state:

```sql
-- Remove duplicates (keep lowest id)
DELETE FROM merkle_leaves
WHERE leaf_id NOT IN (
    SELECT MIN(leaf_id) FROM merkle_leaves GROUP BY tree_name, leaf
);

-- Check current state
SELECT tree_name, COUNT(*) FROM merkle_leaves GROUP BY tree_name;
```

### Reset Intent Status (manual intervention)

```sql
-- Re-queue a stuck intent
UPDATE shadow_intents
SET status = 'tokens_delivered', updated_at = EXTRACT(EPOCH FROM NOW())::bigint
WHERE id = '0x...';
```

---

## Background Services

### RelayCoordinator (10s)

1. **Pending → Batched**: Calls `add_to_pending_batch` on source chain contract, adds commitment to in-memory Merkle tree
2. **Batched → NearSubmitted**: Advances once deposit address is set
3. **NearSubmitted → TokensDelivered**: Polls NEAR 1Click `/v0/status` until `SUCCESS`, verifies delivery tx on destination chain

### SettlementCoordinator (15s)

1. Verifies NEAR bridge completed
2. Verifies token delivery (ERC20 Transfer event or native ETH value to settlement contract)
3. Checks nullifier not already used
4. Generates and verifies Merkle proof off-chain
5. Decrypts ECIES params (nullifier, recipient)
6. Calls `settle_and_release` on destination chain
7. Calls `mark_settled` on source chain with retry

**Rescue path**: If intent stuck at `tokens_delivered` for >1 hour, falls back to `rescue_tokens` (owner key), bypassing Merkle proof.

### RootSyncCoordinator (30s)

Syncs Merkle roots cross-chain:
- StarkNet root → EVM contract (`syncMerkleRoot`)
- EVM root → StarkNet contract (`sync_merkle_root`)

Uses in-memory cache to skip redundant submissions when root hasn't changed.

---

## Merkle Trees

Two fixed-height trees (height = 20):

| Tree | Chain | Hash |
|------|-------|------|
| `starknet_commitments` | StarkNet source intents | Poseidon |
| `evm_commitments` | EVM source intents | Keccak256 |

Both use **sorted pair hashing** to match on-chain contract implementations.

---

## NEAR 1Click Asset IDs

| Asset | NEAR ID |
|-------|---------|
| STRK on StarkNet | `nep141:starknet.omft.near` |
| ETH on Ethereum | `nep141:eth.omft.near` |
| USDT on Ethereum | `nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near` |
| USDT on BSC | `nep245:v2_1.omni.hot.tg:56_2CMMyVTGZkeyNZTSvS5sarzfir6g` |

> **Note**: Ensure `EVM_RPC_URL` points to the correct chain for the asset being settled.

---

## API

### `POST /api/v1/bridge/initiate`

Authenticated via HMAC-SHA256 (`x-timestamp` + `x-signature` headers, ±300s tolerance).

**Body:**

```json
{
  "intent_id":           "0x<64 hex>",
  "commitment":          "0x<64 hex>",
  "nullifier_hash":      "0x<64 hex>",
  "view_key":            "0x<64 hex>",
  "near_intents_id":     "<UUID from NEAR correlationId>",
  "source_chain":        "starknet",
  "dest_chain":          "evm",
  "encrypted_recipient": "0x<ecies utf8 encrypted recipient>",
  "token":               "0x<source token address>",
  "amount":              "<decimal string, smallest unit>",
  "deposit_address":     "<NEAR deposit address>",
  "encrypted_secret":    "0x<ecies encrypted secret bytes>",
  "encrypted_nullifier": "0x<ecies encrypted nullifier bytes>"
}
```

> `near_intents_id` **must** be the `correlationId` UUID from NEAR `/v0/quote`. A random 32-byte hex will overflow StarkNet felt252.

---

## Deployment (Koyeb)

| Service | Type | Dockerfile |
|---------|------|------------|
| settlement-relayer | Worker | `packages/backend/Dockerfile` |

Build context: **repo root** (not `packages/backend/`).

Set all environment variables as Koyeb secrets.
---

## Security Notes

- ECIES private key (`RELAYER_PRIVATE_KEY`) decrypts nullifier and recipient at settlement time only — never stored in plaintext in DB
- Nullifier uniqueness enforced both off-chain (DB check) and on-chain (contract reverts on reuse)
- HMAC secret is to never be stored or persisted on client/frontend localStorage or memory, set in .env, and be careful to not push the env file to github.
- Owner private key (`EVM_OWNER_PRIVATE_KEY`) is only used for rescue path after 1hr timeout