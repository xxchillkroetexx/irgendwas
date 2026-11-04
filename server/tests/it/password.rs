//! argon2id hashing (`app::server::password`).

use app::server::password::{hash, init_dummy_hash, needs_rehash, verify};
use std::time::{Duration, Instant};

/// A PHP `password_hash` value, as kept by the data migration.
const PHP_BCRYPT: &str = "$2y$10$abcdefghijklmnopqrstuuJ5Z5nZQm2c1yS1V5gW8kq3yJ4bq7tWe";

#[test]
fn hash_and_verify_roundtrip() {
    let h = hash("hunter2-but-longer").unwrap();
    assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{h}");
    assert!(verify("hunter2-but-longer", &h));
    assert!(!verify("hunter2-but-longe", &h));
    assert!(!needs_rehash(&h));
    // Salted: hashing twice gives different strings.
    assert_ne!(h, hash("hunter2-but-longer").unwrap());
}

#[test]
fn verifies_hash_with_other_params_and_flags_rehash() {
    use argon2::password_hash::PasswordHasher;
    use argon2::{Algorithm, Argon2, Params, Version};

    // A hash made with weaker (older) parameters still verifies, but asks for a rehash.
    let weak = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(4096, 1, 1, None).unwrap(),
    );
    let old = weak.hash_password(b"password").unwrap().to_string();
    assert!(old.contains("m=4096,t=1,p=1"), "{old}");
    assert!(verify("password", &old));
    assert!(!verify("passwort", &old));
    assert!(needs_rehash(&old));

    // Same for argon2i.
    let argon2i = Argon2::new(Algorithm::Argon2i, Version::V0x13, Params::default());
    let h = argon2i.hash_password(b"password").unwrap().to_string();
    assert!(verify("password", &h));
    assert!(needs_rehash(&h));
}

#[test]
fn rejects_malformed_and_foreign_hashes() {
    assert!(!verify("x", ""));
    assert!(!verify("x", "not a hash"));
    // PHP bcrypt hashes (kept by the data migration) never verify: users reset.
    assert!(!verify("x", PHP_BCRYPT));
    assert!(needs_rehash(PHP_BCRYPT));
    assert!(needs_rehash("garbage"));
}

/// Fastest of a few runs: parallel tests only ever make a run slower.
fn fastest(stored_hash: &str) -> Duration {
    (0..3)
        .map(|_| {
            let started = Instant::now();
            assert!(!verify("not the password", stored_hash));
            started.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn foreign_hashes_cost_as_much_as_a_real_verification() {
    init_dummy_hash();
    let real = fastest(&hash("the real password").unwrap());
    // Without the dummy run these return in microseconds, over 1000x faster than argon2id:
    // a migrated (or broken) account would be told apart from an unknown email by timing.
    for foreign in [PHP_BCRYPT, "not a hash", ""] {
        let took = fastest(foreign);
        assert!(
            took * 4 >= real,
            "{foreign:?}: {took:?}, real argon2id verification {real:?}"
        );
    }
}
