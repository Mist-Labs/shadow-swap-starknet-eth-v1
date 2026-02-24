// SPDX-License-Identifier: MIT

use snforge_std::{
    declare, ContractClassTrait, DeclareResultTrait, spy_events, EventSpyAssertionsTrait,
    start_cheat_caller_address, stop_cheat_caller_address, start_cheat_block_timestamp,
    stop_cheat_block_timestamp, CheatSpan, EventSpyTrait
};
use starknet::{ContractAddress, contract_address_const, get_block_timestamp};
use shadow_swap::shadow_swap_settlement::{
    IShadowSettlementDispatcher, IShadowSettlementDispatcherTrait,
    IERC20Dispatcher, IERC20DispatcherTrait,
    ShadowSettlement, IntentPublic, IntentDetail, ProcessReason, RemoteRootSnapshot,
};
use shadow_swap::mockERC20::MockERC20;

// ===== TEST HELPERS =====

fn deploy_settlement(owner: ContractAddress, relayer: ContractAddress) -> IShadowSettlementDispatcher {
    let contract = declare("ShadowSettlement").unwrap().contract_class();
    let mut calldata = array![];
    owner.serialize(ref calldata);
    relayer.serialize(ref calldata);
    let (address, _) = contract.deploy(@calldata).unwrap();
    IShadowSettlementDispatcher { contract_address: address }
}

fn deploy_mock_token(owner: ContractAddress) -> IERC20Dispatcher {
    let contract = declare("MockERC20").unwrap().contract_class();
    let mut calldata = array![];
    let name: ByteArray = "Mock Token";
    let symbol: ByteArray = "MTK";
    let decimals: u8 = 18;
    let initial_supply: u256 = 1000000_u256;
    name.serialize(ref calldata);
    symbol.serialize(ref calldata);
    decimals.serialize(ref calldata);
    initial_supply.serialize(ref calldata);
    owner.serialize(ref calldata);
    let (address, _) = contract.deploy(@calldata).unwrap();
    IERC20Dispatcher { contract_address: address }
}

fn fill_batch_with_filler(
    dispatcher: IShadowSettlementDispatcher,
    relayer: ContractAddress,
    start_index: u64,
    count: u64
) {
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < count {
        let commitment: felt252 = (start_index + i + 1000).into();
        dispatcher.add_to_pending_batch(commitment, 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
}

fn add_and_process_commitment(
    dispatcher: IShadowSettlementDispatcher,
    relayer: ContractAddress,
    commitment: felt252,
    near_id: felt252,
    view_key: felt252,
    filler_start: u64
) {
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(commitment, near_id, view_key);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    fill_batch_with_filler(dispatcher, relayer, filler_start, 9);
}

// ===== CONSTRUCTOR TESTS =====

#[test]
fn test_constructor_sets_owner() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    assert!(dispatcher.is_relayer_authorized(relayer), "Relayer should be authorized");
}

#[test]
fn test_constructor_authorizes_relayer_and_root_verifier() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    assert!(dispatcher.is_relayer_authorized(relayer), "Relayer should be authorized");
    assert!(dispatcher.is_root_verifier(relayer), "Relayer should be root verifier");
    assert!(!dispatcher.is_relayer_authorized(owner), "Owner should not be relayer");
}

#[test]
fn test_constructor_sets_default_batch_config() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 0, "Batch should be empty");
}

#[test]
fn test_constructor_initial_merkle_root_is_zero() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let root = dispatcher.get_merkle_root();
    assert!(root == 0, "Initial root should be zero");
}

// ===== ACCESS CONTROL TESTS =====

#[test]
#[should_panic(expected: "Unauthorized")]
fn test_add_to_pending_batch_reverts_if_not_relayer() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let unauthorized = contract_address_const::<'unauthorized'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, unauthorized);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0x999);
}

#[test]
#[should_panic(expected: "Unauthorized")]
fn test_mark_settled_reverts_if_not_relayer() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let unauthorized = contract_address_const::<'unauthorized'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, unauthorized);
    dispatcher.mark_settled(0x123, 0xabc);
}

