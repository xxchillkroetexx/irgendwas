-- 0002 sessions: storage for tower-sessions (server::session_store::MySqlSessionStore).
-- id is the base64url session id (case-sensitive, hence ascii_bin).
-- data is the JSON-encoded tower_sessions Record. expiry_date is UTC (connection time_zone +00:00).
CREATE TABLE sessions (
    id CHAR(22) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    data BLOB NOT NULL,
    expiry_date TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    INDEX idx_sessions_expiry_date (expiry_date)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
