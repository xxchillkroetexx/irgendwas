ALTER TABLE wishlist_items ADD COLUMN position INT NOT NULL DEFAULT 0 AFTER link;
CREATE INDEX idx_wishlist_items_order ON wishlist_items (wishlist_id, position, id);
