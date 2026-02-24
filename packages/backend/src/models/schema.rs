// @generated automatically by Diesel CLI.

diesel::table! {
    commitments (commitment_id) {
        commitment_id -> Int4,
        #[max_length = 16]
        chain -> Varchar,
        #[max_length = 66]
        commitment_hash -> Varchar,
        #[max_length = 66]
        intent_id -> Nullable<Varchar>,
        block_number -> Nullable<Int8>,
        log_index -> Nullable<Int4>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    merkle_leaves (leaf_id) {
        leaf_id -> Int4,
        #[max_length = 64]
        tree_name -> Varchar,
        #[max_length = 66]
        leaf -> Varchar,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    merkle_roots (root_id) {
        root_id -> Int4,
        #[max_length = 64]
        tree_name -> Varchar,
        #[max_length = 66]
        root -> Varchar,
        leaf_count -> Int8,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    shadow_intents (id) {
        #[max_length = 66]
        id -> Varchar,
        #[max_length = 66]
        commitment -> Varchar,
        #[max_length = 66]
        nullifier_hash -> Varchar,
        #[max_length = 66]
        view_key -> Varchar,
        #[max_length = 66]
        near_intents_id -> Varchar,
        #[max_length = 32]
        source_chain -> Varchar,
        #[max_length = 32]
        dest_chain -> Varchar,
        #[max_length = 66]
        encrypted_recipient -> Varchar,
        #[max_length = 66]
        token -> Varchar,
        #[max_length = 78]
        amount -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        #[max_length = 66]
        deposit_address -> Nullable<Varchar>,
        #[max_length = 66]
        near_correlation_id -> Nullable<Varchar>,
        #[max_length = 32]
        near_status -> Nullable<Varchar>,
        #[max_length = 66]
        dest_tx_hash -> Nullable<Varchar>,
        #[max_length = 66]
        settle_tx_hash -> Nullable<Varchar>,
        #[max_length = 66]
        source_settle_tx_hash -> Nullable<Varchar>,
        #[max_length = 512]
        encrypted_secret -> Nullable<Varchar>,
        #[max_length = 512]
        encrypted_nullifier -> Nullable<Varchar>,
        created_at -> Int8,
        updated_at -> Int8,
    }
}

diesel::table! {
    transaction_logs (log_id) {
        log_id -> Int4,
        #[max_length = 66]
        intent_id -> Varchar,
        #[max_length = 32]
        chain -> Varchar,
        #[max_length = 64]
        tx_type -> Varchar,
        #[max_length = 66]
        tx_hash -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        created_at -> Timestamptz,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    commitments,
    merkle_leaves,
    merkle_roots,
    shadow_intents,
    transaction_logs,
);
