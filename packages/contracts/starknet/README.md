# ShadowSwapSettlement — StarkNet Contract

Cairo settlement contract for the ShadowSwap Privacy Bridge on StarkNet Mainnet. Handles commitment batching, cross-chain Merkle root syncing, nullifier-based settlement, and token release.

**Mainnet:** `0x06563b21751c9e9eb852e48b01fda8c66a2e2a2b93c1b13cc85c150f21e7f8d0`

---

## Overview

Users bridge tokens privately by committing a Poseidon hash on StarkNet. The relayer batches commitments into an incremental Merkle tree, syncs roots cross-chain, and calls `settle_and_release` after NEAR delivers tokens to this contract.

```
User → add_to_pending_batch(commitment)
     → [batch processed on-chain]
     → Merkle root updated

NEAR delivers tokens to this contract
     → Relayer calls settle_and_release(intent_id, nullifier, recipient, token, amount, dest_token, ...)
     → Contract verifies nullifier unused + remote root verified
     → Optionally swaps via AVNU (if dest_token != token)
     → Transfers token to recipient
```

---

## Commitment Scheme

Commitments are **5-parameter Poseidon hashes** — binding amount, token, and destination chain prevents reuse across swaps.

```python
# Poseidon over 5 felts
commitment     = poseidon_hash_many([secret, nullifier, amount, token, dest_chain])
nullifier_hash = poseidon_hash_many([nullifier])

# dest_chain values
# 1 = EVM (Ethereum)
# 2 = StarkNet
```

> **Security**: The 5-parameter scheme binds the commitment to a specific swap. A 2-parameter commitment (secret, nullifier only) is insecure — do not use it.

> **Field prime**: All felts must be < StarkNet field prime. Use 31-byte randoms for secret and nullifier. Never use a random 32-byte hex — it may overflow felt252.

---

## Merkle Tree

Fixed-height incremental Merkle tree (height = 20, max 1,048,576 leaves).

- **Hash function**: Poseidon with **sorted pair hashing** — matches Cairo `_hash_pair`
- **Zero hashes**: `zeros[0] = 0`, `zeros[i] = poseidon_hash_many([zeros[i-1], zeros[i-1]])`
- **Root sync**: Relayer pushes EVM's root to this contract via `sync_merkle_root`

```python
# Sorted pair hash — matches Cairo contract _hash_pair
def hash_pair(a, b):
    return poseidon_hash_many([a, b]) if a < b else poseidon_hash_many([b, a])
```

---

## Functions

### Source Side

#### `add_to_pending_batch(commitment: felt252, near_intents_id: felt252, view_key: felt252)`
Adds a commitment to the pending batch. Called by authorized relayer after user initiates a bridge.

- `commitment` — 5-parameter Poseidon hash
- `near_intents_id` — NEAR `correlationId` UUID as felt252 (UUID = 128-bit, fits felt252 — never use random 32-byte hex)
- `view_key` — deterministic per-user key for history lookup

#### `process_batch_if_timeout()`
Seals the current pending batch and updates the Merkle root. Called by relayer when batch is full or timeout elapsed.

#### `mark_settled(commitment: felt252, nullifier_hash: felt252)`
Marks a commitment as settled on the source chain after destination settlement completes.

---

### Cross-Chain Sync

#### `sync_merkle_root(chain_id: felt252, root: felt252, leaf_count: u256)`
Called by relayer to push a remote chain's Merkle root to this contract.

- `chain_id` — EVM chain identifier felt

#### `verify_remote_root(chain_id: felt252, snapshot_index: u256)`
Marks a previously synced remote root snapshot as verified. Required before `settle_and_release` can use it.

---

### Destination Side

#### `settle_and_release(intent_id, nullifier_hash, recipient, token, amount, dest_token, expected_dest_amount, min_dest_amount, routes)`

```python
settle_and_release(
    intent_id:            felt252,
    nullifier_hash:       felt252,
    recipient:            felt252,
    token:                felt252,       # delivered token address (always STRK from NEAR)
    amount:               u256,          # delivered amount
    dest_token:           felt252,       # 0 if no swap needed
    expected_dest_amount: u256,          # AVNU quote buy_amount; (0,0) if no swap
    min_dest_amount:      u256,          # hard slippage floor; (0,0) if no swap
    routes:               Array<Route>   # AVNU route calldata; empty array if no swap
)
```

