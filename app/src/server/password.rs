//! Password hashing: argon2id only. Accounts migrated from the PHP app keep a bcrypt hash
//! until they reset their password; it never verifies.
//!
//! [`needs_rehash`] lets the login flow upgrade hashes when the argon2 parameters change:
//! after a successful [`verify`], store a fresh [`hash`] if it returns true.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::Semaphore;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password work is temporarily busy")]
    Busy,
    #[error("password hashing failed: {0}")]
    Failed(String),
}

// Argon2's default cost is 19 MiB per job. Limit work per process, including
// verification and rehashing, before submitting to Tokio's blocking pool.
const MAX_PASSWORD_JOBS: usize = 4;
// Bound both active and waiting requests, so a valid token cannot create an
// unbounded number of pending tasks. Full queues fail promptly without using it.
const MAX_PASSWORD_REQUESTS: usize = 36;

#[derive(Clone)]
struct WorkLimiter {
    running: Arc<Semaphore>,
    admitted: Arc<Semaphore>,
}

impl WorkLimiter {
    fn new(running: usize, admitted: usize) -> Self {
        Self {
            running: Arc::new(Semaphore::new(running)),
            admitted: Arc::new(Semaphore::new(admitted)),
        }
    }

    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, PasswordError> {
        let admitted = self
            .admitted
            .clone()
            .try_acquire_owned()
            .map_err(|_| PasswordError::Busy)?;
        let running = self
            .running
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| PasswordError::Failed(e.to_string()))?;
        tokio::task::spawn_blocking(move || {
            // Neither permit is released early if the HTTP request is cancelled.
            let (_admitted, _running) = (admitted, running);
            work()
        })
        .await
        .map_err(|e| PasswordError::Failed(e.to_string()))
    }
}

static PASSWORD_LIMITER: LazyLock<WorkLimiter> =
    LazyLock::new(|| WorkLimiter::new(MAX_PASSWORD_JOBS, MAX_PASSWORD_REQUESTS));

// Two in flight preserve the documented concurrent-token race semantics while
// preventing a single valid token from occupying the entire global queue.
const MAX_PER_TOKEN: usize = 2;
static BUSY_TOKENS: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct TokenGuard(String);

impl TokenGuard {
    fn reserve(token_hash: String) -> Result<Self, PasswordError> {
        let mut tokens = BUSY_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
        let count = tokens.entry(token_hash.clone()).or_insert(0);
        if *count >= MAX_PER_TOKEN {
            return Err(PasswordError::Busy);
        }
        *count += 1;
        Ok(Self(token_hash))
    }
}

impl Drop for TokenGuard {
    fn drop(&mut self) {
        let mut tokens = BUSY_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = tokens.get_mut(&self.0) {
            *count -= 1;
            if *count == 0 {
                tokens.remove(&self.0);
            }
        }
    }
}

async fn bounded_work<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, PasswordError> {
    PASSWORD_LIMITER.run(work).await
}

/// Exercise the same process-wide admission gate with a controlled blocking job.
#[cfg(feature = "test-support")]
pub async fn test_password_work(work: impl FnOnce() + Send + 'static) -> Result<(), PasswordError> {
    bounded_work(work).await
}

/// A private limiter for concurrency tests, independent of live requests in the suite.
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct TestPasswordLimiter(WorkLimiter);

#[cfg(feature = "test-support")]
impl TestPasswordLimiter {
    pub fn new(running: usize, admitted: usize) -> Self {
        Self(WorkLimiter::new(running, admitted))
    }

    pub async fn run(&self, work: impl FnOnce() + Send + 'static) -> Result<(), PasswordError> {
        self.0.run(work).await
    }

    pub fn available_admissions(&self) -> usize {
        self.0.admitted.available_permits()
    }
}

async fn bounded_token_work<T: Send + 'static>(
    token_hash: String,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, PasswordError> {
    let guard = TokenGuard::reserve(token_hash)?;
    bounded_work(move || {
        let _guard = guard;
        work()
    })
    .await
}

#[cfg(feature = "test-support")]
pub async fn test_token_work(
    token_hash: String,
    work: impl FnOnce() + Send + 'static,
) -> Result<(), PasswordError> {
    bounded_token_work(token_hash, work).await
}

/// The hasher with the current parameters (RustCrypto defaults: m=19 MiB, t=2, p=1).
fn argon2() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

/// Hashes `password` with argon2id and a random salt (PHC string `$argon2id$v=19$...`).
pub fn hash(password: &str) -> Result<String, PasswordError> {
    argon2()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Failed(e.to_string()))
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
    bounded_work(move || hash(&password)).await?
}

/// At most two registration/reset operations per live token may be hashing or
/// waiting in this process. The guard survives cancellation of blocking work.
pub async fn hash_for_token_async(
    password: String,
    token_hash: String,
) -> Result<String, PasswordError> {
    bounded_token_work(token_hash, move || hash(&password)).await?
}

/// [`verify`] on the blocking thread pool. `stored_hash: None` verifies against the dummy
/// hash (and always fails), so unknown accounts take as long as existing ones.
pub async fn verify_async(
    password: String,
    stored_hash: Option<String>,
) -> Result<bool, PasswordError> {
    bounded_work(move || match stored_hash {
        Some(stored) => verify(&password, &stored),
        None => {
            let _ = verify(&password, &DUMMY_HASH);
            false
        }
    })
    .await
}
