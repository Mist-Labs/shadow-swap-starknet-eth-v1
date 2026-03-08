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
//!
//! ## AVNU Post-Swap (Any chain → StarkNet non-STRK token)
//! When NEAR delivers STRK to this contract but the user wants another token
//! (ETH, USDC, USDT, wBTC, etc.):
//!   1. Relayer fetches AVNU quote offchain
//!   2. Relayer calls settle_and_release with dest_token + expected_dest_amount
//!      + min_dest_amount + routes
//!   3. Contract resets STRK approval to 0, then approves AVNU for sell_amount
//!   4. Contract calls multi_route_swap (expected_dest_amount as AVNU reference,
//!      min_dest_amount as hard slippage floor enforced by both AVNU and contract)
//!   5. AVNU swaps STRK → dest token, delivers to this contract
//!   6. Contract verifies received >= min_dest_amount via balance delta
//!   7. Contract resets AVNU approval to 0 (asserted, not dropped)
//!   8. Contract transfers dest token to recipient
//! When dest_token is zero or equals token: direct transfer, no swap.

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

/// AVNU routing hop.
/// Constructed offchain by the relayer from the AVNU quote API and passed
/// directly into settle_and_release — the contract forwards it to AVNU verbatim.
/// `percent` is in AVNU internal units (sum of all hops = 1_000_000).
/// `additional_swap_params` is DEX-specific calldata (e.g. Ekubo pool key fields).
#[derive(Drop, Serde)]
pub struct Route {
    pub token_from: ContractAddress,
    pub token_to: ContractAddress,
    pub exchange_address: ContractAddress,
    pub percent: u128,
    pub additional_swap_params: Array<felt252>,
}

// ===== INTERFACES =====

#[starknet::interface]
pub trait IERC20<TContractState> {
    fn transfer(ref self: TContractState, recipient: ContractAddress, amount: u256) -> bool;
    fn transfer_from(
        ref self: TContractState, sender: ContractAddress, recipient: ContractAddress, amount: u256,
    ) -> bool;
    fn approve(ref self: TContractState, spender: ContractAddress, amount: u256) -> bool;
    fn balance_of(self: @TContractState, account: ContractAddress) -> u256;
}

/// Minimal AVNU Exchange interface — only the function we call.
/// Full AVNU interface: https://github.com/avnu-labs/avnu-contracts
/// Mainnet exchange address: 0x04270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f
#[starknet::interface]
pub trait IAvnuExchange<TContractState> {
    fn multi_route_swap(
        ref self: TContractState,
        token_from_address: ContractAddress,
        token_from_amount: u256,
        token_to_address: ContractAddress,
        token_to_amount: u256,
        token_to_min_amount: u256,
        beneficiary: ContractAddress,
        integrator_fee_amount_bps: u128,
        integrator_fee_recipient: ContractAddress,
        routes: Array<Route>,
    ) -> bool;
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
    /// Settle a cross-chain intent and release tokens to the recipient.
    ///
    /// `token`                — token delivered to this contract by NEAR (always STRK on StarkNet)
    /// `amount`               — amount delivered
    /// `dest_token`           — token the user wants to receive. Pass zero address when it
    ///                          equals `token` (no swap). Non-zero triggers an AVNU DEX swap.
    /// `expected_dest_amount` — AVNU quote buy_amount used as AVNU's token_to_amount (route
    ///                          scoring reference). Must be >= min_dest_amount. Pass 0 when
    ///                          no swap is required.
    /// `min_dest_amount`      — minimum acceptable output (slippage floor enforced by both
    ///                          AVNU and this contract). Must be > 0 when swap needed.
    /// `routes`               — AVNU routing hops from the offchain quote. Pass empty array
    ///                          when no swap is required.
    fn settle_and_release(
        ref self: TContractState,
        intent_id: felt252,
        nullifier_hash: felt252,
        recipient: ContractAddress,
        token: ContractAddress,
        amount: u256,
        dest_token: ContractAddress,
        expected_dest_amount: u256,
        min_dest_amount: u256,
        routes: Array<Route>,
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
    fn get_avnu_exchange(self: @TContractState) -> ContractAddress;

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
    fn set_avnu_exchange(ref self: TContractState, exchange: ContractAddress);
    fn rescue_tokens(
        ref self: TContractState, token: ContractAddress, to: ContractAddress, amount: u256,
    );
    fn pause(ref self: TContractState);
    fn unpause(ref self: TContractState);

    // ===== OWNERSHIP =====
    fn propose_owner(ref self: TContractState, new_owner: ContractAddress);
    fn accept_ownership(ref self: TContractState);
    fn cancel_ownership_proposal(ref self: TContractState);
    fn get_pending_owner(self: @TContractState) -> ContractAddress;
}

#[starknet::contract]
pub mod ShadowSettlement {
    use core::num::traits::{DivRem, Zero};
    use starknet::storage::*;
    use starknet::{
        ContractAddress, get_block_timestamp, get_caller_address, get_contract_address,
    };
    use super::{
        IAvnuExchangeDispatcher, IAvnuExchangeDispatcherTrait, IERC20Dispatcher,
        IERC20DispatcherTrait, Intent, IntentDetail, IntentPublic, ProcessReason, RemoteRootSnapshot,
        Route,
    };

