use starknet::ContractAddress;

//! # ShadowSettlement - Privacy-Preserving Cross-Chain Bridge
//!
//! ## Commitment Formula (enforced client-side)
//! Frontend MUST generate commitments as:
//!   `commitment = poseidon_hash_many([secret, nullifier, amount_low, amount_high, token, destChain])`
//!
//! Where:
//! - secret, nullifier: 252-bit random values
//! - amount_low, amount_high: u256 amount split into two u128 values
//! - token: felt252 representation of token address
//! - destChain: felt252 chain ID (1 = EVM, 2 = StarkNet)
//!
//! Security benefits:
//! - Prevents commitment reuse across different swap parameters
//! - Prevents cross-swap attacks
//! - Industry standard approach (Tornado Cash, Aztec, etc.)
//!
//! Note: Contract does NOT validate the formula (it's a hash).
//!       Security enforced by frontend + Merkle proof verification.

// ===== STRUCTS =====

/// Internal struct — never returned to external callers with viewKey
#[derive(Drop, Serde, Copy, starknet::Store)]
pub struct Intent {
    pub commitment: felt252,
    pub near_intents_id: felt252,
    pub view_key: felt252,
    pub submitted_at: u64,
    pub settled: bool,
}

/// Public-safe intent data (no viewKey, no nearIntentsId)
#[derive(Drop, Serde, Copy)]
pub struct IntentPublic {
    pub commitment: felt252,
    pub submitted_at: u64,
    pub settled: bool,
}

/// Full intent data returned to view key holder
#[derive(Drop, Serde, Copy)]
pub struct IntentDetail {
    pub commitment: felt252,
    pub near_intents_id: felt252,
    pub submitted_at: u64,
    pub settled: bool,
}

/// Remote chain Merkle root snapshot
#[derive(Drop, Serde, Copy, starknet::Store)]
pub struct RemoteRootSnapshot {
    pub root: felt252,
    pub leaf_count_low: u128,
    pub leaf_count_high: u128,
    pub synced_at: u64,
    pub verified: bool,
}

#[derive(Drop, Serde)]
pub enum ProcessReason {
    BatchFull,
    TimeoutReached,
}

// ===== INTERFACES =====

#[starknet::interface]
pub trait IERC20<TContractState> {
    fn transfer(ref self: TContractState, recipient: ContractAddress, amount: u256) -> bool;
    fn transfer_from(
        ref self: TContractState, sender: ContractAddress, recipient: ContractAddress, amount: u256,
    ) -> bool;
    fn balance_of(self: @TContractState, account: ContractAddress) -> u256;
}

#[starknet::interface]
pub trait IShadowSettlement<TContractState> {
    // ===== SOURCE SIDE =====
    fn add_to_pending_batch(
        ref self: TContractState, commitment: felt252, near_intents_id: felt252, view_key: felt252,
    );
    fn process_batch_if_timeout(ref self: TContractState);
    fn mark_settled(ref self: TContractState, commitment: felt252, nullifier_hash: felt252);

    // ===== CROSS-CHAIN SYNC =====
    fn sync_merkle_root(
        ref self: TContractState, chain_id: felt252, root: felt252, leaf_count: u256,
    );
    fn verify_remote_root(ref self: TContractState, chain_id: felt252, snapshot_index: u256);

    // ===== DESTINATION SIDE =====
    fn settle_and_release(
        ref self: TContractState,
        intent_id: felt252,
        nullifier_hash: felt252,
        recipient: ContractAddress,
        token: ContractAddress,
        amount: u256,
    );

    // ===== PUBLIC VIEW =====
    fn get_intent(self: @TContractState, commitment: felt252) -> IntentPublic;
    fn commitment_exists(self: @TContractState, commitment: felt252) -> bool;
    fn is_nullifier_used(self: @TContractState, nullifier_hash: felt252) -> bool;
    fn get_merkle_root(self: @TContractState) -> felt252;
    fn get_pending_batch_info(self: @TContractState) -> (u64, u64, u64);
    fn is_relayer_authorized(self: @TContractState, relayer: ContractAddress) -> bool;
    fn is_root_verifier(self: @TContractState, verifier: ContractAddress) -> bool;
    fn is_token_whitelisted(self: @TContractState, token: ContractAddress) -> bool;
    fn get_latest_remote_root(self: @TContractState, chain_id: felt252) -> RemoteRootSnapshot;
    fn get_latest_verified_remote_root(
        self: @TContractState, chain_id: felt252,
    ) -> (RemoteRootSnapshot, u256);
    fn get_remote_root_snapshot(
        self: @TContractState, chain_id: felt252, snapshot_index: u256,
    ) -> RemoteRootSnapshot;
    fn get_remote_root_count(self: @TContractState, chain_id: felt252) -> u256;