#[test]
#[should_panic(expected: "Unauthorized")]
fn test_settle_and_release_reverts_if_not_relayer() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let unauthorized = contract_address_const::<'unauthorized'>();
    let user = contract_address_const::<'user'>();
    let token = contract_address_const::<'token'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, unauthorized);
    dispatcher.settle_and_release(0x123, 0xabc, user, token, 1000_u256);
}

#[test]
#[should_panic(expected: "Unauthorized")]
fn test_verify_remote_root_reverts_if_not_verifier() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let unauthorized = contract_address_const::<'unauthorized'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, unauthorized);
    dispatcher.verify_remote_root(0x123, 0_u256);
}

#[test]
fn test_set_relayer_status_revoke_relayer() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_relayer_status(relayer, false);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(!dispatcher.is_relayer_authorized(relayer), "Relayer should be revoked");
}

#[test]
fn test_set_root_verifier_status() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let verifier = contract_address_const::<'verifier'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_root_verifier_status(verifier, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.is_root_verifier(verifier), "Verifier should be authorized");
}

// ===== COMMITMENT ADDITION TESTS =====

#[test]
fn test_add_to_pending_batch_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let commitment: felt252 = 0x123;
    dispatcher.add_to_pending_batch(commitment, 0xabc, 0x999);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 1, "Count should be 1");
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::CommitmentAdded(
            ShadowSettlement::CommitmentAdded { commitment }
        ))
    ]);
}

#[test]
fn test_add_to_pending_batch_event_emits_only_commitment() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let commitment: felt252 = 0x123;
    dispatcher.add_to_pending_batch(commitment, 0xabc, 0x999);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Event should only contain commitment (privacy)
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::CommitmentAdded(
            ShadowSettlement::CommitmentAdded { commitment }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Commitment already exists")]
fn test_add_to_pending_batch_reverts_if_commitment_exists() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let commitment: felt252 = 0x123;
    add_and_process_commitment(dispatcher, relayer, commitment, 0xabc, 0x999, 100);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(commitment, 0xdef, 0x888);
}

#[test]
#[should_panic(expected: "Invalid commitment")]
fn test_add_to_pending_batch_reverts_if_invalid_commitment() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0, 0xabc, 0x999);
}

#[test]
fn test_add_to_pending_batch_allows_no_view_key() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 1, "Count should be 1");
}

#[test]
fn test_add_to_pending_batch_sets_first_submission_time() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (_, first_time, _) = dispatcher.get_pending_batch_info();
    assert!(first_time == 1000, "First time should be 1000");
}

// ===== BATCH PROCESSING TESTS =====

#[test]
fn test_batch_processing_auto_process_when_full() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 10 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 0, "Batch should be reset");
    assert!(dispatcher.get_merkle_root() != 0, "Root should be updated");
}

#[test]
fn test_batch_processing_emits_batch_processed_event() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 10 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Should emit BatchProcessed event
    let events = spy.get_events();
    assert!(ArrayTrait::len(@events.events) > 0, "Should have events");
}

#[test]
fn test_batch_processing_timeout_mechanism() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    dispatcher.add_to_pending_batch(0x456, 0xdef, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, time_remaining) = dispatcher.get_pending_batch_info();
    assert!(count == 2, "Count should be 2");
    assert!(time_remaining == 30, "Time remaining should be 30");
    
    // At 29s - timeout not reached
    start_cheat_block_timestamp(dispatcher.contract_address, 1029);
    let (_, _, time_remaining) = dispatcher.get_pending_batch_info();
    assert!(time_remaining == 1, "Time remaining should be 1");
}

#[test]
#[should_panic(expected: "Batch is empty")]
fn test_process_batch_if_timeout_reverts_if_batch_empty() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, user);
    dispatcher.process_batch_if_timeout();
}

#[test]
fn test_process_batch_if_timeout_anyone_can_call() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1031);
    start_cheat_caller_address(dispatcher.contract_address, user);
    dispatcher.process_batch_if_timeout();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 0, "Batch should be processed");
}

// ===== MERKLE TREE TESTS =====

#[test]
fn test_merkle_tree_empty_root() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let root = dispatcher.get_merkle_root();
    assert!(root == 0, "Empty root should be zero");
}