    // ===== CONSTANTS =====

    const TREE_HEIGHT: u8 = 20;
    const MIN_BATCH_SIZE: u64 = 1;
    const MAX_BATCH_SIZE: u64 = 100;
    const DEFAULT_BATCH_SIZE: u64 = 10;
    const DEFAULT_TIMEOUT: u64 = 30;
    const MIN_TIMEOUT: u64 = 10;
    const MAX_PAGE_SIZE: u64 = 100;

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
        remote_root_snapshots: Map<(felt252, u256), RemoteRootSnapshot>,
        remote_root_count: Map<felt252, u256>,
        latest_remote_root_index: Map<felt252, u256>,
        // O(1) latest verified root lookup.
        // has_verified_root guards against a stale index read when no snapshot
        // has been verified yet for a given chain_id.
        // latest_verified_root_index always points to the highest verified index.
        latest_verified_root_index: Map<felt252, u256>,
        has_verified_root: Map<felt252, bool>,
        // --- Config ---
        batch_size: u64,
        batch_timeout: u64,
        whitelisted_tokens: Map<ContractAddress, bool>,
        // --- DEX integration ---
        /// AVNU exchange contract address. Set at construction; updatable by owner.
        /// Must be non-zero before any swap-enabled settle_and_release call is made.
        avnu_exchange: ContractAddress,
        // --- Access control ---
        authorized_relayers: Map<ContractAddress, bool>,
        root_verifiers: Map<ContractAddress, bool>,
        owner: ContractAddress,
        /// Pending owner for two-step ownership transfer.
        /// Zero when no transfer is in progress.
        pending_owner: ContractAddress,
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
        IntentSettledWithSwap: IntentSettledWithSwap,
        BatchConfigUpdated: BatchConfigUpdated,
        RelayerStatusChanged: RelayerStatusChanged,
        RootVerifierStatusChanged: RootVerifierStatusChanged,
        TokenWhitelistUpdated: TokenWhitelistUpdated,
        AvnuExchangeUpdated: AvnuExchangeUpdated,
        OwnershipProposed: OwnershipProposed,
        OwnershipProposalCancelled: OwnershipProposalCancelled,
        OwnershipTransferred: OwnershipTransferred,
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

    /// Emitted on direct settlement (no DEX swap).
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

    /// Emitted when settlement involved an AVNU DEX swap
    /// (STRK → any AVNU-routable StarkNet token: ETH, USDC, USDT, wBTC, etc.).
    #[derive(Drop, starknet::Event)]
    pub struct IntentSettledWithSwap {
        #[key]
        pub intent_id: felt252,
        #[key]
        pub nullifier_hash: felt252,
        pub delivered_token: ContractAddress,
        pub delivered_amount: u256,
        pub dest_token: ContractAddress,
        pub dest_amount: u256,
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
    pub struct AvnuExchangeUpdated {
        pub exchange: ContractAddress,
    }

