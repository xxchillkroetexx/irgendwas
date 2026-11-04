-- 0004 auth: session epoch (all sessions of a user die when the epoch changes), the
-- two-step registration table, and byte-wise collation for identity columns.
--
-- Emails are stored trimmed and lowercase (normalised in Rust) and compared byte for byte:
-- the table default utf8mb4_unicode_ci is accent-insensitive ('gmail' = 'gmäil'), which
-- would let a lookalike address hit another user's account or pending registration.
ALTER TABLE users
    ADD COLUMN session_epoch INT UNSIGNED NOT NULL DEFAULT 0,
    MODIFY email VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    MODIFY pending_email VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL,
    MODIFY reset_token VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL,
    MODIFY email_verification_token VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL;

CREATE TABLE pending_registrations (
    id INT AUTO_INCREMENT PRIMARY KEY,
    email VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL UNIQUE,
    token_hash CHAR(64) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL UNIQUE,
    next_path VARCHAR(512) NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- NULL DEFAULT NULL on purpose: with explicit_defaults_for_timestamp=OFF a NOT NULL
    -- TIMESTAMP without default would silently get ON UPDATE CURRENT_TIMESTAMP.
    -- Code always writes expires_at; a NULL row counts as expired.
    expires_at TIMESTAMP NULL DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
