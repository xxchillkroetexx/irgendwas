-- 0005 groups: invitation codes are compared byte for byte.
--
-- Codes are stored upper case (normalised in Rust) and looked up exactly. With the table
-- default utf8mb4_unicode_ci the lookup ignored case and accents ('ÀBCDEFGH' matched
-- 'ABCDEFGH'), unlike the normalisation rule. MODIFY keeps the UNIQUE index; existing
-- codes stay valid (a binary collation only tells more values apart).
ALTER TABLE `groups`
    MODIFY invitation_code VARCHAR(32) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;
