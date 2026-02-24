-- Rename recipient to encrypted_recipient to enforce ECIES encryption
ALTER TABLE shadow_intents RENAME COLUMN recipient TO encrypted_recipient;
