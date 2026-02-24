-- Rollback: rename encrypted_recipient back to recipient
ALTER TABLE shadow_intents RENAME COLUMN encrypted_recipient TO recipient;
