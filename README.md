# shadow-swap-starknet-eth-v1

Privacy-preserving cross-chain bridge between StarkNet and Ethereum, powered by NEAR Intents. Users swap tokens across chains without linking their source and destination addresses.

---

## How It Works

1. **User commits** — Frontend generates a cryptographic commitment (Poseidon/Keccak256 hash of secret, nullifier, amount, token, destination chain). All sensitive params are ECIES-encrypted client-side. If the user's source token is not the one NEAR expects (e.g. USDC or ETH on StarkNet instead of STRK), the frontend first performs an AVNU multi-hop swap to acquire the required token before submitting the commitment.
2. **Relayer batches** — Commitment is added to an on-chain incremental Merkle tree on the source chain.
3. **NEAR bridges** — User sends tokens to a NEAR 1Click deposit address. NEAR Intents swaps and delivers the destination token to the settlement contract on the destination chain.
4. **Relayer settles** — After verifying delivery and Merkle proof off-chain, relayer calls `settle_and_release`. If the user requested a destination token different from what NEAR delivered (e.g. USDC instead of STRK), the settlement contract performs a multi-hop swap via AVNU before releasing to the recipient. Destination tokens are sent to the encrypted recipient address.
5. **Source marked** — Source chain commitment is marked settled, preventing replay.

```
User Wallet
    │
    ├── [optional AVNU swap: USDC/ETH → STRK on source chain]
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
                         [optional AVNU swap: STRK → dest_token]
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
Actix-web HTTP server + background services. Handles the full intent lifecycle: batching, NEAR polling, Merkle proof verification, AVNU swap routing, and on-chain settlement.

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
| StarkNet Mainnet | ShadowSwapSettlement | `0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0` |

---

## Supported Routes

### Supported Tokens

| Chain | Supported source tokens | Supported destination tokens |
|-------|------------------------|------------------------------|
| StarkNet | STRK, USDC, ETH, USDT | STRK, USDC, ETH, USDT |
| Ethereum | ETH, USDT, USDC | ETH, USDT, USDC |

### AVNU Swap Rules

AVNU multi-hop swaps occur at two points depending on the route:

**Frontend (source side)** — NEAR 1Click requires STRK as the input token on StarkNet. If the user holds any other token, the frontend swaps to STRK via AVNU before submitting the commitment:

| Source token | Frontend swap |
|-------------|---------------|
| STRK | None |
| USDC, ETH, USDT (StarkNet) | → STRK via AVNU |
| ETH, USDT, USDC (Ethereum) | None (NEAR accepts directly) |

**Settlement contract (destination side)** — NEAR always delivers STRK to the StarkNet settlement contract. If the user requested a different destination token, the contract swaps after delivery:

| Destination token | Settlement swap |
|------------------|-----------------|
| STRK | None |
| USDC, ETH, USDT (StarkNet) | STRK → dest via AVNU |
| Any token (Ethereum) | None (NEAR delivers directly) |

> New tokens can be added without contract changes — whitelist the token on-chain and the relayer fetches the AVNU route automatically at settlement time.

---

## Privacy Model

- **Zero plaintext transmission** — secret, nullifier, and recipient address are ECIES-encrypted by the frontend before leaving the browser
- **Relayer decrypts at settlement only** — private key never touches the database
- **Commitment binding** — 5-parameter hash (secret, nullifier, amount, token, destChain) prevents commitment reuse across different swaps
- **Nullifier uniqueness** — enforced both off-chain (DB) and on-chain (contract reverts on reuse)
- **No ZK proof on-chain** — Merkle proof is verified off-chain by the trusted relayer
- **AVNU swap privacy (destination)** — post-delivery swap is performed by the settlement contract, not the user's wallet — recipient address is never exposed on-chain
- **AVNU swap privacy (source)** — pre-bridge swap is performed by the user's wallet on the source chain, but is unlinked from the destination since the commitment and recipient are ECIES-encrypted before submission

---

## View Key & Transaction History

Users can retrieve their full bridge history without exposing their wallet address on-chain. A **view key** is derived deterministically from the wallet address and a domain string — it is stored with each commitment at submission time and can be used to paginate history via `get_intents_by_view_key` on the StarkNet contract.

### Derivation

```typescript
import { keccak256 } from 'js-sha3';
import { poseidonHashMany } from 'starknet';

const DOMAIN = "YOUR DOMAIN";

function deriveViewKey(walletAddress: string, sourceChain: "evm" | "starknet"): string {
  if (sourceChain === "evm") {
    return "0x" + keccak256(walletAddress.toLowerCase() + DOMAIN);
  } else {
    const addrFelt   = BigInt(walletAddress);
    const domainFelt = BigInt("0x" + Buffer.from(DOMAIN).toString("hex"));
    return "0x" + poseidonHashMany([addrFelt, domainFelt]).toString(16).padStart(64, "0");
  }
}
```

- **During swap** — view key is derived directly from the connected wallet address, no signature required
- **During history view (wallet disconnected)** — user signs a message to prove ownership before the frontend derives the view key

### Fetching History

```typescript
// Fetch first page (20 intents)
const [intents, total] = await contract.get_intents_by_view_key(viewKey, 0, 20);

// Fetch next page
const [page2, _] = await contract.get_intents_by_view_key(viewKey, 20, 20);
```

Each `IntentDetail` contains `commitment`, `near_intents_id`, `submitted_at`, and `settled` status. The contract returns an empty array for unknown view keys — intentionally indistinguishable from a key with no history, preventing probing.

> See the frontend integration doc for the full signature flow and view key derivation spec.

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
|---------|------------|--------------|
| settlement-relayer | Web Service | `GET /health` |
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