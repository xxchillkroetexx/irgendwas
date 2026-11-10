-- Existing assignment rows remain the durable draw-result queue. Leases coordinate
-- workers in different app instances; expiration recovers interrupted workers.
ALTER TABLE gift_assignments
    ADD COLUMN notification_claim CHAR(43) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ADD COLUMN notification_claim_until TIMESTAMP(6) NULL,
    ADD COLUMN notification_next_attempt_at TIMESTAMP(6) NULL;

-- A generation identifies the current draw/reset cycle, including on old groups.
ALTER TABLE `groups`
    ADD COLUMN draw_generation BIGINT UNSIGNED NOT NULL DEFAULT 0,
    ADD COLUMN draw_mail_claim CHAR(43) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ADD COLUMN draw_mail_claim_until TIMESTAMP(6) NULL;

-- A reset is recorded in the same transaction as deleting the assignments. It
-- contains no pairing or receiver data; recipient details are read when sending.
CREATE TABLE draw_reset_notifications (
    id INT AUTO_INCREMENT PRIMARY KEY,
    group_id INT NOT NULL,
    recipient_id INT NOT NULL,
    generation BIGINT UNSIGNED NOT NULL,
    group_name VARCHAR(255) NOT NULL,
    admin_name VARCHAR(255) NOT NULL,
    attempts TINYINT UNSIGNED NOT NULL DEFAULT 0,
    claim CHAR(43) CHARACTER SET ascii COLLATE ascii_bin NULL,
    claim_until TIMESTAMP(6) NULL,
    next_attempt_at TIMESTAMP(6) NULL,
    INDEX idx_reset_notifications_group (group_id),
    FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
    FOREIGN KEY (recipient_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