    // ===== VIEW KEY =====
    fn get_intents_by_view_key(
        self: @TContractState, view_key: felt252, offset: u64, limit: u64,
    ) -> (Array<IntentDetail>, u64);

    // ===== ADMIN =====
    fn update_batch_config(ref self: TContractState, new_batch_size: u64, new_timeout: u64);
    fn set_relayer_status(ref self: TContractState, relayer: ContractAddress, authorized: bool);
    fn set_root_verifier_status(
        ref self: TContractState, verifier: ContractAddress, authorized: bool,
    );
    fn set_token_whitelist(ref self: TContractState, token: ContractAddress, whitelisted: bool);
    fn rescue_tokens(
        ref self: TContractState, token: ContractAddress, to: ContractAddress, amount: u256,
    );
    fn pause(ref self: TContractState);
    fn unpause(ref self: TContractState);
}

#[starknet::contract]
pub mod ShadowSettlement {
    use core::num::traits::{DivRem, Zero};
    use starknet::storage::*;
    use starknet::{ContractAddress, get_block_timestamp, get_caller_address};
    use super::{
        IERC20Dispatcher, IERC20DispatcherTrait, Intent, IntentDetail, IntentPublic, ProcessReason,
        RemoteRootSnapshot,
    };

    // ===== CONSTANTS =====

    const TREE_HEIGHT: u8 = 20;
    const MIN_BATCH_SIZE: u64 = 1;
    const MAX_BATCH_SIZE: u64 = 100;
    const DEFAULT_BATCH_SIZE: u64 = 10;
    const DEFAULT_TIMEOUT: u64 = 30;

    // ===== STORAGE =====

    #[storage]
    pub struct Storage {
        // --- Source side: intent storage ---
        intents: Map<felt252, Intent>,
        used_nullifiers: Map<felt252, bool>,
        // View key mappings
        view_key_commitment_count: Map<felt252, u64>,
        view_key_commitments: Map<(felt252, u64), felt252>,
        // Merkle tree
        next_leaf_index: u64,
        current_root: felt252,
        filled_subtrees: Map<u8, felt252>,
        zeros: Map<u8, felt252>,
        commitment_to_index: Map<felt252, u64>,
        // Pending batch (mapping-based for O(1) reset)
        batch_commitments: Map<u64, felt252>,
        batch_near_intents_ids: Map<u64, felt252>,
        batch_view_keys: Map<u64, felt252>,
        batch_count: u64,
        batch_first_submission_time: u64,
        // --- Cross-chain sync: remote chain roots ---
        // chainId -> array of snapshots
        remote_root_snapshots: Map<(felt252, u256), RemoteRootSnapshot>,
        remote_root_count: Map<felt252, u256>,
        latest_remote_root_index: Map<felt252, u256>,
        // --- Config ---
        batch_size: u64,
        batch_timeout: u64,
        whitelisted_tokens: Map<ContractAddress, bool>,
        // --- Access control ---
        authorized_relayers: Map<ContractAddress, bool>,
        root_verifiers: Map<ContractAddress, bool>,
        owner: ContractAddress,
        paused: bool,
    }

