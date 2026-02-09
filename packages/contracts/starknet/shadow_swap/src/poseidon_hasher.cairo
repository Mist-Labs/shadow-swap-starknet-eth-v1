// Poseidon Hasher for StarkNet - Compatible with EVM PoseidonHasher
//
// IMPORTANT: StarkNet uses native Poseidon over the STARK field
// (p = 2^251 + 17*2^192 + 1), NOT BN254.
//
// Cross-chain compatibility is maintained because:
// 1. Commitments are generated CLIENT-SIDE using the EVM BN254 Poseidon
// 2. EVM stores commitments in Merkle tree (keccak256 for tree nodes)
// 3. Merkle root is synced to StarkNet
// 4. StarkNet verifies STARK proofs of commitment inclusion
//
// This hasher is used for StarkNet-side operations:
// - Nullifier hash verification: nullifier_hash = poseidon(nullifier)
// - View key derivation
// - Any on-chain hashing needed during settlement

#[starknet::interface]
pub trait IPoseidonHasher<TContractState> {
    /// Hash 2 elements using Poseidon
    fn hash2(self: @TContractState, a: felt252, b: felt252) -> felt252;

    /// Hash 3 elements using Poseidon
    fn hash3(self: @TContractState, a: felt252, b: felt252, c: felt252) -> felt252;

    /// Hash 4 elements (commitment = poseidon(secret, nullifier, amount, dest_chain))
    fn hash4(self: @TContractState, a: felt252, b: felt252, c: felt252, d: felt252) -> felt252;

    /// Hash arbitrary-length input
    fn hash_many(self: @TContractState, inputs: Span<felt252>) -> felt252;

    /// Hash pair with deterministic ordering (mirrors EVM _hashPair)
    fn hash_pair_sorted(self: @TContractState, a: felt252, b: felt252) -> felt252;

    /// Compute commitment from components
    fn compute_commitment(
        self: @TContractState,
        secret: felt252,
        nullifier: felt252,
        amount: felt252,
        dest_chain: felt252,
    ) -> felt252;

    /// Compute nullifier hash
    fn compute_nullifier_hash(self: @TContractState, nullifier: felt252) -> felt252;
}

#[starknet::contract]
pub mod PoseidonHasher {
    use core::hash::{HashStateExTrait, HashStateTrait};
    use core::poseidon::PoseidonTrait;

    #[storage]
    struct Storage {}

    #[abi(embed_v0)]
    impl PoseidonHasherImpl of super::IPoseidonHasher<ContractState> {
        fn hash2(self: @ContractState, a: felt252, b: felt252) -> felt252 {
            let mut state = PoseidonTrait::new();
            state = state.update(a);
            state = state.update(b);
            state.finalize()
        }

        fn hash3(self: @ContractState, a: felt252, b: felt252, c: felt252) -> felt252 {
            let mut state = PoseidonTrait::new();
            state = state.update(a);
            state = state.update(b);
            state = state.update(c);
            state.finalize()
        }

        fn hash4(self: @ContractState, a: felt252, b: felt252, c: felt252, d: felt252) -> felt252 {
            let mut state = PoseidonTrait::new();
            state = state.update(a);
            state = state.update(b);
            state = state.update(c);
            state = state.update(d);
            state.finalize()
        }

        fn hash_many(self: @ContractState, inputs: Span<felt252>) -> felt252 {
            let mut state = PoseidonTrait::new();
            let mut i: u32 = 0;
            while i < inputs.len() {
                state = state.update(*inputs.at(i));
                i += 1;
            }
            state.finalize()
        }

        /// Deterministic ordering — mirrors EVM ShadowIntentPool._hashPair
        fn hash_pair_sorted(self: @ContractState, a: felt252, b: felt252) -> felt252 {
            let a_u256: u256 = a.into();
            let b_u256: u256 = b.into();
            let (first, second) = if a_u256 < b_u256 {
                (a, b)
            } else {
                (b, a)
            };
            let mut state = PoseidonTrait::new();
            state = state.update(first);
            state = state.update(second);
            state.finalize()
        }

        /// commitment = poseidon(secret, nullifier, amount, dest_chain)
        fn compute_commitment(
            self: @ContractState,
            secret: felt252,
            nullifier: felt252,
            amount: felt252,
            dest_chain: felt252,
        ) -> felt252 {
            let mut state = PoseidonTrait::new();
            state = state.update(secret);
            state = state.update(nullifier);
            state = state.update(amount);
            state = state.update(dest_chain);
            state.finalize()
        }

        /// nullifier_hash = poseidon(nullifier)
        fn compute_nullifier_hash(self: @ContractState, nullifier: felt252) -> felt252 {
            let mut state = PoseidonTrait::new();
            state = state.update(nullifier);
            state.finalize()
        }
    }
}
