-- 0008 draw (spec: draw, "Data & migrations"): the draw timestamp, the per-row delivery state
-- of the draw mails, "nobody gives to themselves" at the DB level, and the unused
-- `email_templates` table goes.
--
-- Fresh start: the tables are empty here. On a database that already holds data (e.g. one
-- created from the PHP schema), the statements before each ALTER make it apply cleanly:
-- NULL flags become FALSE (the PHP columns were nullable), self-assignments (never valid)
-- are deleted so the CHECK can be added, and groups that are already drawn get a
-- `drawn_at` (invariant D6: is_drawn = TRUE <=> drawn_at IS NOT NULL).

UPDATE `groups` SET is_drawn = FALSE WHERE is_drawn IS NULL;

ALTER TABLE `groups`
    MODIFY is_drawn BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN drawn_at TIMESTAMP NULL DEFAULT NULL AFTER is_drawn;

UPDATE `groups` g
SET g.drawn_at = COALESCE(
        (SELECT MAX(ga.created_at) FROM gift_assignments ga WHERE ga.group_id = g.id),
        CURRENT_TIMESTAMP)
WHERE g.is_drawn = TRUE;

UPDATE gift_assignments SET notification_sent = FALSE WHERE notification_sent IS NULL;
DELETE FROM gift_assignments WHERE giver_id = receiver_id;

ALTER TABLE gift_assignments
    MODIFY notification_sent BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN notification_attempts TINYINT UNSIGNED NOT NULL DEFAULT 0 AFTER notification_sent,
    ADD COLUMN notification_last_attempt_at TIMESTAMP NULL DEFAULT NULL AFTER notification_attempts,
    ADD CONSTRAINT chk_gift_assignments_not_self CHECK (giver_id <> receiver_id);

DROP TABLE email_templates;
