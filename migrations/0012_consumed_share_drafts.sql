-- A draft token is single-use even if concurrent requests loaded the same
-- session snapshot. Only a one-way hash is persisted. The claim and resulting
-- wishlist item are committed atomically. Claims are retained for the life of
-- the account: sessions are rolling and drafts can otherwise remain valid
-- indefinitely. Deleting the account removes its claims.
CREATE TABLE consumed_share_drafts (
    draft_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    user_id INT NOT NULL,
    consumed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
