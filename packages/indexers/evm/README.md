# ShadowSwap Indexer (EVM)

Goldsky webhook indexer for the ShadowSwap privacy bridge on Ethereum Mainnet. Receives real-time contract events via Goldsky, authenticates them, and forwards to the settlement relayer.

## Architecture

```
EVM Contract → Goldsky → Webhook Server → Backend Relayer
```

- **Goldsky**: Indexes EVM contract events in real-time and pushes to webhook
- **Webhook Server**: Express server — validates Goldsky auth, deduplicates, transforms, and forwards events to relayer via HMAC-signed requests
- **Relayer**: Processes events and updates intent/commitment state in database

## Events Indexed

| Entity | Description |
|--------|-------------|
| `commitment_added` | Commitment added to on-chain Merkle tree |
| `intent_settled` | Settlement completed on destination chain |
| `intent_marked_settled` | Settlement marked on source chain |
| `merkle_root_updated` | Merkle root updated after batch processed |
| `batch_processed` | Pending batch sealed on-chain |

## Contract Addresses

| Network | Address |
|---------|---------|
| Ethereum Mainnet | `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468` |

## Setup

### 1. Install Dependencies

```bash
npm install
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env with your values
```

Required environment variables:

```env
PORT=3000
WEBHOOK_URL=https://your-webhook-domain.com/webhook   # Public URL of this server — used when registering with Goldsky
GOLDSKY_WEBHOOK_SECRET=...                            # From Goldsky dashboard — used to authenticate incoming webhooks
RELAYER_BASE_URL=https://your-backend.com/api/v1
HMAC_SECRET=...                                       # Must match backend HMAC_SECRET exactly
```

### 3. Build

```bash
npm run build
```

### 4. Deploy Webhook Server

Deploy as a **Web Service** (not Worker) — requires the `/health` endpoint to be reachable for health checks.

#### Koyeb

Build from repo root (monorepo):

```bash
docker build -t shadowswap-evm-indexer -f packages/indexers/evm/Dockerfile .
```

- Dockerfile location: `packages/indexers/evm/Dockerfile`
- Deploy as: **Web Service**
- Health check: `GET /health`

#### Docker

```bash
# Build from repo root
docker build -t shadowswap-evm-indexer -f packages/indexers/evm/Dockerfile .
docker run -p 3000:3000 --env-file .env shadowswap-evm-indexer
```

#### Railway / Render / Fly.io

```bash
railway up
```

### 5. Configure Goldsky Webhook

In the Goldsky dashboard:

- **Webhook URL**: `https://your-domain.com/webhook`
- **Auth header**: `goldsky-webhook-secret: your_secret`
- **Contract**: `ShadowSettlement` at `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468`
- **Events**: Select all (commitment_added, intent_settled, intent_marked_settled, merkle_root_updated, batch_processed)

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/webhook` | Receives Goldsky event payloads |
| `GET` | `/health` | Health check — returns `{ status: "ok" }` |

### Webhook Auth

Goldsky sends a `goldsky-webhook-secret` header with each request. The server validates this against `GOLDSKY_WEBHOOK_SECRET` before processing.

### Relayer Forwarding

Events are forwarded to the relayer with HMAC-SHA256 signing:

```
x-signature: <hmac-sha256(HMAC_SECRET, timestamp + body)>
x-timestamp:  <unix timestamp>
```

The relayer validates these headers using the shared `HMAC_SECRET`.

## Deduplication

In-memory deduplication cache with 1-hour TTL prevents duplicate events from being forwarded to the relayer if Goldsky delivers the same event more than once.

## Local Development

```bash
npm run dev
```

Test webhook locally:

```bash
curl -X POST http://localhost:3000/webhook \
  -H "goldsky-webhook-secret: your_secret" \
  -H "Content-Type: application/json" \
  -d '{"event": "commitment_added", "data": {...}}'
```

Health check:

```bash
curl http://localhost:3000/health
```