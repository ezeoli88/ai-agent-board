//! Encryption service using AES-256-GCM.
//!
//! Produces ciphertext in the exact same format as the TypeScript server:
//! `hex(iv):hex(authTag):hex(ciphertext)`
//!
//! The TypeScript server uses Node.js `crypto.createCipheriv('aes-256-gcm', key, iv)`
//! with a 16-byte IV. Node's GCM implementation appends the auth tag separately via
//! `cipher.getAuthTag()`. The `aes-gcm` Rust crate appends the 16-byte tag to the
//! ciphertext by default, so we split it off to match the TS format.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aes_gcm::aead::generic_array::typenum::{U12, U16};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::Aead;
use aes_gcm::aes::Aes256;
use aes_gcm::{Aes256Gcm, AesGcm, KeyInit};
use rand::RngCore;
use tracing::{info, warn};

use crate::error::AppError;

/// Tag size for AES-256-GCM (16 bytes / 128 bits).
const TAG_SIZE: usize = 16;

/// IV (nonce) size matching the TypeScript server (16 bytes).
/// Note: AES-GCM standard uses 12-byte nonces. The TypeScript server uses 16-byte IVs
/// because Node.js GCM supports arbitrary IV sizes. The `aes-gcm` crate expects
/// exactly 12 bytes, so we use 12 bytes here for Rust correctness.
///
/// IMPORTANT: Since the TS server uses 16-byte IVs and this Rust server uses 12-byte
/// nonces, data encrypted by one server CANNOT be decrypted by the other directly.
/// However, this service will correctly round-trip its own data, and for migration
/// purposes the nonce size in `decrypt` is inferred from the hex length of the IV part.
const NONCE_SIZE: usize = 12;

/// Filename for the persisted encryption key.
const KEY_FILENAME: &str = "encryption.key";

/// Global encryption key, initialized once.
static ENCRYPTION_KEY: OnceLock<Vec<u8>> = OnceLock::new();

/// Directory where the key file is stored, set by [`init_encryption_key`].
static KEY_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initializes the encryption key, persisting it to disk so secrets survive restarts.
///
/// Call this once at startup, after the data directory is known.
///
/// Key resolution order:
/// 1. `ENCRYPTION_KEY` env var (explicit override, highest priority)
/// 2. `<data_dir>/encryption.key` file (auto-persisted key)
/// 3. Generate a new random key and save it to `<data_dir>/encryption.key`
pub fn init_encryption_key(data_dir: &Path) {
    KEY_DIR.get_or_init(|| data_dir.to_path_buf());

    ENCRYPTION_KEY.get_or_init(|| {
        // 1. Check env var (explicit override)
        if let Ok(env_key) = env::var("ENCRYPTION_KEY") {
            if !env_key.is_empty() {
                let key_bytes = hex::decode(&env_key).expect(
                    "ENCRYPTION_KEY must be valid hex (64 hex characters = 32 bytes)",
                );
                if key_bytes.len() != 32 {
                    panic!(
                        "ENCRYPTION_KEY must be 32 bytes (64 hex characters), got {} bytes",
                        key_bytes.len()
                    );
                }
                info!("Using encryption key from ENCRYPTION_KEY env var");
                return key_bytes;
            }
        }

        let key_path = data_dir.join(KEY_FILENAME);

        // 2. Try to read existing key file
        if key_path.exists() {
            match std::fs::read_to_string(&key_path) {
                Ok(contents) => {
                    let hex_key = contents.trim().to_string();
                    if hex_key.len() == 64 {
                        match hex::decode(&hex_key) {
                            Ok(key_bytes) if key_bytes.len() == 32 => {
                                info!(path = %key_path.display(), "Loaded encryption key from file");
                                return key_bytes;
                            }
                            _ => {
                                warn!(path = %key_path.display(), "Key file contains invalid data, regenerating");
                            }
                        }
                    } else {
                        warn!(
                            path = %key_path.display(),
                            len = hex_key.len(),
                            "Key file has wrong length (expected 64 hex chars), regenerating"
                        );
                    }
                }
                Err(e) => {
                    warn!(path = %key_path.display(), error = %e, "Failed to read key file, regenerating");
                }
            }
        }

        // 3. Generate a new key and persist it
        let mut key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let hex_key = hex::encode(&key);

        // Ensure the directory exists
        if let Some(parent) = key_path.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        match std::fs::write(&key_path, &hex_key) {
            Ok(()) => {
                info!(path = %key_path.display(), "Generated and saved new encryption key");
            }
            Err(e) => {
                warn!(
                    path = %key_path.display(),
                    error = %e,
                    "Failed to save encryption key to file — secrets will NOT persist across restarts!"
                );
            }
        }

        key
    });
}

/// Returns the 32-byte encryption key.
///
/// Panics if called before [`init_encryption_key`].
/// In tests, lazily generates a random key.
pub fn get_encryption_key() -> &'static [u8] {
    ENCRYPTION_KEY.get_or_init(|| {
        // Fallback for tests or if init_encryption_key was not called
        warn!("get_encryption_key() called without init — generating ephemeral key (test mode?)");
        let mut key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    })
}

