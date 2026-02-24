-- Your SQL goes here
CREATE TABLE merkle_leaves (
    leaf_id    SERIAL PRIMARY KEY,
    tree_name  VARCHAR(64) NOT NULL,
    leaf       VARCHAR(66) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_merkle_leaves_tree ON merkle_leaves(tree_name);