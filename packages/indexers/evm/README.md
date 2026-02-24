# ShadowSwap Indexer (EVM)

Goldsky webhook indexer for ShadowSwap privacy bridge on Ethereum/Sepolia.

## Architecture

```
EVM Contract → Goldsky → Webhook Server → Backend Relayer
```

- **Goldsky**: Indexes EVM events in real-time
- **Webhook Server**: Receives events, transforms, authenticates with HMAC
- **Backend**: Processes events and updates database

## Events Indexed

- `commitment_added` - When commitment added to Merkle tree
- `settled` - When settlement completes on destination chain
- `marked_settled` - When settlement marked on source chain
- `merkle_root_updated` - When Merkle root syncs

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

### 3. Build

```bash
npm run build
```

### 4. Deploy to Goldsky

```bash
./deploy.sh
```

### 5. Configure Webhook in Goldsky Dashboard

- Webhook URL: `https://your-domain.com/webhook`
- Add header: `goldsky-webhook-secret: your_secret`
- Select events: All from `ShadowSettlement` contract

### 6. Deploy Webhook Server

Deploy to your hosting platform (Railway, Render, Fly.io, etc.)

Environment variables needed:
- `PORT=3000`
- `GOLDSKY_WEBHOOK_SECRET=...`
- `RELAYER_BASE_URL=https://your-backend.com/api/v1`
- `HMAC_SECRET=...` (must match backend)

## Local Development

```bash
npm run dev
```

Test webhook:
```bash
curl -X POST http://localhost:3000/webhook \
  -H "goldsky-webhook-secret: your_secret" \
  -H "Content-Type: application/json" \
  -d '{...}'
```

## Deployment

### Railway
```bash
railway up
```

### Docker
```bash
docker build -t shadowswap-indexer .
docker run -p 3000:3000 --env-file .env shadowswap-indexer
```

## Health Check

```bash
curl http://localhost:3000/health
```

## Contract Addresses

- **Ethereum**: `0x239365b4A26d947B9CE38ec88A804F03b8248042`
- **Sepolia**: `0x239365b4A26d947B9CE38ec88A804F03b8248042`
