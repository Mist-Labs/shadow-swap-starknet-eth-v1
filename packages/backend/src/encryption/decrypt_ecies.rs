use anyhow::{anyhow, Result};
use secp256k1::SecretKey;

/// Decrypt ECIES-encrypted data using relayer's private key
///
/// Used to decrypt:
/// - Nullifier (for settlement)
/// - Secret (for proof generation, if needed)
/// - Recipient (destination address, decrypted only at settlement time)
///
/// # Arguments
/// * `encrypted_hex` - Hex-encoded encrypted data (with or without 0x prefix)
/// * `private_key_hex` - Relayer's private key in hex (with or without 0x prefix)
///
/// # Returns
/// Decrypted data as hex string with 0x prefix
pub fn decrypt_with_ecies(encrypted_hex: &str, private_key_hex: &str) -> Result<String> {
    let encrypted_hex = encrypted_hex.strip_prefix("0x").unwrap_or(encrypted_hex);
    let private_key_hex = private_key_hex
        .strip_prefix("0x")
        .unwrap_or(private_key_hex);

    let encrypted =
        hex::decode(encrypted_hex).map_err(|e| anyhow!("Invalid encrypted data hex: {}", e))?;

    let private_key_bytes =
        hex::decode(private_key_hex).map_err(|e| anyhow!("Invalid private key hex: {}", e))?;

    let secret_key = SecretKey::from_slice(&private_key_bytes)
        .map_err(|e| anyhow!("Invalid private key format: {}", e))?;

    let decrypted_bytes = ecies::decrypt(&secret_key.secret_bytes(), &encrypted)
        .map_err(|e| anyhow!("ECIES decryption failed: {}", e))?;

    let hex_string = hex::encode(&decrypted_bytes);
    Ok(format!("0x{}", hex_string))
}

/// Decrypt ECIES-encrypted data and return the plaintext as a UTF-8 string.
///
/// Use this when the original plaintext was a UTF-8 string (e.g., an Ethereum
/// address like "0x2af4..."). The counterpart `decrypt_with_ecies` hex-encodes the
/// raw bytes and is correct for 32-byte secrets/nullifiers, but not for string values.
pub fn decrypt_with_ecies_utf8(encrypted_hex: &str, private_key_hex: &str) -> Result<String> {
    let encrypted_hex = encrypted_hex.strip_prefix("0x").unwrap_or(encrypted_hex);
    let private_key_hex = private_key_hex
        .strip_prefix("0x")
        .unwrap_or(private_key_hex);

    let encrypted =
        hex::decode(encrypted_hex).map_err(|e| anyhow!("Invalid encrypted data hex: {}", e))?;

    let private_key_bytes =
        hex::decode(private_key_hex).map_err(|e| anyhow!("Invalid private key hex: {}", e))?;

    let secret_key = SecretKey::from_slice(&private_key_bytes)
        .map_err(|e| anyhow!("Invalid private key format: {}", e))?;

    let decrypted_bytes = ecies::decrypt(&secret_key.secret_bytes(), &encrypted)
        .map_err(|e| anyhow!("ECIES decryption failed: {}", e))?;

    String::from_utf8(decrypted_bytes)
        .map_err(|e| anyhow!("Decrypted bytes are not valid UTF-8: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{PublicKey, Secp256k1};

    #[test]
    fn test_ecies_round_trip() {
        // Generate test keypair
        let private_key_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let private_key_bytes = hex::decode(private_key_hex).unwrap();
        let secret_key = SecretKey::from_slice(&private_key_bytes).unwrap();

        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_bytes = public_key.serialize();

        // Test data - this is the HEX STRING we want to encrypt
        let original = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

        // Convert hex string to bytes for encryption (this is what actually gets encrypted)
        let original_bytes = hex::decode(original).unwrap();

        // Encrypt the BYTES, not the ASCII string representation
        let encrypted = ecies::encrypt(&public_key_bytes, &original_bytes).unwrap();
        let encrypted_hex = hex::encode(&encrypted);

        // Decrypt
        let decrypted = decrypt_with_ecies(&encrypted_hex, private_key_hex).unwrap();

        assert_eq!(format!("0x{}", original), decrypted);
        println!("✅ ECIES decryption test passed");
    }

    #[test]
    fn test_decrypt_with_0x_prefix() {
        let private_key_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let private_key_bytes = hex::decode(private_key_hex).unwrap();
        let secret_key = SecretKey::from_slice(&private_key_bytes).unwrap();

        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_bytes = public_key.serialize();

        let original = "test message";
        let encrypted = ecies::encrypt(&public_key_bytes, original.as_bytes()).unwrap();
        let encrypted_hex = format!("0x{}", hex::encode(&encrypted));

        // Test with 0x prefix
        let decrypted =
            decrypt_with_ecies(&encrypted_hex, &format!("0x{}", private_key_hex)).unwrap();

        assert!(decrypted.starts_with("0x"));
    }

    #[test]
    fn test_decrypt_invalid_hex() {
        let result = decrypt_with_ecies("invalid_hex", "validkey123");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_invalid_private_key() {
        let encrypted = "0x1234567890abcdef";
        let result = decrypt_with_ecies(encrypted, "invalid_key");
        assert!(result.is_err());
    }
}
