//! Port of `src/config-crypto.ts`. Byte-compatible with the Node implementation:
//! AES-256-GCM, scrypt key derivation (Node defaults: N=2^14, r=8, p=1, len=32),
//! payload layout `salt[16] | iv[12] | tag[16] | ciphertext`, base64 after the
//! `FSENC1:` prefix.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use scrypt::{scrypt, Params};

const PREFIX: &str = "FSENC1:";

/// Accept a 64-char hex string (32 bytes) or base64 of exactly 32 bytes.
fn parse_master_key(raw: &str) -> Result<[u8; 32]> {
    let t = raw.trim();
    let is_hex64 = t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit());
    if is_hex64 {
        let bytes = hex::decode(t)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    if let Ok(b) = B64.decode(t) {
        if b.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&b);
            return Ok(key);
        }
    }
    bail!("FILESANDBOX_MASTER_KEY must be 64 hex chars (32 bytes) or base64 of 32 bytes")
}

/// scrypt with Node's default cost parameters.
fn derive_key(master: &[u8; 32], salt: &[u8]) -> Result<[u8; 32]> {
    // Node's scryptSync defaults: N=16384 (log2=14), r=8, p=1.
    let params = Params::new(14, 8, 1, 32).map_err(|e| anyhow!("scrypt params: {e}"))?;
    let mut out = [0u8; 32];
    scrypt(master, salt, &params, &mut out).map_err(|e| anyhow!("scrypt: {e}"))?;
    Ok(out)
}

/// Encrypt plaintext; returns `FSENC1:` + base64(salt | iv | tag | ciphertext).
pub fn encrypt_config_json(plaintext: &str, master_key_raw: &str) -> Result<String> {
    let master = parse_master_key(master_key_raw)?;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key_bytes = derive_key(&master, &salt)?;
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    // aes-gcm returns ciphertext with the 16-byte tag appended.
    let ct_and_tag = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext.as_bytes())
        .map_err(|e| anyhow!("aes-gcm encrypt: {e}"))?;
    let split = ct_and_tag.len() - 16;
    let (ciphertext, tag) = ct_and_tag.split_at(split);

    // Node layout puts the tag before the ciphertext.
    let mut payload = Vec::with_capacity(16 + 12 + 16 + ciphertext.len());
    payload.extend_from_slice(&salt);
    payload.extend_from_slice(&iv);
    payload.extend_from_slice(tag);
    payload.extend_from_slice(ciphertext);
    Ok(format!("{PREFIX}{}", B64.encode(payload)))
}

/// Decrypt a blob written by [`encrypt_config_json`] (or the Node version).
pub fn decrypt_config_json(blob: &str, master_key_raw: &str) -> Result<String> {
    let blob = blob.trim();
    let b64 = blob
        .strip_prefix(PREFIX)
        .ok_or_else(|| anyhow!("Encrypted config must start with {PREFIX}"))?;
    let master = parse_master_key(master_key_raw)?;
    let raw = B64.decode(b64).map_err(|e| anyhow!("base64: {e}"))?;
    if raw.len() < 16 + 12 + 16 + 1 {
        bail!("Encrypted config truncated");
    }
    let salt = &raw[0..16];
    let iv = &raw[16..28];
    let tag = &raw[28..44];
    let data = &raw[44..];
    let key_bytes = derive_key(&master, salt)?;

    // aes-gcm wants ciphertext || tag.
    let mut ct_and_tag = Vec::with_capacity(data.len() + 16);
    ct_and_tag.extend_from_slice(data);
    ct_and_tag.extend_from_slice(tag);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let plain = cipher
        .decrypt(Nonce::from_slice(iv), ct_and_tag.as_ref())
        .map_err(|_| anyhow!("decrypt failed (bad key or tampered payload)"))?;
    Ok(String::from_utf8(plain)?)
}

pub fn is_encrypted_config_payload(s: &str) -> bool {
    s.trim_start().starts_with(PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn round_trip() {
        let pt = r#"{"vtApiKey":"abc","httpPort":3847}"#;
        let blob = encrypt_config_json(pt, ZERO_KEY).unwrap();
        assert!(is_encrypted_config_payload(&blob));
        assert_eq!(decrypt_config_json(&blob, ZERO_KEY).unwrap(), pt);
    }

    /// Parity: decrypt a ciphertext produced by the Node `config-crypto.ts`
    /// (master key = 32 zero bytes, plaintext `{"hello":"world"}`).
    #[test]
    fn decrypts_node_fixture() {
        let blob = "FSENC1:qtATRN5PPfDVoMLyn09OVrMOhqX9X1bdEc+DDlIi+UlDbCsYaposxnNoW5ykmMamMeXJR4JHFrnGGKeEOQ==";
        assert_eq!(
            decrypt_config_json(blob, ZERO_KEY).unwrap(),
            r#"{"hello":"world"}"#
        );
    }

    #[test]
    fn rejects_bad_prefix() {
        assert!(decrypt_config_json("nope", ZERO_KEY).is_err());
        assert!(!is_encrypted_config_payload("{}"));
    }

    #[test]
    fn rejects_wrong_key() {
        let blob = encrypt_config_json("secret", ZERO_KEY).unwrap();
        let other = "1111111111111111111111111111111111111111111111111111111111111111";
        assert!(decrypt_config_json(&blob, other).is_err());
    }
}