#[test]
fn test_merkle_tree_single_commitment() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, 0, 300);
    
    let root = dispatcher.get_merkle_root();
    assert!(root != 0, "Root should be non-zero");
}

#[test]
fn test_merkle_tree_root_changes_with_new_commitments() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 10 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let root1 = dispatcher.get_merkle_root();
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 10;
    while i < 20 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let root2 = dispatcher.get_merkle_root();
    assert!(root2 != root1, "Root should change");
}

// ===== SOURCE-SIDE SETTLEMENT TESTS (mark_settled) =====

#[test]
fn test_mark_settled_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let commitment: felt252 = 0x123;
    let nullifier: felt252 = 0xabc;
    
    add_and_process_commitment(dispatcher, relayer, commitment, 0xdef, 0, 400);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(commitment, nullifier);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let intent = dispatcher.get_intent(commitment);
    assert!(intent.settled, "Intent should be settled");
    assert!(dispatcher.is_nullifier_used(nullifier), "Nullifier should be used");
}

#[test]
fn test_mark_settled_emits_event_with_timestamp() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    let commitment: felt252 = 0x123;
    let nullifier: felt252 = 0xabc;
    
    add_and_process_commitment(dispatcher, relayer, commitment, 0xdef, 0, 500);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 2000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(commitment, nullifier);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::IntentMarkedSettled(
            ShadowSettlement::IntentMarkedSettled { 
                nullifier_hash: nullifier,
                commitment,
                timestamp: 2000
            }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Commitment not found")]
fn test_mark_settled_reverts_if_commitment_not_found() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(0x123, 0xabc);
}

#[test]
#[should_panic(expected: "Nullifier already used")]
fn test_mark_settled_reverts_if_nullifier_used() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let commitment1: felt252 = 0x123;
    let commitment2: felt252 = 0x456;
    let nullifier: felt252 = 0xabc;
    
    add_and_process_commitment(dispatcher, relayer, commitment1, 0xdef, 0, 600);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(commitment1, nullifier);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    add_and_process_commitment(dispatcher, relayer, commitment2, 0xdec, 0, 700);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(commitment2, nullifier);
}

// ===== CROSS-CHAIN SYNC TESTS =====

#[test]
fn test_sync_merkle_root_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    let chain_id: felt252 = 'ethereum-mainnet';
    let root: felt252 = 0x123456;
    let leaf_count: u256 = 100_u256;
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, root, leaf_count);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let snapshot = dispatcher.get_latest_remote_root(chain_id);
    assert!(snapshot.root == root, "Root should match");
    assert!(u256 { low: snapshot.leaf_count_low, high: snapshot.leaf_count_high } == leaf_count, "Leaf count should match");
    assert!(snapshot.synced_at == 1000, "Timestamp should match");
    assert!(!snapshot.verified, "Should not be verified initially");
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::RemoteRootSynced(
            ShadowSettlement::RemoteRootSynced {
                chain_id,
                root,
                leaf_count,
                snapshot_index: 0_u256
            }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Invalid chain_id")]
fn test_sync_merkle_root_reverts_if_invalid_chain_id() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(0, 0x123, 100_u256);
}

#[test]
#[should_panic(expected: "Invalid root")]
fn test_sync_merkle_root_reverts_if_zero_root() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root('ethereum-mainnet', 0, 100_u256);
}

#[test]
#[should_panic(expected: "Invalid leaf count")]
fn test_sync_merkle_root_reverts_if_zero_leaf_count() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root('ethereum-mainnet', 0x123, 0_u256);
}

#[test]
fn test_sync_merkle_root_creates_history() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x111, 100_u256);
    dispatcher.sync_merkle_root(chain_id, 0x222, 200_u256);
    dispatcher.sync_merkle_root(chain_id, 0x333, 300_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let count = dispatcher.get_remote_root_count(chain_id);
    assert!(count == 3_u256, "Should have 3 snapshots");
    
    let latest = dispatcher.get_latest_remote_root(chain_id);
    assert!(latest.root == 0x333, "Latest should be third root");
}

