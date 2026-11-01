//! Password hashing: argon2id only (fresh start, no legacy PHP bcrypt hashes).
//!
//! [`needs_rehash`] lets the login flow upgrade hashes when the argon2 parameters change:
//! after a successful [`verify`], store a fresh [`hash`] if it returns true.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};

#[derive(Debug, thiserror::Error)]
#[error("password hashing failed: {0}")]
pub struct PasswordError(String);

/// The hasher with the current parameters (RustCrypto defaults: m=19 MiB, t=2, p=1).
fn argon2() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

/// Hashes `password` with argon2id and a random salt (PHC string `$argon2id$v=19$...`).
pub fn hash(password: &str) -> Result<String, PasswordError> {
    argon2()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| PasswordError(e.to_string()))
}

/// Checks `password` against a stored argon2 PHC hash. Malformed hashes never verify.
pub fn verify(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        // The parameters are taken from the hash itself.
        Ok(parsed) => argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// True if `stored_hash` is not argon2id v19 with the current m/t/p parameters.
pub fn needs_rehash(stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return true;
    };
    if parsed.algorithm.as_str() != Algorithm::Argon2id.ident().as_str()
        || parsed.version != Some(Version::V0x13.into())
    {
        return true;
    }
    let current = Params::default();
    match Params::try_from(&parsed) {
        Ok(p) => {
            p.m_cost() != current.m_cost()
                || p.t_cost() != current.t_cost()
                || p.p_cost() != current.p_cost()
        }
        Err(_) => true,
    }
}
