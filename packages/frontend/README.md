# Shadow Swap — Frontend

Next.js 16 frontend for the Shadow Swap privacy bridge (Ethereum ↔ StarkNet).

## Stack

- **Framework:** Next.js 16 (App Router), React 19
- **Styling:** TailwindCSS v3, shadcn/ui, Framer Motion
- **EVM Wallet:** wagmi v3, Reown AppKit (WalletConnect)
- **StarkNet Wallet:** starknet-react v5, StarknetKit
- **State / Data:** TanStack Query v5
- **Encryption:** eciesjs (ECIES), js-sha3 (Keccak256), starknet (Poseidon)
- **Package Manager:** pnpm

## Getting Started

```bash
# From monorepo root
pnpm install

# Start dev server
cd packages/frontend
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000).

## Scripts

| Command | Description |
|---|---|
| `pnpm dev` | Start development server |
| `pnpm build` | Production build (webpack) |
| `pnpm start` | Start production server |
| `pnpm typecheck` | TypeScript type check |
| `pnpm lint` | ESLint |
| `pnpm lint:fix` | ESLint with auto-fix |
| `pnpm format` | Prettier format |

## Environment Variables

Create a `.env` file in `packages/frontend/`:

```env
# Public — safe to expose to browser
NEXT_PUBLIC_RELAYER_URL=https://appropriate-chelsea-mist-labs-1f0a1134.koyeb.app/api/v1
NEXT_PUBLIC_RELAYER_PUBKEY=<secp256k1 pubkey, no 0x prefix>
NEXT_PUBLIC_NEAR_API_KEY=<NEAR 1Click API key>
NEXT_PUBLIC_REOWN_PROJECT_ID=<Reown / WalletConnect project ID>
NEXT_PUBLIC_EVM_SETTLEMENT=0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468
NEXT_PUBLIC_STARKNET_SETTLEMENT=0x07576cc5d7cd8f2cf82572a4b7bddeb2eac7de872cdfed575eff399c3ce86114
NEXT_PUBLIC_STARKNET_RPC_URL=<StarkNet mainnet RPC URL>

# Server-only — never expose to browser
HMAC_SECRET=<shared secret with backend relayer>
```

## Project Structure

```
app/
├── page.tsx              Landing page
├── bridge/               Bridge UI
├── activity/             Transaction history
├── stats/                Protocol stats
├── docs/                 Documentation
└── api/
    ├── bridge/           Proxy → POST /bridge/initiate (HMAC signed)
    ├── intents/          Proxy → GET /bridge/intents  (fan-out across statuses)
    ├── intent/[id]/      Proxy → GET /bridge/intent/{id}
    ├── stats/            Proxy → GET /metrics (HMAC signed)
    └── health/           Proxy → GET /health

components/
├── bridge/               BridgeForm, BridgeProgress, RecentActivity
├── landing/              HeroSection, stats panels
└── shared/               Header, Footer

hooks/
├── useBridge.ts          Bridge submission logic
├── useBridgeIntents.ts   Intent list fetching + formatTimeAgo
├── useBridgeStats.ts     Stats + health polling
└── useIntentStatus.ts    Single intent polling

lib/
├── api.ts                API client (all backend calls)
├── tokens.ts             Supported tokens + addresses
└── contracts.ts          Contract addresses + explorer URLs
```

## API Proxy Pattern

All backend calls go through Next.js API routes (`app/api/`). This keeps:
- `HMAC_SECRET` server-side only (never in browser bundles)
- Backend URL resolution server-side
- HMAC-SHA256 signing using Node.js `crypto` module

HMAC signing: `HMAC-SHA256(HMAC_SECRET, timestamp + body)` where `body = ""` for GET requests.

## Supported Tokens

| Symbol | Ethereum Address | StarkNet Address |
|---|---|---|
| ETH | Native | `0x049d36570d...` |
| USDC | `0xA0b8699...` | `0x053c9125...` |
| USDT | `0xdAC17F9...` | `0x068f5c6a...` |

## Requirements

- Node.js ≥ 20
- pnpm ≥ 9
