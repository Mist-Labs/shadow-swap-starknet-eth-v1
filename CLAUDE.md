# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Shadow Swap is a privacy-preserving cross-chain swap protocol using NEAR Intents for liquidity routing. Users deposit on EVM chains, commitments are batched into a Merkle tree, and settlement happens on StarkNet with ZK proof verification.

## Monorepo Structure

pnpm workspace (pnpm v9) with packages under `packages/`:
- **packages/contracts/evm** — Solidity contracts (Foundry/Forge), the EVM side where users deposit and commitments are batched
- **packages/contracts/starknet/shadow_swap** — Cairo contracts (Scarb + snforge), the StarkNet settlement side that verifies proofs and processes claims
- **packages/backend** — Rust relayer service (Cargo, Rust 2024 edition, tokio + serde), currently scaffolded

## Build & Test Commands

### Root (runs across all packages)
```
pnpm build
pnpm test
pnpm lint
```

### EVM Contracts (from `packages/contracts/evm/`)
```
forge build                          # compile
forge test                           # run all tests
forge test --match-test test_Name    # run single test
forge test -vvv                      # verbose output (CI default)
forge fmt                            # format solidity
forge fmt --check                    # check formatting (CI uses this)
forge snapshot                       # gas snapshots
forge build --sizes                  # contract sizes
```

Foundry profiles: `default`, `ci` (verbosity=4, fuzz runs=10000), `lite` (fuzz runs=32).
Solidity version: 0.8.29, EVM target: paris, optimizer: 200 runs.
Gas reports configured for `ShadowIntentPool`.

### StarkNet Contracts (from `packages/contracts/starknet/shadow_swap/`)
```
scarb build                          # compile Cairo contracts
snforge test                         # run all tests
snforge test -f test_name            # run single test
```

Cairo edition: 2024_07, starknet dependency: 2.13.1, snforge_std: 0.53.0.

### Backend (from `packages/backend/`)
```
cargo build                          # compile
cargo test                           # run tests
cargo run                            # run service
```

## Architecture

### Cross-Chain Flow
1. **EVM (Source)**: User's intent is submitted by an authorized relayer to `ShadowIntentPool.addToPendingBatch()` with a privacy commitment, NEAR Intents ID, and optional view key
2. **Batching**: Commitments accumulate in a pending batch; processed automatically when batch is full (default 10) or after timeout (default 30s via `processBatchIfTimeout()` — callable by anyone)
3. **Merkle Tree**: On batch processing, commitments are inserted into an incremental Merkle tree (height 20, ~1M leaves), producing a new root
4. **Root Sync**: Relayer syncs the EVM Merkle root to StarkNet via `ShadowSettlement.sync_merkle_root()`
5. **StarkNet (Settlement)**: Claims are batched similarly; `_execute_claim()` verifies Merkle proofs against the synced root, checks nullifier hashes (Poseidon), and marks intents as claimed

### Key Contracts
- **ShadowIntentPool.sol** — EVM-side pool with incremental Merkle tree, batched commitment processing, nullifier tracking, and view key indexing. Uses keccak256 for Merkle tree nodes.
- **PoseidonHasher.sol** — EVM Poseidon hash (BN254 curve) for privacy commitments. Commitment = Poseidon(secret, nullifier, amount, destChain).
- **ShadowSettlement (Cairo)** — StarkNet-side settlement that registers intents, verifies Merkle proofs, processes batched claims with encrypted recipients, and uses native Poseidon (STARK field) for nullifier hashing.
- **PoseidonHasher (Cairo)** — StarkNet Poseidon wrapper using native `core::poseidon::PoseidonTrait`.

### Cross-Chain Hashing Note
EVM uses Poseidon over BN254 for commitments and keccak256 for Merkle tree nodes. StarkNet uses native Poseidon over the STARK field. Commitments are generated client-side using EVM BN254 Poseidon; Merkle roots are synced as opaque values.

### Access Control
- **Relayer**: Authorized addresses that can submit commitments, mark settlements, and sync roots. Managed by owner via `setRelayerStatus()`.
- **Owner**: Can update batch config and manage relayers.
- **Anyone**: Can trigger `processBatchIfTimeout()` when timeout expires.

### Privacy Model
- View keys allow users to track their own swaps without revealing them publicly
- Nullifier hashes prevent double-spending
- Batching provides anonymity set (commitments are mixed in batches)
- Encrypted recipients on StarkNet side (`EncryptedRecipient` struct with ciphertext + nonce)

## Environment Variables
EVM deployment requires: `SEPOLIA_RPC_URL`, `MAINNET_RPC_URL`, `ETHERSCAN_API_KEY` (see `foundry.toml` rpc_endpoints/etherscan sections).
