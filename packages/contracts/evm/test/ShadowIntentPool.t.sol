// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console2} from "forge-std/Test.sol";
import {ShadowSettlement} from "../src/ShadowSettlement.sol";
import {Vm} from "forge-std/Vm.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

// Mock ERC20 for testing
contract MockERC20 {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    
    string public name = "Mock Token";
    string public symbol = "MOCK";
    uint8 public decimals = 18;
    uint256 public totalSupply;
    
    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }
    
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }
    
    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }
    
    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
    }
}

contract ShadowSettlementTest is Test {
    ShadowSettlement public pool;
    MockERC20 public mockToken;

    address public owner = address(0x1);
    address public relayer = address(0x2);
    address public unauthorized = address(0x3);
    address public user = address(0x4);

    bytes32 public commitment1 = keccak256("commitment1");
    bytes32 public commitment2 = keccak256("commitment2");
    bytes32 public commitment3 = keccak256("commitment3");

    bytes32 public nearId1 = keccak256("near1");
    bytes32 public nearId2 = keccak256("near2");
    bytes32 public nearId3 = keccak256("near3");

    bytes32 public viewKey1 = keccak256("viewkey1");
    bytes32 public viewKey2 = keccak256("viewkey2");

    bytes32 public nullifier1 = keccak256("nullifier1");
    bytes32 public nullifier2 = keccak256("nullifier2");

    bytes32 public intentId1 = keccak256("intentId1");
    bytes32 public intentId2 = keccak256("intentId2");

    // Updated event signatures matching the contract
    event CommitmentAdded(bytes32 indexed commitment);

    event BatchProcessed(
        uint256 indexed batchId,
        uint256 commitmentsCount,
        ShadowSettlement.ProcessReason reason
    );

    event IntentMarkedSettled(
        bytes32 indexed nullifierHash,
        bytes32 indexed commitment,
        uint64 timestamp
    );

    event IntentSettled(
        bytes32 indexed intentId,
        bytes32 indexed nullifierHash,
        address token,
        uint256 amount,
        uint64 timestamp
    );

    event MerkleRootUpdated(bytes32 indexed newRoot);
    event BatchConfigUpdated(uint256 newBatchSize, uint256 newTimeout);
    event RelayerStatusChanged(address indexed relayer, bool authorized);
    event TokenWhitelistUpdated(address indexed token, bool whitelisted);

    function setUp() public {
        vm.startPrank(owner);
        pool = new ShadowSettlement(owner, relayer);
        mockToken = new MockERC20();
        
        // Whitelist mock token
        pool.setTokenWhitelist(address(mockToken), true);
        vm.stopPrank();
    }

    // ===== HELPERS =====

    /// @dev Fill batch to trigger auto-processing (default batch size = 10)
    function _fillBatchWithFiller(uint256 startIndex, uint256 count) internal {
        for (uint256 i = startIndex; i < startIndex + count; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("filler", i)),
                keccak256(abi.encode("near_filler", i)),
                bytes32(0)
            );
        }
    }

    /// @dev Add a commitment and fill the rest of the batch to process it
    function _addAndProcessCommitment(
        bytes32 commitment,
        bytes32 nearId,
        bytes32 viewKey,
        uint256 fillerStart
    ) internal {
        pool.addToPendingBatch(commitment, nearId, viewKey);
        _fillBatchWithFiller(fillerStart, 9);
    }

    // ===== CONSTRUCTOR TESTS =====

    function test_Constructor_SetsOwner() public view {
        assertEq(pool.owner(), owner);
    }

    function test_Constructor_AuthorizesRelayer() public view {
        assertTrue(pool.authorizedRelayers(relayer));
    }

    function test_Constructor_SetsDefaultBatchConfig() public view {
        assertEq(pool.batchSize(), 10);
        assertEq(pool.batchTimeout(), 30);
    }

    function test_Constructor_InitialMerkleRootIsZero() public view {
        assertEq(pool.getMerkleRoot(), bytes32(0));
    }

    function test_Constructor_InitialLeafIndexIsZero() public view {
        assertEq(pool.nextLeafIndex(), 0);
    }

    // ===== ACCESS CONTROL TESTS =====

    function test_AddToPendingBatch_RevertsIfNotRelayer() public {
        vm.prank(unauthorized);
        vm.expectRevert(ShadowSettlement.Unauthorized.selector);
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);
    }

    function test_MarkSettled_RevertsIfNotRelayer() public {
        vm.prank(unauthorized);
        vm.expectRevert(ShadowSettlement.Unauthorized.selector);
        pool.markSettled(commitment1, nullifier1);
    }

    function test_SettleAndRelease_RevertsIfNotRelayer() public {
        vm.prank(unauthorized);
        vm.expectRevert(ShadowSettlement.Unauthorized.selector);
        pool.settleAndRelease(intentId1, nullifier1, user, address(mockToken), 1000 ether);
    }

    function test_SetRelayerStatus_OnlyOwner() public {
        vm.prank(unauthorized);
        vm.expectRevert();
        pool.setRelayerStatus(unauthorized, true);

        vm.prank(owner);
        pool.setRelayerStatus(unauthorized, true);
        assertTrue(pool.authorizedRelayers(unauthorized));
    }

    function test_SetRelayerStatus_RevokeRelayer() public {
        vm.startPrank(owner);
        pool.setRelayerStatus(relayer, false);
        vm.stopPrank();

        assertFalse(pool.isRelayerAuthorized(relayer));

        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.Unauthorized.selector);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));
    }

    // ===== COMMITMENT ADDITION TESTS =====

    function test_AddToPendingBatch_Success() public {
        vm.prank(relayer);
        vm.expectEmit(true, false, false, false);
        emit CommitmentAdded(commitment1);

        pool.addToPendingBatch(commitment1, nearId1, viewKey1);

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 1);
    }

    function test_AddToPendingBatch_EventEmitsOnlyCommitment() public {
        // Verify event has no leafIndex or pendingCount (privacy)
        vm.prank(relayer);

        vm.recordLogs();
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);

        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 1);
        // CommitmentAdded(bytes32 indexed commitment) → 2 topics (sig + commitment)
        assertEq(entries[0].topics.length, 2);
        assertEq(entries[0].topics[1], commitment1);
        // No additional data (no leafIndex, no pendingCount)
        assertEq(entries[0].data.length, 0);
    }

    function test_AddToPendingBatch_RevertsIfCommitmentExists() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 100);

        vm.expectRevert(ShadowSettlement.CommitmentExists.selector);
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);
        vm.stopPrank();
    }

    function test_AddToPendingBatch_RevertsIfInvalidCommitment() public {
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.InvalidCommitment.selector);
        pool.addToPendingBatch(bytes32(0), nearId1, viewKey1);
    }

    function test_AddToPendingBatch_AllowsNoViewKey() public {
        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 1);
    }

    function test_AddToPendingBatch_SetsFirstSubmissionTime() public {
        uint256 startTime = block.timestamp;

        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        (, uint64 firstTime, ) = pool.getPendingBatchInfo();
        assertEq(firstTime, startTime);
    }

    // ===== BATCH PROCESSING TESTS =====

    function test_BatchProcessing_AutoProcessWhenFull() public {
        vm.startPrank(relayer);

        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode(i)),
                keccak256(abi.encode("near", i)),
                bytes32(0)
            );
        }

        vm.stopPrank();

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 0);
        assertEq(pool.nextLeafIndex(), 10);
        assertTrue(pool.getMerkleRoot() != bytes32(0));
    }

    function test_BatchProcessing_EmitsBatchProcessedEvent() public {
        vm.startPrank(relayer);

        for (uint256 i = 0; i < 9; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode(i)),
                keccak256(abi.encode("near", i)),
                bytes32(0)
            );
        }

        vm.expectEmit(false, false, false, true);
        emit BatchProcessed(10, 10, ShadowSettlement.ProcessReason.BATCH_FULL);

        pool.addToPendingBatch(
            keccak256(abi.encode(uint256(9))),
            keccak256(abi.encode("near", uint256(9))),
            bytes32(0)
        );

        vm.stopPrank();
    }

    function test_BatchProcessing_EmitsMerkleRootUpdatedEvent() public {
        vm.startPrank(relayer);

        for (uint256 i = 0; i < 9; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode(i)),
                keccak256(abi.encode("near", i)),
                bytes32(0)
            );
        }

        vm.recordLogs();
        pool.addToPendingBatch(
            keccak256(abi.encode(uint256(9))),
            keccak256(abi.encode("near", uint256(9))),
            bytes32(0)
        );

        vm.stopPrank();

        // Verify MerkleRootUpdated was emitted
        Vm.Log[] memory entries = vm.getRecordedLogs();
        bool foundMerkleEvent = false;
        for (uint256 i = 0; i < entries.length; i++) {
            if (
                entries[i].topics[0] == keccak256("MerkleRootUpdated(bytes32)")
            ) {
                foundMerkleEvent = true;
                // Root should not be zero
                assertNotEq(entries[i].topics[1], bytes32(0));
            }
        }
        assertTrue(foundMerkleEvent);
    }

    function test_BatchProcessing_TimeoutMechanism() public {
        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));
        pool.addToPendingBatch(commitment2, nearId2, bytes32(0));
        pool.addToPendingBatch(commitment3, nearId3, bytes32(0));
        vm.stopPrank();

        (uint256 count, uint64 firstTime, uint256 timeRemaining) = pool
            .getPendingBatchInfo();
        assertEq(count, 3);
        assertGt(firstTime, 0);
        assertEq(timeRemaining, 30);

        // At 29s — timeout not reached
        vm.warp(block.timestamp + 29);
        vm.prank(user);
        vm.expectRevert(ShadowSettlement.TimeoutNotReached.selector);
        pool.processBatchIfTimeout();

        (count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 3);

        // At 30s — timeout reached
        vm.warp(block.timestamp + 1);
        vm.prank(user);
        pool.processBatchIfTimeout();

        (count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 0);
        assertEq(pool.nextLeafIndex(), 3);
    }

    function test_ProcessBatchIfTimeout_RevertsIfBatchEmpty() public {
        vm.prank(user);
        vm.expectRevert(ShadowSettlement.BatchEmpty.selector);
        pool.processBatchIfTimeout();
    }

    function test_ProcessBatchIfTimeout_AnyoneCanCall() public {
        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        vm.warp(block.timestamp + 31);

        vm.prank(unauthorized);
        pool.processBatchIfTimeout();

        assertEq(pool.nextLeafIndex(), 1);
    }

    function test_BatchProcessing_ViewKeyMappedAfterIntentExists() public {
        // View key query should work immediately after batch processes
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 200);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory details, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        assertEq(total, 1);
        assertEq(details[0].commitment, commitment1);
    }

    // ===== MERKLE TREE TESTS =====

    function test_MerkleTree_EmptyRoot() public view {
        assertEq(pool.getMerkleRoot(), bytes32(0));
    }

    function test_MerkleTree_SingleCommitment() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 300);
        vm.stopPrank();

        bytes32 root = pool.getMerkleRoot();
        assertNotEq(root, bytes32(0));
    }

    function test_MerkleTree_RootChangesWithNewCommitments() public {
        vm.startPrank(relayer);

        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode(i)),
                keccak256(abi.encode("near", i)),
                bytes32(0)
            );
        }

        bytes32 root1 = pool.getMerkleRoot();

        for (uint256 i = 10; i < 20; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode(i)),
                keccak256(abi.encode("near", i)),
                bytes32(0)
            );
        }

        vm.stopPrank();

        bytes32 root2 = pool.getMerkleRoot();
        assertNotEq(root2, root1);
    }

    function test_MerkleTree_DeterministicRoot() public {
        // Same commitments in same order should produce same root
        vm.startPrank(relayer);
        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("deterministic", i)),
                keccak256(abi.encode("near_det", i)),
                bytes32(0)
            );
        }
        vm.stopPrank();

        bytes32 root = pool.getMerkleRoot();

        // Deploy fresh contract and repeat
        vm.prank(owner);
        ShadowSettlement pool2 = new ShadowSettlement(owner, relayer);

        vm.startPrank(relayer);
        for (uint256 i = 0; i < 10; i++) {
            pool2.addToPendingBatch(
                keccak256(abi.encode("deterministic", i)),
                keccak256(abi.encode("near_det", i)),
                bytes32(0)
            );
        }
        vm.stopPrank();

        assertEq(pool2.getMerkleRoot(), root);
    }

    // ===== SOURCE-SIDE SETTLEMENT TESTS (markSettled) =====

    function test_MarkSettled_Success() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 400);

        pool.markSettled(commitment1, nullifier1);
        vm.stopPrank();

        ShadowSettlement.IntentPublic memory intent = pool.getIntent(
            commitment1
        );
        assertTrue(intent.settled);
        assertTrue(pool.isNullifierUsed(nullifier1));
    }

    function test_MarkSettled_EmitsEventWithTimestamp() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 500);

        vm.recordLogs();
        pool.markSettled(commitment1, nullifier1);
        vm.stopPrank();

        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 1);
        // IntentMarkedSettled(bytes32 indexed nullifierHash, bytes32 indexed commitment, uint64 timestamp)
        assertEq(entries[0].topics.length, 3);
        assertEq(entries[0].topics[1], nullifier1);
        assertEq(entries[0].topics[2], commitment1);
        // data contains timestamp
        uint64 ts = abi.decode(entries[0].data, (uint64));
        assertEq(ts, uint64(block.timestamp));
    }

    function test_MarkSettled_RevertsIfCommitmentNotFound() public {
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.CommitmentNotFound.selector);
        pool.markSettled(commitment1, nullifier1);
    }

    function test_MarkSettled_RevertsIfNullifierUsed() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 600);
        pool.markSettled(commitment1, nullifier1);

        _addAndProcessCommitment(commitment2, nearId2, bytes32(0), 700);

        vm.expectRevert(ShadowSettlement.NullifierUsed.selector);
        pool.markSettled(commitment2, nullifier1);
        vm.stopPrank();
    }

    // ===== DESTINATION-SIDE SETTLEMENT TESTS (settleAndRelease) =====

    function test_SettleAndRelease_Success() public {
        // Fund contract
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            1000 ether
        );
        
        assertTrue(pool.isNullifierUsed(nullifier1));
        assertEq(mockToken.balanceOf(user), 1000 ether);
    }

    function test_SettleAndRelease_EmitsCorrectEvent() public {
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        vm.expectEmit(true, true, false, true);
        emit IntentSettled(
            intentId1,
            nullifier1,
            address(mockToken),
            1000 ether,
            uint64(block.timestamp)
        );
        
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            1000 ether
        );
    }

    function test_SettleAndRelease_EventDoesNotIncludeRecipient() public {
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        vm.recordLogs();
        
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            1000 ether
        );
        
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 1); // Only IntentSettled (MockERC20 doesn't emit Transfer)
        
        // Verify IntentSettled event
        assertEq(entries[0].topics[0], keccak256("IntentSettled(bytes32,bytes32,address,uint256,uint64)"));
        
        // Verify topics: [sig, intentId, nullifierHash]
        assertEq(entries[0].topics.length, 3);
        assertEq(entries[0].topics[1], intentId1);
        assertEq(entries[0].topics[2], nullifier1);
        
        // Verify data: [token, amount, timestamp]
        (address token, uint256 amount, uint64 timestamp) = abi.decode(
            entries[0].data,
            (address, uint256, uint64)
        );
        assertEq(token, address(mockToken));
        assertEq(amount, 1000 ether);
        assertEq(timestamp, uint64(block.timestamp));
        
        // ✅ NO recipient in topics or data!
    }

    function test_SettleAndRelease_RevertsIfNullifierUsed() public {
        mockToken.mint(address(pool), 2000 ether);
        
        vm.startPrank(relayer);
        
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            1000 ether
        );
        
        vm.expectRevert(ShadowSettlement.NullifierUsed.selector);
        pool.settleAndRelease(
            intentId2,
            nullifier1,
            unauthorized,
            address(mockToken),
            1000 ether
        );
        
        vm.stopPrank();
    }

    function test_SettleAndRelease_RevertsIfTokenNotWhitelisted() public {
        MockERC20 unauthorizedToken = new MockERC20();
        unauthorizedToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.TokenNotWhitelisted.selector);
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(unauthorizedToken),
            1000 ether
        );
    }

    function test_SettleAndRelease_RevertsIfZeroRecipient() public {
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.InvalidRecipient.selector);
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            address(0),
            address(mockToken),
            1000 ether
        );
    }

    function test_SettleAndRelease_RevertsIfZeroAmount() public {
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.InvalidAmount.selector);
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            0
        );
    }

    function test_SettleAndRelease_RevertsWhenPaused() public {
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(owner);
        pool.pause();
        
        vm.prank(relayer);
        vm.expectRevert();
        pool.settleAndRelease(
            intentId1,
            nullifier1,
            user,
            address(mockToken),
            1000 ether
        );
    }

    function test_SettleAndRelease_MultipleIntentsWithDifferentNullifiers() public {
        mockToken.mint(address(pool), 3000 ether);
        
        vm.startPrank(relayer);
        
        pool.settleAndRelease(intentId1, nullifier1, user, address(mockToken), 1000 ether);
        pool.settleAndRelease(intentId2, nullifier2, unauthorized, address(mockToken), 1000 ether);
        
        vm.stopPrank();
        
        assertEq(mockToken.balanceOf(user), 1000 ether);
        assertEq(mockToken.balanceOf(unauthorized), 1000 ether);
        assertTrue(pool.isNullifierUsed(nullifier1));
        assertTrue(pool.isNullifierUsed(nullifier2));
    }

    // ===== TOKEN WHITELIST TESTS =====

    function test_SetTokenWhitelist_Success() public {
        address newToken = address(0x9999);
        
        vm.prank(owner);
        vm.expectEmit(true, false, false, true);
        emit TokenWhitelistUpdated(newToken, true);
        
        pool.setTokenWhitelist(newToken, true);
        assertTrue(pool.whitelistedTokens(newToken));
    }

    function test_SetTokenWhitelist_OnlyOwner() public {
        address newToken = address(0x9999);
        
        vm.prank(unauthorized);
        vm.expectRevert();
        pool.setTokenWhitelist(newToken, true);
        
        vm.prank(owner);
        pool.setTokenWhitelist(newToken, true);
        assertTrue(pool.whitelistedTokens(newToken));
    }

    function test_SetTokenWhitelist_Delist() public {
        vm.prank(owner);
        pool.setTokenWhitelist(address(mockToken), false);
        
        assertFalse(pool.whitelistedTokens(address(mockToken)));
        
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        vm.expectRevert(ShadowSettlement.TokenNotWhitelisted.selector);
        pool.settleAndRelease(intentId1, nullifier1, user, address(mockToken), 1000 ether);
    }

    function test_SetTokenWhitelist_RevertsIfAlreadyWhitelisted() public {
        // mockToken was already whitelisted in setUp
        vm.prank(owner);
        vm.expectRevert(ShadowSettlement.TokenWhitelistUnchanged.selector);
        pool.setTokenWhitelist(address(mockToken), true);
    }

    function test_SetTokenWhitelist_RevertsIfAlreadyDelisted() public {
        address newToken = address(0x9999);
        // newToken was never whitelisted (default false)
        vm.prank(owner);
        vm.expectRevert(ShadowSettlement.TokenWhitelistUnchanged.selector);
        pool.setTokenWhitelist(newToken, false);
    }

    // ===== VIEW KEY TESTS =====

    function test_ViewKey_GetIntentsByViewKey() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 800);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory details, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);

        assertEq(total, 1);
        assertEq(details.length, 1);
        assertEq(details[0].commitment, commitment1);
        assertEq(details[0].nearIntentsId, nearId1);
        assertFalse(details[0].settled);
    }

    function test_ViewKey_GetMultipleIntentsByViewKey() public {
        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);
        pool.addToPendingBatch(commitment2, nearId2, viewKey1);
        pool.addToPendingBatch(commitment3, nearId3, viewKey1);
        _fillBatchWithFiller(900, 7);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory details, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);

        assertEq(total, 3);
        assertEq(details[0].commitment, commitment1);
        assertEq(details[1].commitment, commitment2);
        assertEq(details[2].commitment, commitment3);
    }

    function test_ViewKey_ReturnsNearIntentsId() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 1000);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory details, ) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);

        assertEq(details[0].nearIntentsId, nearId1);
    }

    function test_ViewKey_ReturnsEmptyIfInvalid() public {
        (ShadowSettlement.IntentDetail[] memory details, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        
        assertEq(total, 0);
        assertEq(details.length, 0);
    }

    function test_ViewKey_SeparateViewKeysAreIsolated() public {
        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);
        pool.addToPendingBatch(commitment2, nearId2, viewKey2);
        _fillBatchWithFiller(1100, 8);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory details1, uint256 total1) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        (ShadowSettlement.IntentDetail[] memory details2, uint256 total2) = pool
            .getIntentsByViewKey(viewKey2, 0, 0);

        assertEq(total1, 1);
        assertEq(total2, 1);
        assertEq(details1[0].commitment, commitment1);
        assertEq(details2[0].commitment, commitment2);
    }

    function test_ViewKey_ReflectsSettlementStatus() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 1200);
        vm.stopPrank();

        // Before settlement
        (ShadowSettlement.IntentDetail[] memory before, ) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        assertFalse(before[0].settled);

        // Settle
        vm.prank(relayer);
        pool.markSettled(commitment1, nullifier1);

        // After settlement
        (ShadowSettlement.IntentDetail[] memory after_, ) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        assertTrue(after_[0].settled);
    }

    // ===== PAGINATION TESTS =====

    function test_Pagination_OffsetAndLimit() public {
        vm.prank(owner);
        pool.updateBatchConfig(5, 30);

        vm.startPrank(relayer);
        for (uint256 i = 0; i < 5; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("paginated", i)),
                keccak256(abi.encode("near_page", i)),
                viewKey1
            );
        }
        vm.stopPrank();

        // Get first 2
        (ShadowSettlement.IntentDetail[] memory page1, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 2);
        assertEq(total, 5);
        assertEq(page1.length, 2);
        assertEq(
            page1[0].commitment,
            keccak256(abi.encode("paginated", uint256(0)))
        );
        assertEq(
            page1[1].commitment,
            keccak256(abi.encode("paginated", uint256(1)))
        );

        // Get next 2
        (ShadowSettlement.IntentDetail[] memory page2, ) = pool
            .getIntentsByViewKey(viewKey1, 2, 2);
        assertEq(page2.length, 2);
        assertEq(
            page2[0].commitment,
            keccak256(abi.encode("paginated", uint256(2)))
        );
        assertEq(
            page2[1].commitment,
            keccak256(abi.encode("paginated", uint256(3)))
        );

        // Get last 1
        (ShadowSettlement.IntentDetail[] memory page3, ) = pool
            .getIntentsByViewKey(viewKey1, 4, 2);
        assertEq(page3.length, 1);
        assertEq(
            page3[0].commitment,
            keccak256(abi.encode("paginated", uint256(4)))
        );
    }

    function test_Pagination_ZeroLimitReturnsAll() public {
        vm.prank(owner);
        pool.updateBatchConfig(3, 30);

        vm.startPrank(relayer);
        for (uint256 i = 0; i < 3; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("all", i)),
                keccak256(abi.encode("near_all", i)),
                viewKey1
            );
        }
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory all, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 0, 0);
        assertEq(total, 3);
        assertEq(all.length, 3);
    }

    function test_Pagination_OffsetBeyondTotalReturnsEmpty() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 1300);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory result, uint256 total) = pool
            .getIntentsByViewKey(viewKey1, 100, 10);
        assertEq(total, 1);
        assertEq(result.length, 0);
    }

    function test_Pagination_LimitLargerThanRemaining() public {
        vm.prank(owner);
        pool.updateBatchConfig(2, 30);

        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, viewKey1);
        pool.addToPendingBatch(commitment2, nearId2, viewKey1);
        vm.stopPrank();

        (ShadowSettlement.IntentDetail[] memory result, ) = pool
            .getIntentsByViewKey(viewKey1, 0, 999);
        assertEq(result.length, 2);
    }

    // ===== PUBLIC VIEW FUNCTION TESTS =====

    function test_GetIntent_ReturnsPublicInfoOnly() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 1400);
        vm.stopPrank();

        ShadowSettlement.IntentPublic memory info = pool.getIntent(commitment1);
        assertEq(info.commitment, commitment1);
        assertGt(info.submittedAt, 0);
        assertFalse(info.settled);
    }

    function test_GetIntent_RevertsIfNotFound() public {
        vm.expectRevert(ShadowSettlement.CommitmentNotFound.selector);
        pool.getIntent(commitment1);
    }

    function test_CommitmentExists_ReturnsTrueAfterProcessing() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 1500);
        vm.stopPrank();

        assertTrue(pool.commitmentExists(commitment1));
    }

    function test_CommitmentExists_ReturnsFalseIfNotRegistered() public view {
        assertFalse(pool.commitmentExists(commitment1));
    }

    function test_IsNullifierUsed_ReturnsFalseInitially() public view {
        assertFalse(pool.isNullifierUsed(nullifier1));
    }

    function test_IsNullifierUsed_ReturnsTrueAfterSourceSettlement() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 1600);
        pool.markSettled(commitment1, nullifier1);
        vm.stopPrank();

        assertTrue(pool.isNullifierUsed(nullifier1));
    }

    function test_IsNullifierUsed_ReturnsTrueAfterDestinationSettlement() public {
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(relayer);
        pool.settleAndRelease(intentId1, nullifier1, user, address(mockToken), 1000 ether);

        assertTrue(pool.isNullifierUsed(nullifier1));
    }

    // ===== RESCUE TOKENS TESTS =====

    function test_RescueTokens_Success() public {
        mockToken.mint(address(pool), 100 ether);
        
        vm.prank(owner);
        pool.rescueTokens(address(mockToken), owner, 100 ether);
        
        assertEq(mockToken.balanceOf(owner), 100 ether);
        assertEq(mockToken.balanceOf(address(pool)), 0);
    }

    function test_RescueTokens_OnlyOwner() public {
        mockToken.mint(address(pool), 100 ether);
        
        vm.prank(unauthorized);
        vm.expectRevert();
        pool.rescueTokens(address(mockToken), unauthorized, 100 ether);
        
        vm.prank(owner);
        pool.rescueTokens(address(mockToken), owner, 100 ether);
        assertEq(mockToken.balanceOf(owner), 100 ether);
    }

    // ===== PAUSABLE TESTS =====

    function test_Pause_OnlyOwner() public {
        vm.prank(unauthorized);
        vm.expectRevert();
        pool.pause();
    }

    function test_Pause_BlocksAddToPendingBatch() public {
        vm.prank(owner);
        pool.pause();

        vm.prank(relayer);
        vm.expectRevert();
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));
    }

    function test_Pause_BlocksMarkSettled() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, bytes32(0), 1800);
        vm.stopPrank();

        vm.prank(owner);
        pool.pause();

        vm.prank(relayer);
        vm.expectRevert();
        pool.markSettled(commitment1, nullifier1);
    }

    function test_Pause_BlocksSettleAndRelease() public {
        mockToken.mint(address(pool), 1000 ether);
        
        vm.prank(owner);
        pool.pause();

        vm.prank(relayer);
        vm.expectRevert();
        pool.settleAndRelease(intentId1, nullifier1, user, address(mockToken), 1000 ether);
    }

    function test_Pause_BlocksProcessBatchIfTimeout() public {
        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        vm.warp(block.timestamp + 31);

        vm.prank(owner);
        pool.pause();

        vm.prank(user);
        vm.expectRevert();
        pool.processBatchIfTimeout();
    }

    function test_Pause_ViewFunctionsStillWork() public {
        vm.startPrank(relayer);
        _addAndProcessCommitment(commitment1, nearId1, viewKey1, 1900);
        vm.stopPrank();

        vm.prank(owner);
        pool.pause();

        // All view functions should still work
        pool.getIntent(commitment1);
        pool.getIntentsByViewKey(viewKey1, 0, 0);
        pool.commitmentExists(commitment1);
        pool.isNullifierUsed(nullifier1);
        pool.getMerkleRoot();
        pool.getPendingBatchInfo();
        pool.isRelayerAuthorized(relayer);
    }

    function test_Unpause_RestoresOperations() public {
        vm.prank(owner);
        pool.pause();

        vm.prank(owner);
        pool.unpause();

        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 1);
    }

    function test_Unpause_OnlyOwner() public {
        vm.prank(owner);
        pool.pause();

        vm.prank(unauthorized);
        vm.expectRevert();
        pool.unpause();
    }

    // ===== ETH REJECTION TESTS =====

    function test_Receive_RevertsOnDirectETH() public {
        vm.deal(user, 1 ether);
        vm.prank(user);
        (bool success, ) = address(pool).call{value: 1 ether}("");
        assertFalse(success);
    }

    function test_Fallback_RevertsOnUnknownCall() public {
        vm.deal(user, 1 ether);
        vm.prank(user);
        (bool success, ) = address(pool).call{value: 1 ether}(
            abi.encodeWithSignature("nonExistentFunction()")
        );
        assertFalse(success);
    }

    // ===== ADMIN TESTS =====

    function test_UpdateBatchConfig_Success() public {
        vm.prank(owner);
        pool.updateBatchConfig(20, 60);

        assertEq(pool.batchSize(), 20);
        assertEq(pool.batchTimeout(), 60);
    }

    function test_UpdateBatchConfig_RevertsIfInvalidSize() public {
        vm.startPrank(owner);

        vm.expectRevert(ShadowSettlement.InvalidBatchSize.selector);
        pool.updateBatchConfig(0, 30);

        vm.expectRevert(ShadowSettlement.InvalidBatchSize.selector);
        pool.updateBatchConfig(101, 30);

        vm.stopPrank();
    }

    function test_UpdateBatchConfig_RevertsIfInvalidTimeout() public {
        vm.prank(owner);
        vm.expectRevert(ShadowSettlement.InvalidTimeout.selector);
        pool.updateBatchConfig(10, 0);
    }

    function test_UpdateBatchConfig_OnlyOwner() public {
        vm.prank(unauthorized);
        vm.expectRevert();
        pool.updateBatchConfig(20, 60);
    }

    function test_UpdateBatchConfig_EmitsEvent() public {
        vm.prank(owner);
        vm.expectEmit(false, false, false, true);
        emit BatchConfigUpdated(25, 45);
        pool.updateBatchConfig(25, 45);
    }

    // ===== EDGE CASE TESTS =====

    function testFuzz_AddToPendingBatch_RandomCommitments(
        bytes32 commitment,
        bytes32 nearId
    ) public {
        vm.assume(commitment != bytes32(0));

        vm.prank(relayer);
        pool.addToPendingBatch(commitment, nearId, bytes32(0));

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 1);
    }

    function test_EdgeCase_BatchSizeOne() public {
        vm.prank(owner);
        pool.updateBatchConfig(1, 30);

        vm.prank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 0);
        assertEq(pool.nextLeafIndex(), 1);
    }

    function test_EdgeCase_MaxBatchSize() public {
        vm.prank(owner);
        pool.updateBatchConfig(100, 30);

        vm.startPrank(relayer);
        for (uint256 i = 0; i < 100; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("max", i)),
                keccak256(abi.encode("near_max", i)),
                bytes32(0)
            );
        }
        vm.stopPrank();

        (uint256 count, , ) = pool.getPendingBatchInfo();
        assertEq(count, 0);
        assertEq(pool.nextLeafIndex(), 100);
    }

    function test_EdgeCase_MultipleTimeouts() public {
        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));
        vm.warp(block.timestamp + 31);
        vm.stopPrank();

        vm.prank(user);
        pool.processBatchIfTimeout();

        vm.startPrank(relayer);
        pool.addToPendingBatch(commitment2, nearId2, bytes32(0));
        vm.warp(block.timestamp + 31);
        vm.stopPrank();

        vm.prank(user);
        pool.processBatchIfTimeout();

        assertEq(pool.nextLeafIndex(), 2);
    }

    function test_EdgeCase_MixedProcessingReasons() public {
        vm.startPrank(relayer);

        // First batch: auto-processed at 10
        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("mixed", i)),
                keccak256(abi.encode("near_mixed", i)),
                bytes32(0)
            );
        }

        // Second batch: processed via timeout
        pool.addToPendingBatch(commitment1, nearId1, bytes32(0));
        vm.stopPrank();

        vm.warp(block.timestamp + 31);
        vm.prank(user);
        pool.processBatchIfTimeout();

        assertEq(pool.nextLeafIndex(), 11);
    }

    function test_EdgeCase_BatchCountResetIsO1() public {
        vm.startPrank(relayer);

        // First batch
        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("batch1", i)),
                keccak256(abi.encode("near_b1", i)),
                bytes32(0)
            );
        }

        assertEq(pool.batchCount(), 0);

        // Second batch works fine
        for (uint256 i = 0; i < 10; i++) {
            pool.addToPendingBatch(
                keccak256(abi.encode("batch2", i)),
                keccak256(abi.encode("near_b2", i)),
                bytes32(0)
            );
        }

        vm.stopPrank();

        assertEq(pool.batchCount(), 0);
        assertEq(pool.nextLeafIndex(), 20);
    }
}
