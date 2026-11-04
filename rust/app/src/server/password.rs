//! Password hashing: argon2id only. The PHP bcrypt hashes kept by the data migration
//! (`scripts/migrate-php-data.sh`) never verify; those users reset their password.
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

/// Checks `password` against a stored argon2 PHC hash. Anything else (malformed, or a
/// migrated PHP bcrypt hash) never verifies but still costs one argon2id run against the
/// dummy hash: otherwise such an account would answer faster than an unknown email (which
/// is checked against the dummy hash too) and give itself away.
pub fn verify(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        // The parameters are taken from the hash itself.
        Ok(parsed) if Algorithm::new(parsed.algorithm.as_str()).is_ok() => argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        _ => {
            let _ = verify(password, &DUMMY_HASH);
            false
        }
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

/// A real argon2id hash (current parameters) of a random password, computed once per
/// process. Logins for unknown emails verify against it, so they cost the same time as a
/// wrong password for an existing account.
static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let mut secret = [0u8; 32];
    // Without OS randomness the fixed bytes are still fine: the hash is never matched.
    let _ = getrandom::fill(&mut secret);
    let password: String = secret.iter().map(|b| format!("{b:02x}")).collect();
    hash(&password).expect("argon2 hashes a fixed-size password")
});

/// Computes the dummy hash now (call at startup so the first login does not pay for it).
pub fn init_dummy_hash() {
    std::sync::LazyLock::force(&DUMMY_HASH);
}

/// [`hash`] on the blocking thread pool (argon2 takes tens of milliseconds of CPU).
pub async fn hash_async(password: String) -> Result<String, PasswordError> {
    tokio::task::spawn_blocking(move || hash(&password))
        .await
        .map_err(|e| PasswordError(e.to_string()))?
}

/// [`verify`] on the blocking thread pool. `stored_hash: None` verifies against the dummy
/// hash (and always fails), so unknown accounts take as long as existing ones.
pub async fn verify_async(password: String, stored_hash: Option<String>) -> bool {
    tokio::task::spawn_blocking(move || match stored_hash {
        Some(stored) => verify(&password, &stored),
        None => {
            let _ = verify(&password, &DUMMY_HASH);
            false
        }
    })
    .await
    .unwrap_or(false)
}
