-- Your SQL goes here
CREATE TABLE merkle_roots (
    root_id    SERIAL PRIMARY KEY,
    tree_name  VARCHAR(64) NOT NULL,
    root       VARCHAR(66) NOT NULL,
    leaf_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_merkle_roots_tree ON merkle_roots(tree_name);