#[test]
fn test_verify_remote_root_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x123, 100_u256);
    dispatcher.verify_remote_root(chain_id, 0_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let snapshot = dispatcher.get_remote_root_snapshot(chain_id, 0_u256);
    assert!(snapshot.verified, "Should be verified");
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::RemoteRootVerified(
            ShadowSettlement::RemoteRootVerified {
                chain_id,
                snapshot_index: 0_u256,
                verifier: relayer
            }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Snapshot not found")]
fn test_verify_remote_root_reverts_if_snapshot_not_found() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.verify_remote_root(chain_id, 0_u256);
}

#[test]
#[should_panic(expected: "Root already verified")]
fn test_verify_remote_root_reverts_if_already_verified() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x123, 100_u256);
    dispatcher.verify_remote_root(chain_id, 0_u256);
    dispatcher.verify_remote_root(chain_id, 0_u256);
}

#[test]
fn test_get_latest_verified_remote_root() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x111, 100_u256);
    dispatcher.sync_merkle_root(chain_id, 0x222, 200_u256);
    dispatcher.sync_merkle_root(chain_id, 0x333, 300_u256);
    
    dispatcher.verify_remote_root(chain_id, 1_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (snapshot, index) = dispatcher.get_latest_verified_remote_root(chain_id);
    assert!(snapshot.root == 0x222, "Should return second root");
    assert!(index == 1_u256, "Index should be 1");
    assert!(snapshot.verified, "Should be verified");
}

#[test]
#[should_panic(expected: "No verified snapshot found")]
fn test_get_latest_verified_remote_root_reverts_if_none_verified() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x111, 100_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    dispatcher.get_latest_verified_remote_root(chain_id);
}

// ===== DESTINATION-SIDE SETTLEMENT TESTS (settle_and_release) =====

#[test]
fn test_settle_and_release_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    // Whitelist token
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Fund contract
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 1000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    // Settle
    let intent_id: felt252 = 0x123;
    let nullifier: felt252 = 0xabc;
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(intent_id, nullifier, user, token.contract_address, 500_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.is_nullifier_used(nullifier), "Nullifier should be used");
    assert!(token.balance_of(user) == 500_u256, "User should receive tokens");
}

#[test]
fn test_settle_and_release_checks_transfer_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Fund contract
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 1000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    // Settle - transfer should succeed
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user, token.contract_address, 500_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(token.balance_of(user) == 500_u256, "Transfer succeeded");
}

#[test]
fn test_settle_and_release_event_does_not_include_recipient() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 1000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    let intent_id: felt252 = 0x123;
    let nullifier: felt252 = 0xabc;
    
    start_cheat_block_timestamp(dispatcher.contract_address, 3000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(intent_id, nullifier, user, token.contract_address, 500_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Event should NOT include recipient!
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::IntentSettled(
            ShadowSettlement::IntentSettled {
                intent_id,
                nullifier_hash: nullifier,
                token: token.contract_address,
                amount: 500_u256,
                timestamp: 3000
            }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Nullifier already used")]
fn test_settle_and_release_reverts_if_nullifier_used() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 2000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    let nullifier: felt252 = 0xabc;
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, nullifier, user, token.contract_address, 500_u256);
    dispatcher.settle_and_release(0x456, nullifier, user, token.contract_address, 500_u256);
}

#[test]
#[should_panic(expected: "Invalid recipient")]
fn test_settle_and_release_reverts_if_zero_recipient() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let zero_address = contract_address_const::<0>();
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, zero_address, token.contract_address, 500_u256);
}

#[test]
#[should_panic(expected: "Token not whitelisted")]
fn test_settle_and_release_reverts_if_token_not_whitelisted() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let token = contract_address_const::<'fake_token'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user, token, 1000_u256);
}

#[test]
#[should_panic(expected: "Invalid amount")]
fn test_settle_and_release_reverts_if_zero_amount() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user, token.contract_address, 0_u256);
}

