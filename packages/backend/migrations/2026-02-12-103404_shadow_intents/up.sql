-- Your SQL goes here
CREATE TABLE shadow_intents (
    id                    VARCHAR(66)  PRIMARY KEY,
    commitment            VARCHAR(66)  NOT NULL,
    nullifier_hash        VARCHAR(66)  NOT NULL,
    view_key              VARCHAR(66)  NOT NULL,
    near_intents_id       VARCHAR(66)  NOT NULL,
    source_chain          VARCHAR(32)  NOT NULL,
    dest_chain            VARCHAR(32)  NOT NULL,
    encrypted_recipient   VARCHAR(512) NOT NULL,
    token                 VARCHAR(66)  NOT NULL,
    amount                VARCHAR(78)  NOT NULL,
    status                VARCHAR(32)  NOT NULL DEFAULT 'pending',
    deposit_address       VARCHAR(66),
    near_correlation_id   VARCHAR(66),
    near_status           VARCHAR(32),
    dest_tx_hash          VARCHAR(66),
    settle_tx_hash        VARCHAR(66),
    source_settle_tx_hash VARCHAR(66),
    created_at            BIGINT       NOT NULL,
    updated_at            BIGINT       NOT NULL,
    encrypted_secret      VARCHAR(512),
    encrypted_nullifier   VARCHAR(512),
    dest_token            TEXT
);

CREATE INDEX idx_shadow_intents_status        ON shadow_intents(status);
CREATE INDEX idx_shadow_intents_commitment    ON shadow_intents(commitment);
CREATE INDEX idx_shadow_intents_deposit_address ON shadow_intents(deposit_address);