Releases tokens to recipient. Called by authorized relayer after NEAR delivers tokens to this contract.

Checks:
1. Nullifier not already used
2. Remote EVM Merkle root verified
3. Token is whitelisted
4. Contract holds sufficient balance

If `dest_token` is non-zero and differs from `token`, the contract calls AVNU's `multi_route_swap` to swap the delivered token before releasing to the recipient.

```python
# No swap — deliver STRK directly
settle_and_release(intent_id, nullifier_hash, recipient, STRK, amount, 0, (0,0), (0,0), [])

# Swap STRK → USDC via AVNU
settle_and_release(intent_id, nullifier_hash, recipient, STRK, amount, USDC, buy_amount, min_amount, routes)
```

---

### View Functions

#### `get_merkle_root() → felt252`
Returns the current local Merkle root (all batched commitments).

#### `is_nullifier_used(nullifier_hash: felt252) → bool`
Returns true if nullifier has been spent.

#### `commitment_exists(commitment: felt252) → bool`
Returns true if commitment is in the tree.

#### `get_intent(commitment: felt252) → (felt252, u64, bool)`
Returns `(commitment, submitted_at, settled)` for a given commitment.

#### `get_pending_batch_info() → (u64, u64, u64)`
Returns `(count, batch_start_time, timeout_secs)` for the current pending batch.

#### `get_latest_remote_root(chain_id: felt252) → RemoteRootSnapshot`
Returns the most recently synced remote root snapshot for a given chain.

```
RemoteRootSnapshot {
    root:       felt252,
    leaf_count: u64,
    synced_at:  u64,
    verified:   bool,
}
```

#### `get_latest_verified_remote_root(chain_id: felt252) → (RemoteRootSnapshot, u64)`
Returns the latest verified snapshot and its index.

#### `get_remote_root_snapshot(chain_id: felt252, snapshot_index: u256) → RemoteRootSnapshot`
Returns a specific snapshot by index.

#### `get_remote_root_count(chain_id: felt252) → u64`
Returns total number of synced remote root snapshots.

#### `is_relayer_authorized(relayer: felt252) → bool`
Returns true if address is an authorized relayer.

#### `is_root_verifier(verifier: felt252) → bool`
Returns true if address can call `verify_remote_root`.

#### `is_token_whitelisted(token: felt252) → bool`
Returns true if token is approved for settlement.

#### `get_intents_by_view_key(view_key: felt252, offset: u64, limit: u64) → (Array<IntentDetail>, u64)`
Returns a paginated list of intents associated with a view key, and the total count.

- `view_key` — deterministic key derived from the user's wallet address (see integration doc for derivation)
- `offset` — starting index for pagination
- `limit` — max results to return; pass `0` to return all remaining from offset

Returns `(intents, total)` where `total` is the full count regardless of pagination.

```
IntentDetail {
    commitment:      felt252,  // 5-parameter Poseidon hash
    near_intents_id: felt252,  // NEAR correlationId
    submitted_at:    u64,      // Unix timestamp of submission
    settled:         bool,     // Whether intent has been settled
}
```

**Privacy note**: Returns an empty array with `total = 0` when the view key has no intents — intentionally indistinguishable from a view key that doesn't exist. This prevents probing to determine if a wallet has ever used the bridge.

**Frontend usage** — fetch a user's bridge history:
```typescript
// Derive view key (must match what was submitted at bridge initiation)
const viewKey = deriveViewKey(walletAddress, "starknet");

// Fetch first page
const [intents, total] = await contract.get_intents_by_view_key(viewKey, 0, 20);

// Fetch next page
const [page2, _] = await contract.get_intents_by_view_key(viewKey, 20, 20);
```

> Requires the caller to know the view key. The view key is derived deterministically from the wallet address — users must sign a message to prove ownership before the frontend derives and uses it for history lookup.

---

### Admin / Owner

#### `update_batch_config(new_batch_size: u64, new_timeout: u64)`
Update the max batch size and timeout before auto-processing.

#### `set_relayer_status(relayer: felt252, authorized: bool)`
Authorize or revoke a relayer address.

