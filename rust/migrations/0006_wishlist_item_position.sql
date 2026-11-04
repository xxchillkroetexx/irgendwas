-- 0006 wishlists: the owner's order of the items (spec W7).
--
-- Reads use ORDER BY position, id. A new item goes to the end (MAX(position) + 1) and a move
-- rewrites the list's positions as 1..n. No UNIQUE constraint: correctness only relies on
-- the ORDER BY, and swaps need no temporary values.
ALTER TABLE wishlist_items
    ADD COLUMN position INT NOT NULL DEFAULT 0 AFTER link;

-- Rows that already exist keep their creation order, numbered 1..n per list.
UPDATE wishlist_items i
    JOIN (
        SELECT id, ROW_NUMBER() OVER (PARTITION BY wishlist_id ORDER BY id) AS n
        FROM wishlist_items
    ) numbered ON numbered.id = i.id
SET i.position = numbered.n;

CREATE INDEX idx_wishlist_items_order ON wishlist_items (wishlist_id, position, id);
