# shadow-swap-starknet-eth-v1

Privacy-preserving cross-chain bridge between StarkNet and Ethereum, powered by NEAR Intents. Users swap tokens across chains without linking their source and destination addresses.

---

## How It Works

1. **User commits** — Frontend generates a cryptographic commitment (Poseidon/Keccak256 hash of secret, nullifier, amount, token, destination chain). All sensitive params are ECIES-encrypted client-side.
2. **Relayer batches** — Commitment is added to an on-chain incremental Merkle tree on the source chain.
3. **NEAR bridges** — User sends tokens to a NEAR 1Click deposit address. NEAR Intents swaps and delivers the destination token to the settlement contract on the destination chain.
4. **Relayer settles** — After verifying delivery and Merkle proof off-chain, relayer calls `settle_and_release` — destination tokens are sent to the encrypted recipient address.
5. **Source marked** — Source chain commitment is marked settled, preventing replay.

```
User Wallet
    │
    ├── source chain tx → ShadowSettlement (source)
    │                         │
    │                    add_to_pending_batch()
    │                    [Merkle root updated]
    │
    ├── send tokens → NEAR 1Click deposit address
    │                         │
    │                    NEAR Intents swap
    │                         │
    │                    deliver to ShadowSettlement (dest)
    │
    └── relayer calls settle_and_release()
                              │
                         tokens → recipient (private)
```

---

## Monorepo Structure

```
shadow-swap-starknet-eth-v1/
├── packages/
│   ├── backend/              # Rust settlement relayer
│   ├── indexers/
│   │   ├── starknet/         # Apibara v2 StarkNet indexer
│   │   └── evm/              # Goldsky webhook EVM indexer
│   └── contracts/            # Cairo + Solidity contracts (separate repo)
├── pnpm-workspace.yaml
└── README.md
```

---

## Packages

### `packages/backend` — Settlement Relayer (Rust)
Actix-web HTTP server + background services. Handles the full intent lifecycle: batching, NEAR polling, Merkle proof verification, and on-chain settlement.

→ See [packages/backend/README.md](packages/backend/README.md)

### `packages/indexers/starknet` — StarkNet Indexer (TypeScript / Apibara v2)
Streams StarkNet contract events into PostgreSQL in real-time. Forwards commitment and settlement events to the relayer via HMAC-authenticated webhooks.

→ See [packages/indexers/starknet/README.md](packages/indexers/starknet/README.md)

### `packages/indexers/evm` — EVM Indexer (TypeScript / Goldsky)
Express webhook server that receives Goldsky event callbacks for the Ethereum settlement contract and forwards them to the relayer.

→ See [packages/indexers/evm/README.md](packages/indexers/evm/README.md)

---

## Contracts

| Chain | Contract | Address |
|-------|----------|---------|
| Ethereum Mainnet | ShadowSettlement | `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468` |
| StarkNet Mainnet | ShadowSwapSettlement | `0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114` |

---

## Supported Routes

| Source | Destination | Via NEAR |
|--------|-------------|----------|
| STRK (StarkNet) | ETH (Ethereum) | `nep141:eth.omft.near` |
| STRK (StarkNet) | USDT (Ethereum) | `nep245:v2_1.omni.hot.tg:56_2CMMyVTGZkeyNZTSvS5sarzfir6g` |
| USDT (Ethereum) | STRK (StarkNet) | `nep141:starknet.omft.near` |
| ETH (Ethereum) | STRK (StarkNet) | `nep141:starknet.omft.near` |

---

## Privacy Model

- **Zero plaintext transmission** — secret, nullifier, and recipient address are ECIES-encrypted by the frontend before leaving the browser
- **Relayer decrypts at settlement only** — private key never touches the database
- **Commitment binding** — 5-parameter hash (secret, nullifier, amount, token, destChain) prevents commitment reuse across different swaps
- **Nullifier uniqueness** — enforced both off-chain (DB) and on-chain (contract reverts on reuse)
- **No ZK proof on-chain** — Merkle proof is verified off-chain by the trusted relayer

---

## Development

### Prerequisites

- Rust stable
- Node.js 20+
- pnpm 8+
- PostgreSQL (or Neon)

### Install

```bash
pnpm install
```

### Run Services

```bash
# Relayer
cd packages/backend && cargo run

# StarkNet indexer
cd packages/indexers/starknet && pnpm dev

# EVM indexer
cd packages/indexers/evm && pnpm dev
```

---

## Docker (Koyeb / Production)

All services build from the **repo root** — monorepo build context is required.

```bash
# Relayer
docker build -t shadow-settlement-relayer -f packages/backend/Dockerfile .

# StarkNet indexer (deploy as Worker)
docker build -t shadow-starknet-indexer -f packages/indexers/starknet/Dockerfile .

# EVM indexer (deploy as Web Service)
docker build -t shadow-evm-indexer -f packages/indexers/evm/Dockerfile .
```

| Service | Koyeb Type | Health Check |
|---------|-----------|--------------|
| settlement-relayer | Worker | `GET /health` |
| starknet-indexer | Worker | None |
| evm-indexer | Web Service | `GET /health` |

---

## Intent Lifecycle

```
pending → batched → near_submitted → tokens_delivered → settled → marked_settled ✅
                                                      ↘ rescue (after 1hr timeout)
```

Terminal states: `marked_settled`, `failed`, `refunded`, `settlement_failed`

---

Built by Mist-Labs