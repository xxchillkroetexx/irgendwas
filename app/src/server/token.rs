//! Single-use link tokens (spec: auth, "Tokens"): 32 bytes from the OS CSPRNG, base64url
//! without padding (43 characters) in URLs; only the lowercase hex SHA-256 of the token
//! string is stored. Tokens never appear in logs. Shared with the account slice (email
//! verification).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// A fresh random token for a mail link.
pub fn generate() -> Result<String, AppError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::internal(format!("OS RNG failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// What the database stores for `token`: 64 lowercase hex characters.
pub fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";
