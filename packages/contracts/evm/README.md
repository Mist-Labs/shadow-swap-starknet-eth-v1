# ShadowSettlement — EVM Contract

Solidity settlement contract for the ShadowSwap Privacy Bridge on Ethereum Mainnet. Handles commitment batching, cross-chain Merkle root syncing, nullifier-based settlement, and token release.

**Mainnet:** `0xDcDdb3E6EA09dA3a93B1f41BCd017156Ce8b9468`

---

## Overview

Users bridge tokens privately by committing a cryptographic hash on the source chain. The relayer batches commitments, syncs Merkle roots cross-chain, and calls `settleAndRelease` on the destination chain using an off-chain verified Merkle proof. No ZK proof is submitted on-chain — the contract trusts an authorized relayer.

```
User → addToPendingBatch(commitment)
     → [batch processed on-chain]
     → Merkle root updated

NEAR delivers tokens to this contract
     → Relayer calls settleAndRelease(intentId, nullifier, recipient, token, amount)
     → Contract verifies nullifier unused + remote root verified
     → Transfers token to recipient
```

---

## Commitment Scheme

Commitments are **5-parameter Keccak256 hashes** — binding amount, token, and destination chain prevents reuse across swaps.

```solidity
// Preimage: secret || nullifier || amount || token || destChain
commitment = keccak256(abi.encodePacked(secret, nullifier, amount, token, destChain));

// Nullifier hash (single param)
nullifierHash = keccak256(abi.encodePacked(nullifier));
```

> **Security**: The 5-parameter scheme follows the Tornado Cash / Aztec pattern. A 2-parameter commitment (secret, nullifier only) allows reuse across different swap amounts — do not use the old scheme.

---

## Merkle Tree

Fixed-height incremental Merkle tree (height = 20, max 1,048,576 leaves).

- **Hash function**: Keccak256 with **sorted pair hashing** — matches OpenZeppelin `MerkleProof.sol`
- **Zero hashes**: `zeros[0] = 0x00...00`, `zeros[i] = keccak256(zeros[i-1], zeros[i-1])`
- **Root sync**: Relayer pushes StarkNet's root to this contract via `syncMerkleRoot`

```solidity
function _hashPair(bytes32 a, bytes32 b) internal pure returns (bytes32) {
    return a < b ? keccak256(abi.encodePacked(a, b))
                 : keccak256(abi.encodePacked(b, a));
}
```

---

## Functions

### Source Side

#### `addToPendingBatch(bytes32 commitment, bytes32 nearIntentsId, bytes32 viewKey)`
Adds a commitment to the pending batch. Called by authorized relayer after user initiates a bridge.

- `commitment` — 5-parameter keccak256 hash
- `nearIntentsId` — NEAR `correlationId` UUID packed as bytes32
- `viewKey` — deterministic per-user key for history lookup

#### `processBatchIfTimeout()`
Seals the current pending batch and updates the Merkle root. Called by relayer when batch is full or timeout elapsed.

#### `markSettled(bytes32 commitment, bytes32 nullifierHash)`
Marks a commitment as settled on the source chain after destination settlement completes.

---

### Cross-Chain Sync

#### `syncMerkleRoot(string calldata chainId, bytes32 root, uint256 leafCount)`
Called by relayer to push a remote chain's Merkle root to this contract.

- `chainId` — `"starknet"` for StarkNet root syncs

#### `verifyRemoteRoot(string calldata chainId, uint256 snapshotIndex)`
Marks a previously synced remote root snapshot as verified. Required before `settleAndRelease` can use it.

---

### Destination Side

#### `settleAndRelease(bytes32 intentId, bytes32 nullifierHash, address recipient, address token, uint256 amount)`
Releases tokens to recipient. Called by authorized relayer after NEAR delivers tokens to this contract.

Checks:
1. Nullifier not already used
2. Remote Merkle root verified
3. Token is whitelisted
4. Contract holds sufficient balance

For **native ETH**: pass `token = 0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE` — contract sends ETH directly.

```solidity
// Relayer passes actual delivered token/amount from NEAR dest tx receipt
// NOT the source chain token — NEAR swaps (e.g. STRK → ETH)
settleAndRelease(intentId, nullifierHash, recipient, deliveredToken, deliveredAmount);
```

---

### View Functions

#### `getMerkleRoot() → bytes32`
Returns the current local Merkle root.

#### `isNullifierUsed(bytes32 nullifierHash) → bool`
Returns true if nullifier has been spent.

#### `commitmentExists(bytes32 commitment) → bool`
Returns true if commitment is in the tree.

#### `getLatestRemoteRoot(string calldata chainId) → (bytes32 root, uint256 leafCount, uint64 syncedAt, bool verified)`
Returns the most recently synced remote root for a given chain.

#### `getRemoteRootCount(string calldata chainId) → uint256`
Returns total number of synced remote root snapshots.

---

### Admin / Owner

#### `setTokenWhitelist(address token, bool whitelisted)`
Whitelist or delist an ERC20 token. Only whitelisted tokens can be released via `settleAndRelease`.

#### `rescueTokens(address token, address to, uint256 amount)`
Emergency withdrawal. Owner only. Used by relayer rescue path after 1-hour timeout.

#### `pause()` / `unpause()`
Halt all state-changing operations. Owner only.

---

## Events

```solidity
event IntentSettled(
    bytes32 indexed intentId,
    bytes32 indexed nullifierHash,
    address token,
    uint256 amount,
    uint64 timestamp
);
```

---

## Access Control

| Role | Capability |
|------|-----------|
| Owner | `setTokenWhitelist`, `rescueTokens`, `pause`, `unpause` |
| Authorized Relayer | `addToPendingBatch`, `markSettled`, `syncMerkleRoot`, `verifyRemoteRoot`, `settleAndRelease` |
| Anyone | View functions |

Relayer authorization is managed on-chain. Must be set before deployment goes live:

```solidity
setRelayerStatus(relayerAddress, true)
```

---

## Token Handling

NEAR swaps the source token to the destination token before delivering. The relayer reads the actual delivered token and amount from the dest tx receipt.

| Source | Destination | Delivered To Contract |
|--------|-------------|----------------------|
| STRK (StarkNet) | ETH (Ethereum) | Native ETH (`0xEeee...EEeE`) |
| STRK (StarkNet) | USDT (Ethereum) | Ethereum |
| USDT (Ethereum) | STRK (StarkNet) | STRK on StarkNet contract |

> ETH-destination swaps deliver native ETH directly to the contract with no ERC20 Transfer event. The contract must handle the `0xEeee...EEeE` sentinel and send ETH to the recipient.

---

## Setup Checklist

Before the contract is live:

```solidity
// 1. Authorize relayer wallet
setRelayerStatus(relayerWalletAddress, true);

// 2. Whitelist tokens you intend to settle
setTokenWhitelist(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2, true); // WETH
setTokenWhitelist(0xdAC17F958D2ee523a2206206994597C13D831ec7, true); // USDT
// add others as needed
```

---

## Security Properties

- **Nullifier uniqueness**: Each nullifier can only be spent once — prevents double spend on destination
- **No on-chain ZK proof**: Relayer is trusted; Merkle proof verified off-chain
- **Commitment binding**: 5-parameter commitment prevents reuse across different amounts or tokens
- **Pause circuit breaker**: Owner can halt all operations in case of incident
- **Rescue hatch**: Owner can recover stuck funds — use a separate key from the relayer key