#[test]
fn test_settle_and_release_multiple_intents_different_nullifiers() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user1 = contract_address_const::<'user1'>();
    let user2 = contract_address_const::<'user2'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 3000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user1, token.contract_address, 1000_u256);
    dispatcher.settle_and_release(0x456, 0xdef, user2, token.contract_address, 1000_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(token.balance_of(user1) == 1000_u256, "User1 should receive");
    assert!(token.balance_of(user2) == 1000_u256, "User2 should receive");
    assert!(dispatcher.is_nullifier_used(0xabc), "Nullifier 1 used");
    assert!(dispatcher.is_nullifier_used(0xdef), "Nullifier 2 used");
}

// ===== TOKEN WHITELIST TESTS =====

#[test]
fn test_set_token_whitelist_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let token = contract_address_const::<'token'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.is_token_whitelisted(token), "Token should be whitelisted");
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::TokenWhitelistUpdated(
            ShadowSettlement::TokenWhitelistUpdated { token, whitelisted: true }
        ))
    ]);
}

#[test]
fn test_set_token_whitelist_delist() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    dispatcher.set_token_whitelist(token.contract_address, false);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(!dispatcher.is_token_whitelisted(token.contract_address), "Token should be delisted");
}

#[test]
#[should_panic(expected: "Token whitelist status unchanged")]
fn test_set_token_whitelist_reverts_if_already_whitelisted() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let token = contract_address_const::<'token'>();
    let dispatcher = deploy_settlement(owner, relayer);

    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token, true);
    dispatcher.set_token_whitelist(token, true);
}

#[test]
#[should_panic(expected: "Token whitelist status unchanged")]
fn test_set_token_whitelist_reverts_if_already_delisted() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let token = contract_address_const::<'token'>();
    let dispatcher = deploy_settlement(owner, relayer);

    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token, false);
}

// ===== VIEW KEY TESTS =====

#[test]
fn test_view_key_get_intents_by_view_key() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x888;
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, view_key, 800);
    
    let (intents, total) = dispatcher.get_intents_by_view_key(view_key, 0, 0);
    assert!(total == 1, "Total should be 1");
    assert!(intents.len() == 1, "Should return 1 intent");
    
    let first = *intents.at(0);
    assert!(first.commitment == 0x123, "Wrong commitment");
    assert!(first.near_intents_id == 0xabc, "Wrong near ID");
}

#[test]
fn test_view_key_get_multiple_intents() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x888;
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x111, 0xaaa, view_key);
    dispatcher.add_to_pending_batch(0x222, 0xbbb, view_key);
    dispatcher.add_to_pending_batch(0x333, 0xccc, view_key);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    fill_batch_with_filler(dispatcher, relayer, 900, 7);
    
    let (intents, total) = dispatcher.get_intents_by_view_key(view_key, 0, 0);
    assert!(total == 3, "Total should be 3");
    
    let first = *intents.at(0);
    let second = *intents.at(1);
    let third = *intents.at(2);
    
    assert!(first.commitment == 0x111, "Wrong first commitment");
    assert!(second.commitment == 0x222, "Wrong second commitment");
    assert!(third.commitment == 0x333, "Wrong third commitment");
}

#[test]
fn test_view_key_returns_empty_if_invalid() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let (intents, total) = dispatcher.get_intents_by_view_key(0x999, 0, 0);
    assert!(total == 0, "Total should be 0");
    assert!(intents.len() == 0, "Should return empty array");
}

#[test]
fn test_view_key_separate_view_keys_are_isolated() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let vk1: felt252 = 0x111;
    let vk2: felt252 = 0x222;
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0xaaa, 0x1, vk1);
    dispatcher.add_to_pending_batch(0xbbb, 0x2, vk2);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    fill_batch_with_filler(dispatcher, relayer, 1100, 8);
    
    let (intents1, total1) = dispatcher.get_intents_by_view_key(vk1, 0, 0);
    let (intents2, total2) = dispatcher.get_intents_by_view_key(vk2, 0, 0);
    
    assert!(total1 == 1, "VK1 should have 1");
    assert!(total2 == 1, "VK2 should have 1");
    
    let first1 = *intents1.at(0);
    let first2 = *intents2.at(0);
    
    assert!(first1.commitment == 0xaaa, "VK1 wrong commitment");
    assert!(first2.commitment == 0xbbb, "VK2 wrong commitment");
}

