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
          │  RootSyncCoordinator     │  event-driven + 5min fallback
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
                                                      ↘ settlement_failed (after retries exhausted)
                                                      ↘ rescue path (after 1hr → settled, no Merkle proof)
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
| StarkNet Mainnet | ShadowSwapSettlement | `0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0` |

---

## Environment Variables

```env
# ── Database ──────────────────────────────────────────────────────
DATABASE_URL=postgresql://user:password@host/dbname
DATABASE_MAX_CONNECTIONS=20

# ── EVM (Ethereum Mainnet) ────────────────────────────────────────
EVM_RPC_URL=https://eth-mainnet_RPC_URL_
EVM_SETTLEMENT_ADDRESS=0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468
EVM_PRIVATE_KEY=0x...          # Relayer wallet — must be authorized on contract
EVM_OWNER_PRIVATE_KEY=0x...    # Owner wallet — for rescue_tokens fallback only

# ── StarkNet ──────────────────────────────────────────────────────
STARKNET_RPC_URL=https://starknet-mainnet.g.alchemy.com/starknet/version/rpc/v0_10/YOUR_KEY
STARKNET_CONTRACT_ADDRESS=0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0
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

### `shadow_intents` Schema (key columns)

| Column | Notes |
|--------|-------|
| `id` | Intent ID (felt252 hex) |
| `commitment` | Merkle leaf |
| `nullifier_hash` | Public nullifier |
| `encrypted_nullifier` | ECIES-encrypted nullifier, decrypted at settlement |
| `encrypted_recipient` | ECIES-encrypted recipient address |
| `token` | Source token address |
| `dest_token` | Destination token address (optional — if set, AVNU swap is performed) |
| `amount` | Amount in smallest unit |
| `status` | See lifecycle above |
| `deposit_address` | NEAR deposit address for 1Click polling |

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
2. Verifies token delivery (ERC20 Transfer event to settlement contract)
3. Checks nullifier not already used
4. Generates and verifies Merkle proof off-chain
5. Decrypts ECIES params (nullifier, recipient)
6. If `dest_token` differs from delivered token, fetches AVNU quote and builds swap calldata via `/swap/v2/build`
7. Calls `settle_and_release` on destination chain (with or without AVNU swap)
8. Calls `mark_settled` on source chain with retry

**Rescue path**: If intent stuck at `tokens_delivered` for >1 hour, falls back to `rescue_tokens` (owner key), bypassing Merkle proof. Delivers the received token as-is without swap.

### RootSyncCoordinator (event-driven + 5min fallback)

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

## AVNU Swap Integration

When `dest_token` is set and differs from the delivered token, the settlement coordinator:

1. Calls `/swap/v2/quotes` to get `buyAmount` and `quoteId`
2. Calls `/swap/v2/build` with `takerAddress` = settlement contract to get ready-made calldata
3. Extracts route felts from index 11 onwards of the `multi_route_swap` calldata
4. Passes them verbatim to `settle_and_release` — the contract forwards them to AVNU's exchange

> **Note**: NEAR always delivers STRK on the StarkNet path. Any other delivered token is rejected. The swap is STRK → dest_token.

---

## NEAR 1Click Asset IDs

| Asset | NEAR ID |
|-------|---------|
| STRK on StarkNet | `nep141:starknet.omft.near` |
| ETH on Ethereum | `nep141:eth.omft.near` |
| USDT on Ethereum | `nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near` |
| USDC on Ethereum | `nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near` |


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
  "dest_token":          "0x<destination token address, optional>",
  "amount":              "<decimal string, smallest unit>",
  "deposit_address":     "<NEAR deposit address>",
  "encrypted_secret":    "0x<ecies encrypted secret bytes>",
  "encrypted_nullifier": "0x<ecies encrypted nullifier bytes>"
}
```

> **`near_intents_id` must be the `correlationId` UUID from NEAR `/v0/quote`** — e.g. `8200e1f8-02db-4c4d-96ff-e7fb2eacf1e7`. This field is used as a string identifier for NEAR polling and is never converted to felt252. Do **not** pass a random 32-byte hex here — it has no meaning to the NEAR 1Click API and the intent will never resolve.

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
- HMAC secret must never be stored or persisted on client/frontend localStorage or memory — set in `.env` only, never commit to git
- Owner private key (`EVM_OWNER_PRIVATE_KEY`) is only used for the rescue path after 1hr timeout