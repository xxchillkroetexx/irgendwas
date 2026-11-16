-- 0009 group archive (spec: group-archive). New columns start NULL: nothing is archived
-- or warned by the migration; the first sweep after startup archives the drawn groups that
-- are past their archive date (decision U6) and warns those inside the 14-day window.
--
-- `TIMESTAMP NULL DEFAULT NULL` is spelled out so no column gets an implicit
-- CURRENT_TIMESTAMP default or an ON UPDATE clause.
ALTER TABLE `groups`
    ADD COLUMN archived_at TIMESTAMP NULL DEFAULT NULL AFTER drawn_at,
    ADD COLUMN archive_warned_at TIMESTAMP NULL DEFAULT NULL AFTER archived_at,
    ADD COLUMN kept_active_at TIMESTAMP NULL DEFAULT NULL AFTER archive_warned_at;

-- Groups that are not drawn (never drawn, or reset) get a fresh start at the deploy (U10,
-- revised after the review): their clock restarts now, so none of them is archived before
-- nine months have passed and a warning was sent. (`updated_at` cannot serve: the PHP
-- import kept each group's last PHP change.) Assigning updated_at to itself keeps its
-- ON UPDATE CURRENT_TIMESTAMP from touching it.
UPDATE `groups`
SET kept_active_at = CURRENT_TIMESTAMP, updated_at = updated_at
WHERE is_drawn = FALSE;