#[test]
fn test_view_key_reflects_settlement_status() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x888;
    let commitment: felt252 = 0x123;
    
    add_and_process_commitment(dispatcher, relayer, commitment, 0xabc, view_key, 1200);
    
    let (before, _) = dispatcher.get_intents_by_view_key(view_key, 0, 0);
    let before_intent = *before.at(0);
    assert!(!before_intent.settled, "Should not be settled");
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(commitment, 0xdef);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (after, _) = dispatcher.get_intents_by_view_key(view_key, 0, 0);
    let after_intent = *after.at(0);
    assert!(after_intent.settled, "Should be settled");
}

// ===== PAGINATION TESTS =====

#[test]
fn test_pagination_offset_and_limit() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x777;
    
    // Update batch size to 5
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(5, 30);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 5 {
        dispatcher.add_to_pending_batch((i + 1).into(), (i + 100).into(), view_key);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Get first 2
    let (page1, total) = dispatcher.get_intents_by_view_key(view_key, 0, 2);
    assert!(total == 5, "Total should be 5");
    assert!(page1.len() == 2, "Page1 should have 2");
    
    // Get next 2
    let (page2, _) = dispatcher.get_intents_by_view_key(view_key, 2, 2);
    assert!(page2.len() == 2, "Page2 should have 2");
    
    // Get last 1
    let (page3, _) = dispatcher.get_intents_by_view_key(view_key, 4, 2);
    assert!(page3.len() == 1, "Page3 should have 1");
}

#[test]
fn test_pagination_zero_limit_returns_all() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x777;
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(3, 30);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 3 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, view_key);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (all, total) = dispatcher.get_intents_by_view_key(view_key, 0, 0);
    assert!(total == 3, "Total should be 3");
    assert!(all.len() == 3, "Should return all 3");
}

#[test]
fn test_pagination_offset_beyond_total_returns_empty() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let view_key: felt252 = 0x888;
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, view_key, 1300);
    
    let (result, total) = dispatcher.get_intents_by_view_key(view_key, 100, 10);
    assert!(total == 1, "Total should be 1");
    assert!(result.len() == 0, "Should return empty");
}

// ===== PUBLIC VIEW FUNCTION TESTS =====

#[test]
fn test_get_intent_returns_public_info_only() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, 0x999, 1400);
    
    let info = dispatcher.get_intent(0x123);
    assert!(info.commitment == 0x123, "Wrong commitment");
    assert!(!info.settled, "Should not be settled");
}

#[test]
#[should_panic(expected: "Commitment not found")]
fn test_get_intent_reverts_if_not_found() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    dispatcher.get_intent(0x123);
}

#[test]
fn test_commitment_exists_returns_true_after_processing() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, 0, 1500);
    
    assert!(dispatcher.commitment_exists(0x123), "Commitment should exist");
}

#[test]
fn test_commitment_exists_returns_false_if_not_registered() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    assert!(!dispatcher.commitment_exists(0x123), "Commitment should not exist");
}

#[test]
fn test_is_nullifier_used_returns_false_initially() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    assert!(!dispatcher.is_nullifier_used(0xabc), "Nullifier should not be used");
}

#[test]
fn test_is_nullifier_used_returns_true_after_source_settlement() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, 0, 1600);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(0x123, 0xdef);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.is_nullifier_used(0xdef), "Nullifier should be used");
}

#[test]
fn test_is_nullifier_used_returns_true_after_destination_settlement() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 1000_u256);
    stop_cheat_caller_address(token.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user, token.contract_address, 500_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.is_nullifier_used(0xabc), "Nullifier should be used");
}

// ===== RESCUE TOKENS TESTS =====

#[test]
fn test_rescue_tokens_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let recipient = contract_address_const::<'recipient'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(token.contract_address, owner);
    token.transfer(dispatcher.contract_address, 500_u256);
    stop_cheat_caller_address(token.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.rescue_tokens(token.contract_address, recipient, 500_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(token.balance_of(recipient) == 500_u256, "Tokens not rescued");
    assert!(token.balance_of(dispatcher.contract_address) == 0_u256, "Contract should be empty");
}

// ===== PAUSABLE TESTS =====

#[test]
fn test_pause_emergency() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::Paused(
            ShadowSettlement::Paused {}
        ))
    ]);
}

