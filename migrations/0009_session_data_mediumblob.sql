-- Five valid share drafts can exceed the 64 KiB BLOB limit after JSON encoding.
-- Widen in place so existing authenticated sessions and their expiry survive upgrades.
ALTER TABLE sessions MODIFY COLUMN data MEDIUMBLOB NOT NULL;