    // ===== EVENTS =====

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        CommitmentAdded: CommitmentAdded,
        BatchProcessed: BatchProcessed,
        MerkleRootUpdated: MerkleRootUpdated,
        IntentMarkedSettled: IntentMarkedSettled,
        RemoteRootSynced: RemoteRootSynced,
        RemoteRootVerified: RemoteRootVerified,
        IntentSettled: IntentSettled,
        BatchConfigUpdated: BatchConfigUpdated,
        RelayerStatusChanged: RelayerStatusChanged,
        RootVerifierStatusChanged: RootVerifierStatusChanged,
        TokenWhitelistUpdated: TokenWhitelistUpdated,
        Paused: Paused,
        Unpaused: Unpaused,
    }

    #[derive(Drop, starknet::Event)]
    pub struct CommitmentAdded {
        #[key]
        pub commitment: felt252,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BatchProcessed {
        #[key]
        pub batch_id: u64,
        pub commitments_count: u64,
        pub reason: ProcessReason,
    }

    #[derive(Drop, starknet::Event)]
    pub struct MerkleRootUpdated {
        #[key]
        pub new_root: felt252,
    }

    #[derive(Drop, starknet::Event)]
    pub struct IntentMarkedSettled {
        #[key]
        pub nullifier_hash: felt252,
        #[key]
        pub commitment: felt252,
        pub timestamp: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RemoteRootSynced {
        #[key]
        pub chain_id: felt252,
        #[key]
        pub root: felt252,
        pub leaf_count: u256,
        pub snapshot_index: u256,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RemoteRootVerified {
        #[key]
        pub chain_id: felt252,
        pub snapshot_index: u256,
        pub verifier: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    pub struct IntentSettled {
        #[key]
        pub intent_id: felt252,
        #[key]
        pub nullifier_hash: felt252,
        pub token: ContractAddress,
        pub amount: u256,
        pub timestamp: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BatchConfigUpdated {
        pub new_batch_size: u64,
        pub new_timeout: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RelayerStatusChanged {
        #[key]
        pub relayer: ContractAddress,
        pub authorized: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RootVerifierStatusChanged {
        #[key]
        pub verifier: ContractAddress,
        pub authorized: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TokenWhitelistUpdated {
        #[key]
        pub token: ContractAddress,
        pub whitelisted: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct Paused {}

    #[derive(Drop, starknet::Event)]
    pub struct Unpaused {}

    // ===== CONSTRUCTOR =====

    #[constructor]
    fn constructor(
        ref self: ContractState, owner: ContractAddress, initial_relayer: ContractAddress,
    ) {
        self.owner.write(owner);
        self.authorized_relayers.entry(initial_relayer).write(true);
        self.root_verifiers.entry(initial_relayer).write(true);
        self.batch_size.write(DEFAULT_BATCH_SIZE);
        self.batch_timeout.write(DEFAULT_TIMEOUT);
        self.paused.write(false);

        // Initialize Merkle tree zeros
        self.zeros.entry(0).write(0);
        let mut i: u8 = 1;
        while i < TREE_HEIGHT {
            let prev = self.zeros.entry(i - 1).read();
            self.zeros.entry(i).write(self._hash_pair(prev, prev));
            i += 1;
        };
    }

    // ===== IMPLEMENTATION =====

    #[abi(embed_v0)]
    pub impl ShadowSettlementImpl of super::IShadowSettlement<ContractState> {
        // ==============================================================
        //                     SOURCE SIDE FUNCTIONS
        // ==============================================================

        fn add_to_pending_batch(
            ref self: ContractState,
            commitment: felt252,
            near_intents_id: felt252,
            view_key: felt252,
        ) {
            self._assert_not_paused();
            self._only_relayer();
            assert!(commitment != 0, "Invalid commitment");

            let existing = self.intents.entry(commitment).read();
            assert!(existing.commitment == 0, "Commitment already exists");

            // Tree capacity check (mirrors EVM TreeFull error)
            let tree_capacity: u64 = 1048576; // 2^20
            assert!(self.next_leaf_index.read() < tree_capacity, "Tree full");

            let count = self.batch_count.read();
            if count == 0 {
                self.batch_first_submission_time.write(get_block_timestamp());
            }

            self.batch_commitments.entry(count).write(commitment);
            self.batch_near_intents_ids.entry(count).write(near_intents_id);
            self.batch_view_keys.entry(count).write(view_key);
            self.batch_count.write(count + 1);

            self.emit(CommitmentAdded { commitment });

            if count + 1 >= self.batch_size.read() {
                self._process_batch(ProcessReason::BatchFull);
            }
        }

        fn process_batch_if_timeout(ref self: ContractState) {
            self._assert_not_paused();
            let count = self.batch_count.read();
            assert!(count > 0, "Batch is empty");
            let elapsed = get_block_timestamp() - self.batch_first_submission_time.read();
            assert!(elapsed >= self.batch_timeout.read(), "Timeout not reached");
            self._process_batch(ProcessReason::TimeoutReached);
        }

        fn mark_settled(ref self: ContractState, commitment: felt252, nullifier_hash: felt252) {
            self._assert_not_paused();
            self._only_relayer();
            let mut intent = self.intents.entry(commitment).read();
            assert!(intent.commitment != 0, "Commitment not found");
            assert!(!self.used_nullifiers.entry(nullifier_hash).read(), "Nullifier already used");

            intent.settled = true;
            self.intents.entry(commitment).write(intent);
            self.used_nullifiers.entry(nullifier_hash).write(true);

            self
                .emit(
                    IntentMarkedSettled {
                        nullifier_hash, commitment, timestamp: get_block_timestamp(),
                    },
                );
        }

        // ==============================================================
        //                   CROSS-CHAIN SYNC FUNCTIONS
        // ==============================================================

        fn sync_merkle_root(
            ref self: ContractState, chain_id: felt252, root: felt252, leaf_count: u256,
        ) {
            self._assert_not_paused();
            self._only_relayer();
            assert!(chain_id != 0, "Invalid chain_id");
            assert!(root != 0, "Invalid root");
            assert!(leaf_count > 0_u256, "Invalid leaf count");

            let snapshot = RemoteRootSnapshot {
                root,
                leaf_count_low: leaf_count.low,
                leaf_count_high: leaf_count.high,
                synced_at: get_block_timestamp(),
                verified: false,
            };

            let count = self.remote_root_count.entry(chain_id).read();
            self.remote_root_snapshots.entry((chain_id, count)).write(snapshot);
            self.remote_root_count.entry(chain_id).write(count + 1);
            self.latest_remote_root_index.entry(chain_id).write(count);

            self.emit(RemoteRootSynced { chain_id, root, leaf_count, snapshot_index: count });
        }

        fn verify_remote_root(ref self: ContractState, chain_id: felt252, snapshot_index: u256) {
            self._assert_not_paused();
            self._only_root_verifier();

            let count = self.remote_root_count.entry(chain_id).read();
            let count_u256: u256 = count;
            assert!(snapshot_index < count_u256, "Snapshot not found");

            let mut snapshot = self.remote_root_snapshots.entry((chain_id, snapshot_index)).read();
            assert!(!snapshot.verified, "Root already verified");

            snapshot.verified = true;
            self.remote_root_snapshots.entry((chain_id, snapshot_index)).write(snapshot);

            self
                .emit(
                    RemoteRootVerified {
                        chain_id, snapshot_index, verifier: get_caller_address(),
                    },
                );
        }

        // ==============================================================
        //                   DESTINATION SIDE FUNCTIONS
        // ==============================================================

        fn settle_and_release(
            ref self: ContractState,
            intent_id: felt252,
            nullifier_hash: felt252,
            recipient: ContractAddress,
            token: ContractAddress,
            amount: u256,
        ) {
            self._assert_not_paused();
            self._only_relayer();
            assert!(!self.used_nullifiers.entry(nullifier_hash).read(), "Nullifier already used");
            assert!(recipient.is_non_zero(), "Invalid recipient");
            assert!(self.whitelisted_tokens.entry(token).read(), "Token not whitelisted");
            assert!(amount > 0_u256, "Invalid amount");

            self.used_nullifiers.entry(nullifier_hash).write(true);

            let erc20 = IERC20Dispatcher { contract_address: token };
            let success = erc20.transfer(recipient, amount);
            assert!(success, "Transfer failed");

            self
                .emit(
                    IntentSettled {
                        intent_id, nullifier_hash, token, amount, timestamp: get_block_timestamp(),
                    },
                );
        }

        // ==============================================================
        //                     PUBLIC VIEW FUNCTIONS
        // ==============================================================

        fn get_intent(self: @ContractState, commitment: felt252) -> IntentPublic {
            let intent = self.intents.entry(commitment).read();
            assert!(intent.commitment != 0, "Commitment not found");
            IntentPublic {
                commitment: intent.commitment,
                submitted_at: intent.submitted_at,
                settled: intent.settled,
            }
        }

        fn commitment_exists(self: @ContractState, commitment: felt252) -> bool {
            self.intents.entry(commitment).read().commitment != 0
        }

        fn is_nullifier_used(self: @ContractState, nullifier_hash: felt252) -> bool {
            self.used_nullifiers.entry(nullifier_hash).read()
        }

        fn get_merkle_root(self: @ContractState) -> felt252 {
            self.current_root.read()
        }

        fn get_pending_batch_info(self: @ContractState) -> (u64, u64, u64) {
            let count = self.batch_count.read();
            let first_time = self.batch_first_submission_time.read();
            let time_remaining = if count > 0 {
                let elapsed = get_block_timestamp() - first_time;
                let timeout = self.batch_timeout.read();
                if elapsed >= timeout {
                    0
                } else {
                    timeout - elapsed
                }
            } else {
                0
            };
            (count, first_time, time_remaining)
        }

        fn is_relayer_authorized(self: @ContractState, relayer: ContractAddress) -> bool {
            self.authorized_relayers.entry(relayer).read()
        }

        fn is_root_verifier(self: @ContractState, verifier: ContractAddress) -> bool {
            self.root_verifiers.entry(verifier).read()
        }

        fn is_token_whitelisted(self: @ContractState, token: ContractAddress) -> bool {
            self.whitelisted_tokens.entry(token).read()
        }

        fn get_latest_remote_root(self: @ContractState, chain_id: felt252) -> RemoteRootSnapshot {
            let count = self.remote_root_count.entry(chain_id).read();
            assert!(count > 0, "No snapshots found");
            let latest_index = self.latest_remote_root_index.entry(chain_id).read();
            self.remote_root_snapshots.entry((chain_id, latest_index)).read()
        }

        fn get_latest_verified_remote_root(
            self: @ContractState, chain_id: felt252,
        ) -> (RemoteRootSnapshot, u256) {
            let count = self.remote_root_count.entry(chain_id).read();
            assert!(count > 0, "No snapshots found");

            let mut i = count;
            while i > 0 {
                i -= 1;
                let snapshot = self.remote_root_snapshots.entry((chain_id, i)).read();
                if snapshot.verified {
                    return (snapshot, i);
                }
            };

            panic!("No verified snapshot found");
        }

        fn get_remote_root_snapshot(
            self: @ContractState, chain_id: felt252, snapshot_index: u256,
        ) -> RemoteRootSnapshot {
            let count = self.remote_root_count.entry(chain_id).read();
            let count_u256: u256 = count;
            assert!(snapshot_index < count_u256, "Snapshot not found");
            self.remote_root_snapshots.entry((chain_id, snapshot_index)).read()
        }

        fn get_remote_root_count(self: @ContractState, chain_id: felt252) -> u256 {
            self.remote_root_count.entry(chain_id).read()
        }

        // ==============================================================
        //                      VIEW KEY FUNCTIONS
        // ==============================================================

        fn get_intents_by_view_key(
            self: @ContractState, view_key: felt252, offset: u64, limit: u64,
        ) -> (Array<IntentDetail>, u64) {
            let total = self.view_key_commitment_count.entry(view_key).read();

            // Return empty array without revealing if view key exists
            if total == 0 || offset >= total {
                return (array![], total);
            }

            let remaining = total - offset;
            let count = if limit == 0 || limit > remaining {
                remaining
            } else {
                limit
            };

            let mut result = array![];
            let mut i: u64 = 0;
            while i < count {
                let commitment = self.view_key_commitments.entry((view_key, offset + i)).read();
                let intent = self.intents.entry(commitment).read();
                result
                    .append(
                        IntentDetail {
                            commitment: intent.commitment,
                            near_intents_id: intent.near_intents_id,
                            submitted_at: intent.submitted_at,
                            settled: intent.settled,
                        },
                    );
                i += 1;
            }
            (result, total)
        }

        // ==============================================================
        //                      ADMIN FUNCTIONS
        // ==============================================================

        fn update_batch_config(ref self: ContractState, new_batch_size: u64, new_timeout: u64) {
            self._only_owner();
            assert!(
                new_batch_size >= MIN_BATCH_SIZE && new_batch_size <= MAX_BATCH_SIZE,
                "Invalid batch size",
            );
            assert!(new_timeout > 0, "Invalid timeout");

            self.batch_size.write(new_batch_size);
            self.batch_timeout.write(new_timeout);
            self.emit(BatchConfigUpdated { new_batch_size, new_timeout });
        }

        fn set_relayer_status(ref self: ContractState, relayer: ContractAddress, authorized: bool) {
            self._only_owner();
            self.authorized_relayers.entry(relayer).write(authorized);
            self.emit(RelayerStatusChanged { relayer, authorized });
        }

        fn set_root_verifier_status(
            ref self: ContractState, verifier: ContractAddress, authorized: bool,
        ) {
            self._only_owner();
            self.root_verifiers.entry(verifier).write(authorized);
            self.emit(RootVerifierStatusChanged { verifier, authorized });
        }

        fn set_token_whitelist(ref self: ContractState, token: ContractAddress, whitelisted: bool) {
            self._only_owner();
            let current = self.whitelisted_tokens.entry(token).read();
            assert!(current != whitelisted, "Token whitelist status unchanged");
            self.whitelisted_tokens.entry(token).write(whitelisted);
            self.emit(TokenWhitelistUpdated { token, whitelisted });
        }

        fn rescue_tokens(
            ref self: ContractState, token: ContractAddress, to: ContractAddress, amount: u256,
        ) {
            self._only_owner();
            let erc20 = IERC20Dispatcher { contract_address: token };
            erc20.transfer(to, amount);
        }

        fn pause(ref self: ContractState) {
            self._only_owner();
            assert!(!self.paused.read(), "Already paused");
            self.paused.write(true);
            self.emit(Paused {});
        }

        fn unpause(ref self: ContractState) {
            self._only_owner();
            assert!(self.paused.read(), "Not paused");
            self.paused.write(false);
            self.emit(Unpaused {});
        }
    }

    // ==============================================================
    //                     INTERNAL FUNCTIONS
    // ==============================================================

    #[generate_trait]
    impl InternalImpl of InternalTrait {
        fn _only_relayer(self: @ContractState) {
            assert!(self.authorized_relayers.entry(get_caller_address()).read(), "Unauthorized");
        }

        fn _only_root_verifier(self: @ContractState) {
            assert!(self.root_verifiers.entry(get_caller_address()).read(), "Unauthorized");
        }

        fn _only_owner(self: @ContractState) {
            assert!(get_caller_address() == self.owner.read(), "Not owner");
        }

        fn _assert_not_paused(self: @ContractState) {
            assert!(!self.paused.read(), "Contract is paused");
        }

        fn _hash_pair(self: @ContractState, a: felt252, b: felt252) -> felt252 {
            let a_u256: u256 = a.into();
            let b_u256: u256 = b.into();
            if a_u256 < b_u256 {
                core::poseidon::poseidon_hash_span(array![a, b].span())
            } else {
                core::poseidon::poseidon_hash_span(array![b, a].span())
            }
        }

        fn _insert_commitment(ref self: ContractState, commitment: felt252) {
            let index = self.next_leaf_index.read();
            self.commitment_to_index.entry(commitment).write(index);
            self.next_leaf_index.write(index + 1);

            let mut current_hash = commitment;
            let mut current_index = index;
            let mut height: u8 = 0;

            while height < TREE_HEIGHT {
                let divisor: NonZero<u64> = 2_u64.try_into().unwrap();
                let (_, remainder) = DivRem::div_rem(current_index, divisor);

                let (left, right) = if remainder == 0 {
                    self.filled_subtrees.entry(height).write(current_hash);
                    (current_hash, self.zeros.entry(height).read())
                } else {
                    (self.filled_subtrees.entry(height).read(), current_hash)
                };

                current_hash = self._hash_pair(left, right);
                current_index /= 2;
                height += 1;
            }

            self.current_root.write(current_hash);
        }

        fn _process_batch(ref self: ContractState, reason: ProcessReason) {
            let count = self.batch_count.read();
            assert!(count > 0, "Batch is empty");

            let batch_id = self.next_leaf_index.read();

            let mut i: u64 = 0;
            while i < count {
                let commitment = self.batch_commitments.entry(i).read();
                let near_id = self.batch_near_intents_ids.entry(i).read();
                let view_key = self.batch_view_keys.entry(i).read();

                self
                    .intents
                    .entry(commitment)
                    .write(
                        Intent {
                            commitment,
                            near_intents_id: near_id,
                            view_key,
                            submitted_at: get_block_timestamp(),
                            settled: false,
                        },
                    );

                self._insert_commitment(commitment);

                if view_key != 0 {
                    let vk_count = self.view_key_commitment_count.entry(view_key).read();
                    self.view_key_commitments.entry((view_key, vk_count)).write(commitment);
                    self.view_key_commitment_count.entry(view_key).write(vk_count + 1);
                }
                i += 1;
            }

            self.emit(BatchProcessed { batch_id, commitments_count: count, reason });
            self.emit(MerkleRootUpdated { new_root: self.current_root.read() });

            self.batch_count.write(0);
        }
    }
}