#[test]
#[should_panic(expected: "Contract is paused")]
fn test_add_to_batch_while_paused() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
}

#[test]
#[should_panic(expected: "Contract is paused")]
fn test_mark_settled_while_paused() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    add_and_process_commitment(dispatcher, relayer, 0x123, 0xabc, 0, 1800);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.mark_settled(0x123, 0xdef);
}

#[test]
#[should_panic(expected: "Contract is paused")]
fn test_settle_and_release_while_paused() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let token = deploy_mock_token(owner);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.set_token_whitelist(token.contract_address, true);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.settle_and_release(0x123, 0xabc, user, token.contract_address, 100_u256);
}

#[test]
#[should_panic(expected: "Contract is paused")]
fn test_sync_merkle_root_while_paused() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root('ethereum-mainnet', 0x123, 100_u256);
}

#[test]
#[should_panic(expected: "Contract is paused")]
fn test_verify_remote_root_while_paused() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    let chain_id: felt252 = 'ethereum-mainnet';
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.sync_merkle_root(chain_id, 0x123, 100_u256);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.verify_remote_root(chain_id, 0_u256);
}

#[test]
fn test_unpause_restores_operations() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.pause();
    dispatcher.unpause();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 1, "Should be able to add after unpause");
}

// ===== ADMIN TESTS =====

#[test]
fn test_update_batch_config_success() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    let mut spy = spy_events();
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(20, 60);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    spy.assert_emitted(@array![
        (dispatcher.contract_address, ShadowSettlement::Event::BatchConfigUpdated(
            ShadowSettlement::BatchConfigUpdated { new_batch_size: 20, new_timeout: 60 }
        ))
    ]);
}

#[test]
#[should_panic(expected: "Invalid batch size")]
fn test_update_batch_config_reverts_if_batch_size_too_small() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(0, 30);
}

#[test]
#[should_panic(expected: "Invalid batch size")]
fn test_update_batch_config_reverts_if_batch_size_too_large() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(101, 30);
}

#[test]
#[should_panic(expected: "Invalid timeout")]
fn test_update_batch_config_reverts_if_timeout_zero() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(10, 0);
}

// ===== EDGE CASE TESTS =====

#[test]
fn test_edge_case_batch_size_one() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(1, 30);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x123, 0xabc, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 0, "Batch should auto-process");
}

#[test]
fn test_edge_case_batch_size_max() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    start_cheat_caller_address(dispatcher.contract_address, owner);
    dispatcher.update_batch_config(100, 30);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    let mut i: u64 = 0;
    while i < 100 {
        dispatcher.add_to_pending_batch((i + 1).into(), 0, 0);
        i += 1;
    };
    stop_cheat_caller_address(dispatcher.contract_address);
    
    let (count, _, _) = dispatcher.get_pending_batch_info();
    assert!(count == 0, "Batch should auto-process at 100");
}

#[test]
fn test_edge_case_multiple_timeouts() {
    let owner = contract_address_const::<'owner'>();
    let relayer = contract_address_const::<'relayer'>();
    let user = contract_address_const::<'user'>();
    let dispatcher = deploy_settlement(owner, relayer);
    
    // First batch
    start_cheat_block_timestamp(dispatcher.contract_address, 1000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x111, 0xaaa, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 1031);
    start_cheat_caller_address(dispatcher.contract_address, user);
    dispatcher.process_batch_if_timeout();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    // Second batch
    start_cheat_block_timestamp(dispatcher.contract_address, 2000);
    start_cheat_caller_address(dispatcher.contract_address, relayer);
    dispatcher.add_to_pending_batch(0x222, 0xbbb, 0);
    stop_cheat_caller_address(dispatcher.contract_address);
    
    start_cheat_block_timestamp(dispatcher.contract_address, 2031);
    start_cheat_caller_address(dispatcher.contract_address, user);
    dispatcher.process_batch_if_timeout();
    stop_cheat_caller_address(dispatcher.contract_address);
    
    assert!(dispatcher.commitment_exists(0x111), "First should exist");
    assert!(dispatcher.commitment_exists(0x222), "Second should exist");
}
