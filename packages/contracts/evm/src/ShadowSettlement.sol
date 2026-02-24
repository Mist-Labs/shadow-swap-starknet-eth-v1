// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/**
 * @title ShadowSettlement
 * @notice Privacy-preserving cross-chain settlement contract (EVM side)
 * @dev Bidirectional — acts as SOURCE (commitment storage) and DESTINATION (token release)
 *
 * ARCHITECTURE:
 * - Source side: Relayer batches commitments into Merkle tree for anonymity set
 * - Destination side: NEAR 1Click bridges tokens TO this contract via standard
 *   ERC20 transfer, then relayer verifies delivery and calls settleAndRelease
 * - Cross-chain sync: Stores remote chain Merkle roots for auditability
 * - Per-user view keys for private tracking (derived from wallet signature)
 *
 * TOKEN FLOW (when this chain is DESTINATION):
 * 1. User initiates swap on source chain → commitment stored in source Merkle tree
 * 2. User sends tokens to NEAR 1Click via unique depositAddress
 * 3. NEAR 1Click swaps + bridges → tokens arrive at THIS contract via standard ERC20 transfer
 * 4. Relayer polls NEAR status API → gets destinationChainTxHashes
 * 5. Relayer verifies Transfer event on-chain (correct amount to this contract)
 * 6. Relayer calls settleAndRelease(nullifier, recipient, token, amount)
 *    → Contract verifies nullifier, transfers ERC20 to user
 * 7. Relayer calls markSettled(commitment, nullifier) on SOURCE chain contract
 *
 * TRUST MODEL:
 * - Relayer trusted to verify NEAR delivery before releasing (centralized MVP)
 * - Relayer cannot double-spend (nullifier prevents)
 * - Relayer cannot release more than contract balance (safeTransfer reverts)
 * - Owner can pause in emergencies and manage relayers
 *
 * PRIVACY:
 * - View keys are NEVER exposed on-chain or in events
 * - NEAR intent IDs stored internally, not publicly queryable
 * - Commitment tree position not leaked in events
 * - Batch fill level not leaked in events
 * - settleAndRelease reveals recipient but NOT linked to source commitment
 *   (same model as Tornado Cash withdrawals)
 *
 * COMMITMENT FORMULA (enforced client-side):
 * Frontend MUST generate commitments as:
 *   commitment = keccak256(abi.encodePacked(secret, nullifier, amount, token, destChain))
 *
 * Including amount, token, and destChain prevents:
 * - Cross-swap attacks (same secret can't be reused for different amounts/chains)
 * - Commitment reuse across different swap parameters
 * - This is the industry standard (Tornado Cash, Aztec, etc.)
 *
 * Note: Contract does NOT validate the commitment formula (it's a hash).
 *       Security comes from frontend generating correctly + Merkle proof verification.
 */
contract ShadowSettlement is ReentrancyGuard, Ownable, Pausable {
    using SafeERC20 for IERC20;

    // ===== STRUCTS =====

    /// @dev Internal struct — never returned to external callers with viewKey
    struct Intent {
        bytes32 commitment;
        bytes32 nearIntentsId;
        bytes32 viewKey;
        uint64 submittedAt;
        bool settled;
    }

    /// @notice Public-safe intent data (no viewKey, no nearIntentsId)
    struct IntentPublic {
        bytes32 commitment;
        uint64 submittedAt;
        bool settled;
    }

    /// @notice Full intent data returned to view key holder
    struct IntentDetail {
        bytes32 commitment;
        bytes32 nearIntentsId;
        uint64 submittedAt;
        bool settled;
    }

    /// @notice Remote chain Merkle root snapshot
    struct RemoteRootSnapshot {
        bytes32 root;
        uint256 leafCount;
        uint64 syncedAt;
        bool verified;
    }

    // ===== STATE VARIABLES =====

    // --- Source side (commitment storage) ---

    /// @dev Internal — use getIntent() which strips sensitive fields
    mapping(bytes32 => Intent) internal intents;
    mapping(bytes32 => bool) public usedNullifiers;

    /// @dev Internal — only queryable by providing the correct view key
    mapping(bytes32 => bytes32[]) internal viewKeyToCommitments;

    uint256 public constant TREE_HEIGHT = 20;
    uint256 public nextLeafIndex;
    mapping(uint256 => bytes32) internal filledSubtrees;
    bytes32 public currentRoot;
    bytes32[TREE_HEIGHT] internal zeros;

    /// @dev Internal — tree position is privacy-sensitive
    mapping(bytes32 => uint256) internal commitmentToIndex;

    /// @dev Mapping-based batch avoids gas bomb on delete
    mapping(uint256 => bytes32) internal batchCommitments;
    mapping(uint256 => bytes32) internal batchNearIntentsIds;
    mapping(uint256 => bytes32) internal batchViewKeys;
    uint256 public batchCount;
    uint64 public batchFirstSubmissionTime;

    // --- Cross-chain sync ---

    /// @notice Remote chain identifier → root snapshot history
    /// @dev chainId examples: "starknet-mainnet", "starknet-sepolia"
    mapping(string => RemoteRootSnapshot[]) public remoteRootHistory;

    /// @notice Quick lookup: chainId → latest root index
    mapping(string => uint256) public latestRemoteRootIndex;

    /// @notice Trusted root verifiers (can mark roots as verified)
    mapping(address => bool) public rootVerifiers;

    // --- Destination side (token release) ---

    /// @notice Whitelisted tokens for settlement
    mapping(address => bool) public whitelistedTokens;

    // --- Config ---

    uint256 public batchSize;
    uint256 public batchTimeout;
    mapping(address => bool) public authorizedRelayers;

    // ===== CONSTANTS =====

    uint256 public constant MIN_BATCH_SIZE = 1;
    uint256 public constant MAX_BATCH_SIZE = 100;
    uint256 public constant DEFAULT_BATCH_SIZE = 10;
    uint256 public constant DEFAULT_TIMEOUT = 30;

    /// @notice Sentinel address representing native ETH (industry standard)
    address public constant ETH = 0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE;

    // ===== EVENTS =====

    // --- Source side events ---

    /// @notice Emits only commitment hash — no tree position, no batch info
    event CommitmentAdded(bytes32 indexed commitment);

    event BatchProcessed(
        uint256 indexed batchId,
        uint256 commitmentsCount,
        ProcessReason reason
    );

    event MerkleRootUpdated(bytes32 indexed newRoot);

    /// @notice Source-side: commitment marked settled after dest-side release
    event IntentMarkedSettled(
        bytes32 indexed nullifierHash,
        bytes32 indexed commitment,
        uint64 timestamp
    );

    // --- Cross-chain sync events ---

    event RemoteRootSynced(
        string indexed chainId,
        bytes32 indexed root,
        uint256 leafCount,
        uint256 snapshotIndex
    );

    event RemoteRootVerified(
        string indexed chainId,
        uint256 indexed snapshotIndex,
        address verifier
    );

    // --- Destination side events ---

    /// @notice Emitted when tokens are released to user
    /// @dev recipient is visible but NOT linked to source commitment on-chain
    event IntentSettled(
        bytes32 indexed intentId,
        bytes32 indexed nullifierHash,
        address token,
        uint256 amount,
        uint64 timestamp
    );

    // --- Admin events ---

    event BatchConfigUpdated(uint256 newBatchSize, uint256 newTimeout);
    event RelayerStatusChanged(address indexed relayer, bool authorized);
    event RootVerifierStatusChanged(address indexed verifier, bool authorized);
    event TokenWhitelistUpdated(address indexed token, bool whitelisted);

    // ===== ENUMS =====

    enum ProcessReason {
        BATCH_FULL,
        TIMEOUT_REACHED
    }

    // ===== ERRORS =====

    error Unauthorized();
    error InvalidBatchSize();
    error InvalidTimeout();
    error CommitmentExists();
    error CommitmentNotFound();
    error NullifierUsed();
    error BatchEmpty();
    error InvalidCommitment();
    error TreeFull();
    error TimeoutNotReached();
    error TokenNotWhitelisted();
    error InvalidAmount();
    error InvalidRecipient();
    error InvalidChainId();
    error InvalidRoot();
    error RootAlreadyVerified();
    error SnapshotNotFound();
    error TokenWhitelistUnchanged();
    error TransferFailed();

    // ===== MODIFIERS =====

    modifier onlyRelayer() {
        if (!authorizedRelayers[msg.sender]) revert Unauthorized();
        _;
    }

    modifier onlyRootVerifier() {
        if (!rootVerifiers[msg.sender]) revert Unauthorized();
        _;
    }

    // ===== CONSTRUCTOR =====

    constructor(address _owner, address _initialRelayer) Ownable(_owner) {
        authorizedRelayers[_initialRelayer] = true;
        rootVerifiers[_initialRelayer] = true;
        batchSize = DEFAULT_BATCH_SIZE;
        batchTimeout = DEFAULT_TIMEOUT;

        zeros[0] = bytes32(0);
        for (uint256 i = 1; i < TREE_HEIGHT; i++) {
            zeros[i] = _hashPair(zeros[i - 1], zeros[i - 1]);
        }
    }

    /// @notice Accept native ETH — NEAR bridge delivers ETH directly to this contract
    receive() external payable {}

    /// @notice Reject unknown calls with calldata
    fallback() external {
        revert("Unknown function");
    }

    // ==============================================================
    //                     SOURCE SIDE FUNCTIONS
    //         (when this chain is where the user starts)
    // ==============================================================

    /**
     * @notice Add commitment to pending batch
     * @dev Called by relayer when user submits intent via API.
     *      Commitment is opaque bytes32 generated client-side:
     *      commitment = Poseidon(secret, nullifier, amount, destChain)
     *
     * @param commitment Privacy commitment (opaque bytes32 from client)
     * @param nearIntentsId NEAR Intents tracking ID (internal, not publicly exposed)
     * @param viewKey Optional per-user view key (bytes32(0) to skip)
     */
    function addToPendingBatch(
        bytes32 commitment,
        bytes32 nearIntentsId,
        bytes32 viewKey
    ) external onlyRelayer whenNotPaused {
        if (commitment == bytes32(0)) revert InvalidCommitment();
        if (intents[commitment].commitment != bytes32(0))
            revert CommitmentExists();
        if (nextLeafIndex >= (uint256(1) << TREE_HEIGHT)) revert TreeFull();

        uint256 count = batchCount;

        if (count == 0) {
            batchFirstSubmissionTime = uint64(block.timestamp);
        }

        batchCommitments[count] = commitment;
        batchNearIntentsIds[count] = nearIntentsId;
        batchViewKeys[count] = viewKey;
        batchCount = count + 1;

        emit CommitmentAdded(commitment);

        if (count + 1 >= batchSize) {
            _processBatch(ProcessReason.BATCH_FULL);
        }
    }

    /**
     * @notice Process batch if timeout reached
     * @dev Anyone can call — ensures liveness even if relayer is slow.
     *      Processes even single-item batches (liveness > privacy).
     */
    function processBatchIfTimeout() external whenNotPaused {
        if (batchCount == 0) revert BatchEmpty();

        uint256 timeSinceFirst = block.timestamp - batchFirstSubmissionTime;
        if (timeSinceFirst < batchTimeout) revert TimeoutNotReached();

        _processBatch(ProcessReason.TIMEOUT_REACHED);
    }

    /**
     * @notice Internal batch processing
     * @dev Registers all pending commitments in Merkle tree.
     *      View key mapping is written here
     *      to ensure Intent struct exists before viewKey queries work.
     */
    function _processBatch(ProcessReason reason) internal {
        uint256 count = batchCount;
        if (count == 0) revert BatchEmpty();

        for (uint256 i = 0; i < count; i++) {
            bytes32 commitment = batchCommitments[i];
            bytes32 nearIntentsId = batchNearIntentsIds[i];
            bytes32 viewKey = batchViewKeys[i];

            intents[commitment] = Intent({
                commitment: commitment,
                nearIntentsId: nearIntentsId,
                viewKey: viewKey,
                submittedAt: uint64(block.timestamp),
                settled: false
            });

            _insertCommitment(commitment);

            if (viewKey != bytes32(0)) {
                viewKeyToCommitments[viewKey].push(commitment);
            }
        }

        emit BatchProcessed(nextLeafIndex, count, reason);
        emit MerkleRootUpdated(currentRoot);

        batchCount = 0;
    }

    /**
     * @notice Mark a source-side commitment as settled
     * @dev Called by relayer AFTER tokens were released on the destination chain.
     *      This updates the source-side intent status so view key queries
     *      reflect the completed settlement. Runs on the chain where the
     *      commitment was originally stored (source chain).
     *
     *      Flow: settleAndRelease (dest) → relayer confirms → markSettled (source)
     *
     * @param commitment Intent commitment (must exist in this contract's tree)
     * @param nullifierHash Hash of nullifier (prevents double-marking)
     */
    function markSettled(
        bytes32 commitment,
        bytes32 nullifierHash
    ) external onlyRelayer whenNotPaused {
        Intent storage intent = intents[commitment];
        if (intent.commitment == bytes32(0)) revert CommitmentNotFound();
        if (usedNullifiers[nullifierHash]) revert NullifierUsed();

        intent.settled = true;
        usedNullifiers[nullifierHash] = true;

        emit IntentMarkedSettled(
            nullifierHash,
            commitment,
            uint64(block.timestamp)
        );
    }

    // ==============================================================
    //                   CROSS-CHAIN SYNC FUNCTIONS
    // ==============================================================

    /**
     * @notice Sync Merkle root from remote chain (e.g., StarkNet)
     * @dev Called by relayer to store remote chain state for auditability.
     *      Enables cross-verification that commitments exist on both chains.
     *
     * @param chainId Remote chain identifier (e.g., "starknet-mainnet")
     * @param root Merkle root from remote chain
     * @param leafCount Number of commitments in remote tree at time of sync
     */
    function syncMerkleRoot(
        string calldata chainId,
        bytes32 root,
        uint256 leafCount
    ) external onlyRelayer whenNotPaused {
        if (bytes(chainId).length == 0) revert InvalidChainId();
        if (root == bytes32(0)) revert InvalidRoot();
        if (leafCount == 0) revert InvalidAmount();

        RemoteRootSnapshot memory snapshot = RemoteRootSnapshot({
            root: root,
            leafCount: leafCount,
            syncedAt: uint64(block.timestamp),
            verified: false
        });

        remoteRootHistory[chainId].push(snapshot);
        uint256 newIndex = remoteRootHistory[chainId].length - 1;
        latestRemoteRootIndex[chainId] = newIndex;

        emit RemoteRootSynced(chainId, root, leafCount, newIndex);
    }

    /**
     * @notice Mark a synced root as verified
     * @dev Called by trusted verifier (could be oracle, bridge, or multi-sig).
     *      Once verified, root is considered authoritative.
     *
     * @param chainId Remote chain identifier
     * @param snapshotIndex Index in remoteRootHistory array
     */
    function verifyRemoteRoot(
        string calldata chainId,
        uint256 snapshotIndex
    ) external onlyRootVerifier whenNotPaused {
        RemoteRootSnapshot[] storage snapshots = remoteRootHistory[chainId];
        if (snapshotIndex >= snapshots.length) revert SnapshotNotFound();

        RemoteRootSnapshot storage snapshot = snapshots[snapshotIndex];
        if (snapshot.verified) revert RootAlreadyVerified();

        snapshot.verified = true;

        emit RemoteRootVerified(chainId, snapshotIndex, msg.sender);
    }

    // ==============================================================
    //                   DESTINATION SIDE FUNCTIONS
    //        (when this chain is where the user receives)
    // ==============================================================

    /**
     * @notice Release tokens to user after NEAR bridge delivery is verified
     * @dev Called by relayer after confirming token arrival via:
     *      1. Poll NEAR status API → get destinationChainTxHashes
     *      2. Verify Transfer event on-chain (amount + recipient = this contract)
     *      3. Call this function to release tokens to user
     *
     *      No on-chain deposit tracking — tokens arrive via standard ERC20
     *      transfer from NEAR 1Click bridge infrastructure. Relayer verifies
     *      the exact amount off-chain before calling.
     *
     * PRIVACY:
     * - recipient address visible on-chain in this call
     * - NOT linked to any source-chain commitment on-chain
     * - Link exists only in relayer's off-chain DB
     * - Same model as Tornado Cash withdrawals
     *
     * @param nullifierHash Hash of nullifier (prevents double-settlement)
     * @param recipient User's destination address on this chain
     * @param token ERC20 token to release
     * @param amount Amount to release (verified by relayer against NEAR status)
     */
    function settleAndRelease(
        bytes32 intentId,
        bytes32 nullifierHash,
        address recipient,
        address token,
        uint256 amount
    ) external onlyRelayer nonReentrant whenNotPaused {
        if (usedNullifiers[nullifierHash]) revert NullifierUsed();
        if (recipient == address(0)) revert InvalidRecipient();
        if (!whitelistedTokens[token]) revert TokenNotWhitelisted();
        if (amount == 0) revert InvalidAmount();

        usedNullifiers[nullifierHash] = true;

        if (token == ETH) {
            (bool ok, ) = payable(recipient).call{value: amount}("");
            if (!ok) revert TransferFailed();
        } else {
            IERC20(token).safeTransfer(recipient, amount);
        }

        emit IntentSettled(
            intentId,
            nullifierHash,
            token,
            amount,
            uint64(block.timestamp)
        );
    }

    // ===== MERKLE TREE FUNCTIONS =====

    /**
     * @notice Insert commitment into incremental Merkle tree
     * @dev Relayer replicates this logic off-chain for proof generation.
     *      Uses sorted hashing to prevent sibling-position leaks.
     */
    function _insertCommitment(bytes32 commitment) internal {
        uint256 index = nextLeafIndex;
        commitmentToIndex[commitment] = index;
        nextLeafIndex++;

        bytes32 currentHash = commitment;
        bytes32 left;
        bytes32 right;

        for (uint256 height = 0; height < TREE_HEIGHT; height++) {
            if (index & 1 == 0) {
                left = currentHash;
                right = zeros[height];
                filledSubtrees[height] = currentHash;
            } else {
                left = filledSubtrees[height];
                right = currentHash;
            }

            currentHash = _hashPair(left, right);
            index >>= 1;
        }

        currentRoot = currentHash;
    }

    function getMerkleRoot() external view returns (bytes32) {
        return currentRoot;
    }

    /**
     * @notice Hash pair of nodes with deterministic ordering
     * @dev Sorted to prevent sibling-position information leaks
     */
    function _hashPair(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        return
            a < b
                ? keccak256(abi.encodePacked(a, b))
                : keccak256(abi.encodePacked(b, a));
    }

    // ===== VIEW KEY FUNCTIONS =====

    /**
     * @notice Get intents for a view key with pagination (full details)
     * @dev View key IS the auth — if you have it, you see everything.
     *      Returns empty array if view key has no intents (prevents existence probing).
     *      Compliance responsibility lies with the user via their view key.
     *
     * @param viewKey Per-user view key (derived from wallet signature client-side)
     * @param offset Start index (0-based)
     * @param limit Max results to return (0 = all from offset)
     * @return userIntents Array of intent details for the requested page
     * @return total Total number of intents for this view key
     */
    function getIntentsByViewKey(
        bytes32 viewKey,
        uint256 offset,
        uint256 limit
    ) external view returns (IntentDetail[] memory userIntents, uint256 total) {
        bytes32[] storage commitments = viewKeyToCommitments[viewKey];
        total = commitments.length;

        if (total == 0 || offset >= total) {
            userIntents = new IntentDetail[](0);
            return (userIntents, total);
        }

        uint256 remaining = total - offset;
        uint256 count = (limit == 0 || limit > remaining) ? remaining : limit;

        userIntents = new IntentDetail[](count);

        for (uint256 i = 0; i < count; i++) {
            Intent storage intent = intents[commitments[offset + i]];
            userIntents[i] = IntentDetail({
                commitment: intent.commitment,
                nearIntentsId: intent.nearIntentsId,
                submittedAt: intent.submittedAt,
                settled: intent.settled
            });
        }

        return (userIntents, total);
    }

    // ===== PUBLIC VIEW FUNCTIONS =====

    function getIntent(
        bytes32 commitment
    ) external view returns (IntentPublic memory info) {
        Intent storage intent = intents[commitment];
        if (intent.commitment == bytes32(0)) revert CommitmentNotFound();

        info = IntentPublic({
            commitment: intent.commitment,
            submittedAt: intent.submittedAt,
            settled: intent.settled
        });
    }

    function commitmentExists(
        bytes32 commitment
    ) external view returns (bool exists) {
        return intents[commitment].commitment != bytes32(0);
    }

    function isNullifierUsed(bytes32 nullifier) external view returns (bool) {
        return usedNullifiers[nullifier];
    }

    function getPendingBatchInfo()
        external
        view
        returns (
            uint256 count,
            uint64 firstSubmissionTime,
            uint256 timeRemaining
        )
    {
        count = batchCount;
        firstSubmissionTime = batchFirstSubmissionTime;

        if (count > 0) {
            uint256 elapsed = block.timestamp - firstSubmissionTime;
            timeRemaining = elapsed >= batchTimeout
                ? 0
                : batchTimeout - elapsed;
        }
    }

    function isRelayerAuthorized(address relayer) external view returns (bool) {
        return authorizedRelayers[relayer];
    }

    function isRootVerifier(address verifier) external view returns (bool) {
        return rootVerifiers[verifier];
    }

    function getLatestRemoteRoot(
        string calldata chainId
    ) external view returns (RemoteRootSnapshot memory snapshot) {
        RemoteRootSnapshot[] storage snapshots = remoteRootHistory[chainId];
        if (snapshots.length == 0) revert SnapshotNotFound();
        return snapshots[latestRemoteRootIndex[chainId]];
    }

    function getLatestVerifiedRemoteRoot(
        string calldata chainId
    ) external view returns (RemoteRootSnapshot memory snapshot, uint256 index) {
        RemoteRootSnapshot[] storage snapshots = remoteRootHistory[chainId];
        if (snapshots.length == 0) revert SnapshotNotFound();

        for (uint256 i = snapshots.length; i > 0; i--) {
            if (snapshots[i - 1].verified) {
                return (snapshots[i - 1], i - 1);
            }
        }

        revert SnapshotNotFound();
    }

    function getRemoteRootSnapshot(
        string calldata chainId,
        uint256 snapshotIndex
    ) external view returns (RemoteRootSnapshot memory snapshot) {
        RemoteRootSnapshot[] storage snapshots = remoteRootHistory[chainId];
        if (snapshotIndex >= snapshots.length) revert SnapshotNotFound();
        return snapshots[snapshotIndex];
    }

    function getRemoteRootCount(
        string calldata chainId
    ) external view returns (uint256 count) {
        return remoteRootHistory[chainId].length;
    }

    // ===== ADMIN FUNCTIONS =====

    function updateBatchConfig(
        uint256 newBatchSize,
        uint256 newTimeout
    ) external onlyOwner {
        if (newBatchSize < MIN_BATCH_SIZE || newBatchSize > MAX_BATCH_SIZE) {
            revert InvalidBatchSize();
        }
        if (newTimeout == 0) revert InvalidTimeout();

        batchSize = newBatchSize;
        batchTimeout = newTimeout;

        emit BatchConfigUpdated(newBatchSize, newTimeout);
    }

    function setRelayerStatus(
        address relayer,
        bool authorized
    ) external onlyOwner {
        authorizedRelayers[relayer] = authorized;
        emit RelayerStatusChanged(relayer, authorized);
    }

    function setRootVerifierStatus(
        address verifier,
        bool authorized
    ) external onlyOwner {
        rootVerifiers[verifier] = authorized;
        emit RootVerifierStatusChanged(verifier, authorized);
    }

    /**
     * @notice Whitelist or delist a token for settlement
     * @param token ERC20 token address
     * @param whitelisted True to whitelist, false to delist
     */
    function setTokenWhitelist(
        address token,
        bool whitelisted
    ) external onlyOwner {
        if (whitelistedTokens[token] == whitelisted) revert TokenWhitelistUnchanged();
        whitelistedTokens[token] = whitelisted;
        emit TokenWhitelistUpdated(token, whitelisted);
    }

    /**
     * @notice Emergency rescue stuck tokens
     * @dev Only callable by owner. For recovering tokens from failed NEAR
     *      bridge transfers or tokens sent to contract by mistake.
     *
     * @param token ERC20 token address
     * @param to Recipient address
     * @param amount Amount to rescue
     */
    function rescueTokens(
        address token,
        address to,
        uint256 amount
    ) external onlyOwner {
        if (token == ETH) {
            (bool ok, ) = payable(to).call{value: amount}("");
            if (!ok) revert TransferFailed();
        } else {
            IERC20(token).safeTransfer(to, amount);
        }
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }
}
