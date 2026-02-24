-- Your SQL goes here
CREATE TABLE commitments (
    commitment_id   SERIAL PRIMARY KEY,
    chain           VARCHAR(16) NOT NULL,
    commitment_hash VARCHAR(66) NOT NULL UNIQUE,
    intent_id       VARCHAR(66),
    block_number    BIGINT,
    log_index       INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_commitments_chain ON commitments(chain);