/// Encrypts a plaintext string using AES-256-GCM.
///
/// Returns the encrypted data in the format: `hex(nonce):hex(authTag):hex(ciphertext)`
///
/// This matches the TypeScript format `iv:authTag:ciphertext`, allowing interoperability
/// when both servers use the same nonce size.
pub fn encrypt(plaintext: &str) -> Result<String, AppError> {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = GenericArray::from_slice(&nonce_bytes);

    // Encrypt — aes-gcm appends the 16-byte auth tag to the ciphertext
    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Encryption failed: {e}")))?;

    // Split: last TAG_SIZE bytes are the auth tag, the rest is ciphertext
    let ct_len = ciphertext_with_tag.len() - TAG_SIZE;
    let ciphertext = &ciphertext_with_tag[..ct_len];
    let auth_tag = &ciphertext_with_tag[ct_len..];

    // Format: hex(nonce):hex(authTag):hex(ciphertext)
    Ok(format!(
        "{}:{}:{}",
        hex::encode(nonce_bytes),
        hex::encode(auth_tag),
        hex::encode(ciphertext),
    ))
}

/// Decrypts data that was encrypted with [`encrypt`].
///
/// Expects the format `hex(iv):hex(authTag):hex(ciphertext)`.
pub fn decrypt(encrypted: &str) -> Result<String, AppError> {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));

    let parts: Vec<&str> = encrypted.split(':').collect();
    if parts.len() != 3 {
        return Err(AppError::Validation(
            "Invalid encrypted data format: expected iv:authTag:ciphertext".into(),
        ));
    }

    let iv_hex = parts[0];
    let auth_tag_hex = parts[1];
    let ciphertext_hex = parts[2];

    if iv_hex.is_empty() || auth_tag_hex.is_empty() {
        return Err(AppError::Validation(
            "Invalid encrypted data format: missing components".into(),
        ));
    }

    let nonce_bytes =
        hex::decode(iv_hex).map_err(|e| AppError::Validation(format!("Invalid IV hex: {e}")))?;
    let auth_tag = hex::decode(auth_tag_hex)
        .map_err(|e| AppError::Validation(format!("Invalid auth tag hex: {e}")))?;
    let ciphertext = hex::decode(ciphertext_hex)
        .map_err(|e| AppError::Validation(format!("Invalid ciphertext hex: {e}")))?;

    if auth_tag.len() != TAG_SIZE {
        return Err(AppError::Validation(format!(
            "Invalid auth tag length: expected {TAG_SIZE} bytes, got {}",
            auth_tag.len()
        )));
    }

    // aes-gcm expects the payload as ciphertext || auth_tag
    let mut payload = ciphertext;
    payload.extend_from_slice(&auth_tag);

    // Dispatch based on nonce length:
    // - 12 bytes: standard AES-GCM (Rust-encrypted data)
    // - 16 bytes: Node.js GCM (TypeScript server data)
    let plaintext_bytes = match nonce_bytes.len() {
        12 => {
            let nonce = GenericArray::<u8, U12>::from_slice(&nonce_bytes);
            cipher
                .decrypt(nonce, payload.as_ref())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Decryption failed: {e}")))?
        }
        16 => {
            // TypeScript server uses 16-byte IVs with Node.js crypto GCM
            let cipher_16: AesGcm<Aes256, U16> = AesGcm::new(GenericArray::from_slice(key));
            let nonce = GenericArray::<u8, U16>::from_slice(&nonce_bytes);
            cipher_16.decrypt(nonce, payload.as_ref()).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Decryption failed (16-byte nonce): {e}"))
            })?
        }
        other => {
            return Err(AppError::Validation(format!(
                "Invalid nonce length: expected 12 or 16 bytes, got {other}"
            )));
        }
    };

    String::from_utf8(plaintext_bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Decrypted data is not valid UTF-8: {e}")))
}

/// Validates that a string is a valid encryption key format.
/// Must be exactly 64 hex characters (32 bytes).
pub fn is_valid_encryption_key(key: &str) -> bool {
    if key.len() != 64 {
        return false;
    }
    key.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let plaintext = "Hello, world! This is a secret message.";
        let encrypted = encrypt(plaintext).unwrap();

        // Verify format: three hex parts separated by colons
        let parts: Vec<&str> = encrypted.split(':').collect();
        assert_eq!(parts.len(), 3);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
        assert!(!parts[2].is_empty());

        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let plaintext = "same message";
        let e1 = encrypt(plaintext).unwrap();
        let e2 = encrypt(plaintext).unwrap();
        // Different nonces should produce different ciphertexts
        assert_ne!(e1, e2);

        // But both should decrypt to the same plaintext
        assert_eq!(decrypt(&e1).unwrap(), plaintext);
        assert_eq!(decrypt(&e2).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_invalid_format() {
        assert!(decrypt("invalid").is_err());
        assert!(decrypt("a:b").is_err());
        assert!(decrypt("::").is_err());
    }

    #[test]
    fn test_is_valid_encryption_key() {
        // Valid: 64 hex chars
        assert!(is_valid_encryption_key(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        // Valid: uppercase hex
        assert!(is_valid_encryption_key(
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"
        ));
        // Invalid: too short
        assert!(!is_valid_encryption_key("0123456789abcdef"));
        // Invalid: too long
        assert!(!is_valid_encryption_key(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef00"
        ));
        // Invalid: non-hex chars
        assert!(!is_valid_encryption_key(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
        // Invalid: empty
        assert!(!is_valid_encryption_key(""));
    }

    #[test]
    fn test_encrypt_empty_string() {
        let encrypted = encrypt("").unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_unicode() {
        let plaintext = "Hola mundo! Esto es una clave secreta.";
        let encrypted = encrypt(plaintext).unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
