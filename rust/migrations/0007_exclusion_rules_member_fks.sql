-- 0007 exclusions: a rule can only exist between two current members of its group, and
-- removing a membership removes the rules in both directions (spec: exclusions, "Data &
-- migrations"; backs up groups D5, which still deletes them explicitly).
--
-- `group_members` has UNIQUE(group_id, user_id) (`unique_member`), a valid FK target.
-- `(group_id, user_id)` is covered by the prefix of `unique_exclusion`; InnoDB creates the
-- index for `(group_id, excluded_user_id)`. The single-column FKs and `unique_exclusion`
-- stay. Self-rules are refused by `add_exclusion` and ignored by the feasibility check.

-- Fresh start: the table is empty here. On a database that already holds data (e.g. one
-- created from the PHP schema), rules whose giver or excluded user is no longer a member
-- would make the foreign keys fail, so they go first (they never applied to a draw).
DELETE e FROM exclusion_rules e
    LEFT JOIN group_members giver
        ON giver.group_id = e.group_id AND giver.user_id = e.user_id
    LEFT JOIN group_members excluded
        ON excluded.group_id = e.group_id AND excluded.user_id = e.excluded_user_id
WHERE giver.id IS NULL OR excluded.id IS NULL;

ALTER TABLE exclusion_rules
    ADD CONSTRAINT fk_exclusion_giver_member
        FOREIGN KEY (group_id, user_id) REFERENCES group_members (group_id, user_id)
        ON DELETE CASCADE,
    ADD CONSTRAINT fk_exclusion_excluded_member
        FOREIGN KEY (group_id, excluded_user_id) REFERENCES group_members (group_id, user_id)
        ON DELETE CASCADE;