    /// Emitted when the current owner nominates a new owner.
    #[derive(Drop, starknet::Event)]
    pub struct OwnershipProposed {
        #[key]
        pub current_owner: ContractAddress,
        pub proposed_owner: ContractAddress,
    }

    /// Emitted when the current owner cancels a pending proposal.
    #[derive(Drop, starknet::Event)]
    pub struct OwnershipProposalCancelled {
        #[key]
        pub cancelled_proposed_owner: ContractAddress,
    }

    /// Emitted when the pending owner accepts and the transfer completes.
    #[derive(Drop, starknet::Event)]
    pub struct OwnershipTransferred {
        #[key]
        pub previous_owner: ContractAddress,
        #[key]
        pub new_owner: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    pub struct Paused {}

    #[derive(Drop, starknet::Event)]
    pub struct Unpaused {}

    // ===== CONSTRUCTOR =====

    #[constructor]
    fn constructor(
        ref self: ContractState,
        owner: ContractAddress,
        initial_relayer: ContractAddress,
        avnu_exchange: ContractAddress,
    ) {
        self.owner.write(owner);
        self.pending_owner.write(Zero::zero());
        self.authorized_relayers.entry(initial_relayer).write(true);
        self.root_verifiers.entry(initial_relayer).write(true);
        self.batch_size.write(DEFAULT_BATCH_SIZE);
        self.batch_timeout.write(DEFAULT_TIMEOUT);
        self.paused.write(false);
        // avnu_exchange may be zero at construction time for testnets;
        // set_avnu_exchange must be called before swap-enabled settlements.
        self.avnu_exchange.write(avnu_exchange);

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
            assert!(nullifier_hash != 0, "Invalid nullifier hash");
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

            // Update O(1) verified root pointer.
            // Always tracks the highest verified index — snapshots can be verified
            // out of order so we only update if this one is newer.
            let has_any = self.has_verified_root.entry(chain_id).read();
            let current_latest = self.latest_verified_root_index.entry(chain_id).read();
            if !has_any || snapshot_index > current_latest {
                self.latest_verified_root_index.entry(chain_id).write(snapshot_index);
                self.has_verified_root.entry(chain_id).write(true);
            }

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
            dest_token: ContractAddress,
            expected_dest_amount: u256,
            min_dest_amount: u256,
            routes: Array<Route>,
        ) {
            self._assert_not_paused();
            self._only_relayer();

            // ── Guards ────────────────────────────────────────────────
            assert!(intent_id != 0, "Invalid intent id");
            assert!(nullifier_hash != 0, "Invalid nullifier hash");
            assert!(!self.used_nullifiers.entry(nullifier_hash).read(), "Nullifier already used");
            assert!(recipient.is_non_zero(), "Invalid recipient");
            assert!(self.whitelisted_tokens.entry(token).read(), "Token not whitelisted");
            assert!(amount > 0_u256, "Invalid amount");

            // Mark nullifier used immediately — prevents re-entrancy replay
            // even if the swap or transfer below were to call back into this contract.
            self.used_nullifiers.entry(nullifier_hash).write(true);

            let needs_swap = dest_token.is_non_zero() && dest_token != token;

            if needs_swap {
                // ── Swap path: STRK → dest token via AVNU ─────────────
                assert!(
                    self.whitelisted_tokens.entry(dest_token).read(), "Dest token not whitelisted",
                );
                assert!(min_dest_amount > 0_u256, "min_dest_amount required for swap");
                assert!(
                    expected_dest_amount >= min_dest_amount,
                    "expected_dest_amount < min_dest_amount",
                );
                assert!(!routes.is_empty(), "Routes required for swap");

                let dest_amount = self
                    ._perform_avnu_swap(
                        token,
                        amount,
                        dest_token,
                        expected_dest_amount,
                        min_dest_amount,
                        routes,
                    );

                let erc20 = IERC20Dispatcher { contract_address: dest_token };
                let success = erc20.transfer(recipient, dest_amount);
                assert!(success, "Dest token transfer failed");

                self
                    .emit(
                        IntentSettledWithSwap {
                            intent_id,
                            nullifier_hash,
                            delivered_token: token,
                            delivered_amount: amount,
                            dest_token,
                            dest_amount,
                            timestamp: get_block_timestamp(),
                        },
                    );
            } else {
                // ── Direct path: no swap needed ───────────────────────
                let erc20 = IERC20Dispatcher { contract_address: token };
                let success = erc20.transfer(recipient, amount);
                assert!(success, "Transfer failed");

                self
                    .emit(
                        IntentSettled {
                            intent_id,
                            nullifier_hash,
                            token,
                            amount,
                            timestamp: get_block_timestamp(),
                        },
                    );
            }
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

        fn get_avnu_exchange(self: @ContractState) -> ContractAddress {
            self.avnu_exchange.read()
        }

        fn get_latest_remote_root(self: @ContractState, chain_id: felt252) -> RemoteRootSnapshot {
            let count = self.remote_root_count.entry(chain_id).read();
            assert!(count > 0, "No snapshots found");
            let latest_index = self.latest_remote_root_index.entry(chain_id).read();
            self.remote_root_snapshots.entry((chain_id, latest_index)).read()
        }

        /// O(1) lookup — no backward loop.
        /// latest_verified_root_index is updated in verify_remote_root whenever
        /// a snapshot with a higher index is verified.
        fn get_latest_verified_remote_root(
            self: @ContractState, chain_id: felt252,
        ) -> (RemoteRootSnapshot, u256) {
            assert!(self.has_verified_root.entry(chain_id).read(), "No verified snapshot found");
            let idx = self.latest_verified_root_index.entry(chain_id).read();
            (self.remote_root_snapshots.entry((chain_id, idx)).read(), idx)
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

            if total == 0 || offset >= total {
                return (array![], total);
            }

            let remaining = total - offset;
            // Cap at MAX_PAGE_SIZE — prevents OOG on large view key sets.
            let effective_limit = if limit == 0 || limit > MAX_PAGE_SIZE {
                MAX_PAGE_SIZE
            } else {
                limit
            };
            let count = if effective_limit > remaining {
                remaining
            } else {
                effective_limit
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
            assert!(new_timeout >= MIN_TIMEOUT, "Timeout too short");

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

        /// Update the AVNU exchange contract address.
        /// Only callable by owner. Emits AvnuExchangeUpdated.
        fn set_avnu_exchange(ref self: ContractState, exchange: ContractAddress) {
            self._only_owner();
            assert!(exchange.is_non_zero(), "Invalid exchange address");
            self.avnu_exchange.write(exchange);
            self.emit(AvnuExchangeUpdated { exchange });
        }

        fn rescue_tokens(
            ref self: ContractState, token: ContractAddress, to: ContractAddress, amount: u256,
        ) {
            self._only_owner();
            assert!(to.is_non_zero(), "Invalid recipient");
            assert!(amount > 0_u256, "Invalid amount");
            let erc20 = IERC20Dispatcher { contract_address: token };
            let success = erc20.transfer(to, amount);
            assert!(success, "Rescue transfer failed");
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

        // ==============================================================
        //                      OWNERSHIP FUNCTIONS
        //
        // Two-step pattern: current owner nominates, pending owner accepts.
        // Prevents permanent lockout from a typo in the new owner address.
        // Current owner can cancel a pending proposal at any time.
        // ==============================================================

        /// Step 1 — Current owner nominates a new owner.
        /// The proposed owner must call accept_ownership to complete the transfer.
        /// Calling again before accept_ownership overwrites the pending proposal.
        fn propose_owner(ref self: ContractState, new_owner: ContractAddress) {
            self._only_owner();
            assert!(new_owner.is_non_zero(), "Invalid proposed owner");
            assert!(new_owner != self.owner.read(), "Already owner");
            self.pending_owner.write(new_owner);
            self
                .emit(
                    OwnershipProposed {
                        current_owner: get_caller_address(), proposed_owner: new_owner,
                    },
                );
        }

        /// Step 2 — Pending owner accepts and becomes the new owner.
        /// Clears the pending owner slot on completion.
        fn accept_ownership(ref self: ContractState) {
            let caller = get_caller_address();
            let pending = self.pending_owner.read();
            assert!(pending.is_non_zero(), "No ownership proposal");
            assert!(caller == pending, "Not pending owner");

            let previous = self.owner.read();
            self.owner.write(caller);
            self.pending_owner.write(Zero::zero());

            self.emit(OwnershipTransferred { previous_owner: previous, new_owner: caller });
        }

        /// Cancel a pending ownership proposal. Only callable by current owner.
        fn cancel_ownership_proposal(ref self: ContractState) {
            self._only_owner();
            let pending = self.pending_owner.read();
            assert!(pending.is_non_zero(), "No ownership proposal");
            self.pending_owner.write(Zero::zero());
            self.emit(OwnershipProposalCancelled { cancelled_proposed_owner: pending });
        }

        fn get_pending_owner(self: @ContractState) -> ContractAddress {
            self.pending_owner.read()
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

        /// Swap `sell_token` → `buy_token` via AVNU and return the received amount.
        ///
        /// Security properties (in order of execution):
        /// 1. AVNU exchange address must be non-zero (set by owner).
        /// 2. Approval reset to 0 BEFORE setting new allowance (USDT-safe pattern).
        /// 3. Approval set to exactly sell_amount — principle of least privilege.
        /// 4. Both pre/post approval operations are asserted — not silently dropped.
        /// 5. expected_buy_amount passed as AVNU token_to_amount (route scoring reference).
        /// 6. min_buy_amount passed as AVNU token_to_min_amount (hard slippage floor).
        /// 7. Beneficiary = this contract — we hold and verify before forwarding.
        /// 8. Received amount verified by balance delta — does not trust AVNU return value.
        /// 9. Received amount must be >= min_buy_amount (second independent slippage guard).
        fn _perform_avnu_swap(
            ref self: ContractState,
            sell_token: ContractAddress,
            sell_amount: u256,
            buy_token: ContractAddress,
            expected_buy_amount: u256,
            min_buy_amount: u256,
            routes: Array<Route>,
        ) -> u256 {
            let exchange_address = self.avnu_exchange.read();
            assert!(exchange_address.is_non_zero(), "AVNU exchange not configured");

            let this = get_contract_address();

            // Snapshot buy_token balance before swap — compute received amount
            // via delta rather than trusting AVNU's return value.
            let buy_erc20 = IERC20Dispatcher { contract_address: buy_token };
            let balance_before = buy_erc20.balance_of(this);

            let sell_erc20 = IERC20Dispatcher { contract_address: sell_token };

            // Reset approval to 0 first — required for USDT-style tokens that
            // revert if a non-zero allowance is overwritten directly.
            let pre_reset = sell_erc20.approve(exchange_address, 0_u256);
            assert!(pre_reset, "Pre-swap approval reset failed");

            // Approve AVNU for exactly sell_amount — principle of least privilege.
            let approved = sell_erc20.approve(exchange_address, sell_amount);
            assert!(approved, "AVNU approval failed");

            // Call AVNU.
            //   token_to_amount     = expected_buy_amount (quote reference for route scoring)
            //   token_to_min_amount = min_buy_amount      (hard slippage floor enforced by AVNU)
            //   beneficiary         = this contract       (we verify balance delta before forwarding)
            //   integrator_fee_bps  = 0                   (no fee taken by ShadowSwap)
            let exchange = IAvnuExchangeDispatcher { contract_address: exchange_address };
            let swapped = exchange
                .multi_route_swap(
                    sell_token,
                    sell_amount,
                    buy_token,
                    expected_buy_amount,
                    min_buy_amount,
                    this,
                    0_u128,
                    this,
                    routes,
                );
            assert!(swapped, "AVNU swap failed");

            // Reset approval to zero — assert return value, never silently drop.
            let post_reset = sell_erc20.approve(exchange_address, 0_u256);
            assert!(post_reset, "Post-swap approval reset failed");

            // Compute and verify received amount via balance delta.
            let balance_after = buy_erc20.balance_of(this);
            assert!(balance_after > balance_before, "No tokens received from swap");
            let received = balance_after - balance_before;
            assert!(received >= min_buy_amount, "Slippage exceeded");

            received
        }
    }
}
