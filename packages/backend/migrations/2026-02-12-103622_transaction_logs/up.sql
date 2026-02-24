-- Your SQL goes here
CREATE TABLE transaction_logs (
    log_id     SERIAL PRIMARY KEY,
    intent_id  VARCHAR(66) NOT NULL,
    chain      VARCHAR(32) NOT NULL,
    tx_type    VARCHAR(64) NOT NULL,
    tx_hash    VARCHAR(66) NOT NULL,
    status     VARCHAR(32) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tx_logs_intent ON transaction_logs(intent_id);
CREATE INDEX idx_tx_logs_chain ON transaction_logs(chain);