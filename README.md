# Shadow Swap — Starknet ↔ Ethereum Privacy Bridge

> **Privacy-preserving, intent-based cross-chain bridge between Ethereum Mainnet and StarkNet Mainnet, powered by NEAR 1Click Intents.**

Users bridge tokens without revealing the link between source and destination wallets. All sensitive parameters (secret, nullifier, recipient address) are ECIES-encrypted client-side. The relayer only decrypts at settlement time.

---

## Architecture

```
packages/
├── frontend/      Next.js 16 app — bridge UI, activity tracking, docs
├── backend/       Rust (Actix-web) relayer — intent batching, proving, settling
├── contracts/     Solidity (EVM) + Cairo (StarkNet) settlement contracts
└── indexers/      Chain indexers for intent event detection
```

### How It Works

1. **User** generates privacy params (secret, nullifier, commitment) client-side and submits a bridge intent
2. **NEAR 1Click** provides a cross-chain swap quote; the `correlationId` is used as `near_intents_id`
3. **Relayer** batches commitments on-chain, watches NEAR, and auto-settles on the destination chain
4. **Recipient address** is ECIES-encrypted — never transmitted in plaintext

---

## Contract Addresses (Mainnet)

| Chain | Contract | Address |
|---|---|---|
| Ethereum Mainnet | ShadowSettlement | `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468` |
| StarkNet Mainnet | ShadowSwapSettlement | `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114` |

---

## Getting Started

### Prerequisites

| Tool | Version |
|---|---|
| Node.js | ≥ 20 |
| pnpm | ≥ 9 |
| Rust | ≥ 1.70 (backend only) |

### Install

```bash
git clone https://github.com/Mist-Labs/shadow-swap-starknet-eth-v1.git
cd shadow-swap-starknet-eth-v1
pnpm install
```

### Run the Frontend

```bash
cd packages/frontend
cp .env.example .env   # fill in required values (see below)
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000).

### Run the Backend

```bash
cd packages/backend
cp .env.example .env   # fill in required values
cargo run --release
```

---

## Environment Variables

### Frontend (`packages/frontend/.env`)

| Variable | Exposure | Description |
|---|---|---|
| `NEXT_PUBLIC_RELAYER_URL` | Public | Relayer API base URL |
| `NEXT_PUBLIC_RELAYER_PUBKEY` | Public | Relayer secp256k1 public key (no `0x`) |
| `NEXT_PUBLIC_NEAR_API_KEY` | Public | NEAR 1Click API key |
| `NEXT_PUBLIC_REOWN_PROJECT_ID` | Public | Reown / WalletConnect project ID |
| `NEXT_PUBLIC_EVM_SETTLEMENT` | Public | Ethereum settlement contract address |
| `NEXT_PUBLIC_STARKNET_SETTLEMENT` | Public | StarkNet settlement contract address |
| `NEXT_PUBLIC_STARKNET_RPC_URL` | Public | StarkNet RPC endpoint |
| `HMAC_SECRET` | **Server-only** | HMAC signing secret (never expose to browser) |

### Backend (`packages/backend/.env`)

| Variable | Description |
|---|---|
| `HMAC_SECRET` | Must match frontend `HMAC_SECRET` |
| `EVM_RPC_URL` | Ethereum JSON-RPC endpoint |
| `EVM_PRIVATE_KEY` | Relayer EVM private key |
| `STARKNET_RPC_URL` | StarkNet RPC endpoint |
| `STARKNET_PRIVATE_KEY` | Relayer StarkNet private key |
| `RELAYER_PRIVATE_KEY` | Internal relayer signing key |

---

## Packages

### `packages/frontend`
Next.js 16 application. See [`packages/frontend/README.md`](packages/frontend/README.md) for full details.

### `packages/backend`
Rust/Actix-web relayer service handling:
- Intent lifecycle management (pending → batched → settled → marked_settled)
- NEAR 1Click integration
- HMAC-authenticated REST API
- Metrics endpoint (`GET /api/v1/metrics`)

### `packages/contracts`
- **EVM:** Solidity settlement contract (verified on Etherscan)
- **StarkNet:** Cairo settlement contract (SN_MAIN)

### `packages/indexers`
Chain event indexers for detecting deposits and fills on both chains.

---

## Intent Lifecycle

```
pending → batched → near_submitted → tokens_delivered → settled → marked_settled ✅
                                                                 ↘ failed / refunded
```

| Status | Meaning |
|---|---|
| `pending` | Intent received by relayer |
| `batched` | Commitment posted on-chain |
| `near_submitted` | Watching NEAR (may take 1–5 min due to indexer latency) |
| `tokens_delivered` | NEAR swap complete |
| `settled` | Destination chain settlement in progress |
| `marked_settled` | **Final — funds delivered** |
| `failed` / `refunded` | Terminal error state |

---

## API Reference

Relayer base URL: `https://appropriate-chelsea-mist-labs-1f0a1134.koyeb.app/api/v1`

| Endpoint | Auth | Description |
|---|---|---|
| `POST /bridge/initiate` | HMAC | Submit bridge intent |
| `GET /bridge/intent/{id}` | None | Fetch intent status |
| `GET /bridge/intents?status=&limit=` | None | List intents |
| `GET /health` | None | Basic health check |
| `GET /metrics` | HMAC | System metrics (operators) |

HMAC authentication: `x-timestamp` + `x-signature` headers. Signature = `HMAC-SHA256(secret, timestamp + body)`.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | Next.js 16, React 19, TailwindCSS, shadcn/ui, Framer Motion |
| Wallet (EVM) | wagmi v3, Reown AppKit |
| Wallet (StarkNet) | starknet-react, StarknetKit |
| Backend | Rust, Actix-web |
| Encryption | ECIES (eciesjs), Keccak256, Poseidon |
| Cross-chain | NEAR 1Click Intents |
| Monorepo | pnpm workspaces |

---

## License

[GPL-3.0](LICENSE)