#### `set_root_verifier_status(verifier: felt252, authorized: bool)`
Authorize or revoke a root verifier address.

#### `set_token_whitelist(token: felt252, whitelisted: bool)`
Whitelist or delist a token for settlement.

#### `rescue_tokens(token: felt252, to: felt252, amount: u256)`
Emergency withdrawal. Owner only. Used by relayer rescue path after 1-hour timeout.

#### `pause()` / `unpause()`
Halt all state-changing operations. Owner only.

---

## Events

```
CommitmentAdded     { commitment, near_intents_id, view_key, batch_index }
BatchProcessed      { batch_id, root, leaf_count, timestamp }
MerkleRootUpdated   { root, leaf_count, timestamp }
IntentSettled       { intent_id, nullifier_hash, token, amount, timestamp }
IntentMarkedSettled { commitment, nullifier_hash }
RemoteRootSynced    { chain_id, root, leaf_count, snapshot_index }
```

---

## Access Control

| Role | Capability |
|------|-----------|
| Owner | `update_batch_config`, `set_relayer_status`, `set_root_verifier_status`, `set_token_whitelist`, `rescue_tokens`, `pause`, `unpause` |
| Authorized Relayer | `add_to_pending_batch`, `process_batch_if_timeout`, `mark_settled`, `sync_merkle_root`, `settle_and_release` |
| Root Verifier | `verify_remote_root` |
| Anyone | View functions |

---

## Token Handling

NEAR always delivers **STRK** to this contract regardless of the source token. The relayer reads the actual delivered token and amount from the StarkNet destination tx Transfer event, then optionally swaps to the user's desired destination token via AVNU.

| Source | User's dest_token | Delivered To Contract | Final Released |
|--------|-------------------|-----------------------|----------------|
| USDT (Ethereum) | STRK | STRK | STRK (no swap) |
| ETH (Ethereum) | STRK | STRK | STRK (no swap) |
| USDC (Ethereum) | USDC (StarkNet) | STRK | USDC (AVNU swap) |
| USDT (Ethereum) | USDC (StarkNet) | STRK | USDC (AVNU swap) |

> The relayer always reads the Transfer event from the delivery tx — `event.keys[2] == settlement_contract` — to determine what was actually delivered, not what the source intent declared.

---

## Setup Checklist

Before the contract is live, immediately after deployment:

```python
# 1. Authorize relayer wallet
set_relayer_status(relayer_wallet_felt, True)

# 2. Authorize root verifier (can be same as relayer)
set_root_verifier_status(relayer_wallet_felt, True)

# 3. Whitelist tokens you intend to settle
set_token_whitelist(STRK_TOKEN_ADDRESS, True)
set_token_whitelist(USDC_TOKEN_ADDRESS, True)
# add others as needed
```

Token addresses on StarkNet Mainnet:

| Token | Address |
|-------|---------|
| STRK | `0x04718f5a0Fc34cC1AF16A1cdee98fFB20C31f5cD61D6Ab07201858f4287c938D` |
| ETH | `0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7` |
| USDT | `0x068f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8` |
| USDC | `0x053c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8` |

---

## Indexer

A separate Apibara v2 indexer (`packages/indexers/starknet`) watches this contract for events and writes to the relayer database. It tracks:

- `CommitmentAdded` → `commitments` table
- `BatchProcessed` → `batches_processed` table
- `MerkleRootUpdated` → `merkle_roots` table
- `IntentSettled` → `intents_settled` table
- `IntentMarkedSettled` → `intents_marked_settled` table

**Starting block:** `6946374`

To reindex from scratch (run in `shadow-swap-indexer` DB, indexer must be stopped first):

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

Then restart the indexer — it will replay from `startingBlock` and forward all events to the relayer.

---

## Security Properties

- **Nullifier uniqueness**: Each nullifier can only be spent once — prevents double spend on destination
- **No on-chain ZK proof**: Relayer is trusted; Merkle proof verified off-chain
- **Commitment binding**: 5-parameter Poseidon commitment prevents reuse across different amounts or tokens
- **Field prime safety**: secret and nullifier must be 31 bytes max to stay below felt252 prime
- **Pause circuit breaker**: Owner can halt all operations in case of incident
- **Rescue hatch**: Owner can recover stuck funds — use a separate key from the relayer key