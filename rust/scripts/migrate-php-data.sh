#!/bin/bash
# One-off migration of the PHP app's data (its MariaDB data directory) into the Rust schema.
#
#   scripts/migrate-php-data.sh <old-volume.tar.gz | old-datadir> [output.sql]
#
# Produces output.sql (default ./php-data-migrated.sql, mode 600: personal data and password
# hashes). Nothing touches a running database: the old data directory is read by a
# throwaway MariaDB of its own version (no network, copy of the files), transformed in a
# second throwaway MariaDB (the production image) that has every rust/migrations/*.sql
# applied, so foreign keys and CHECK constraints are exercised before anything is written.
#
# Import on production (after the app has started once, so its migrations ran, and before
# anyone registers; the file refuses to load into a database that already has users or
# groups, or whose newest migration differs from the one it was generated against):
#
#   podman compose exec -T mariadb sh -c \
#     'mariadb -u"$MARIADB_USER" -p"$MARIADB_PASSWORD" "$MARIADB_DATABASE"' < php-data-migrated.sql
#
# What changes on the way (see docs/rewrite/PLAN.md, decision log 2026-11-03):
# - emails trimmed and lowercased; names, titles, group names trimmed; empty descriptions
#   and links become NULL; CRLF line breaks become LF
# - password hashes are copied as they are (PHP bcrypt `$2y$`); reset and email-change
#   tokens (plaintext in PHP) and pending email changes are dropped
# - registration_deadline / draw_date: PHP wrote Berlin wall-clock times into a UTC session;
#   they are converted from $SOURCE_TIMEZONE (default Europe/Berlin) to UTC
# - groups that are drawn get drawn_at = their newest assignment; assignments are marked as
#   notified (the mail worker must not send last year's draw again), self-assignments and
#   assignments of undrawn groups are dropped
# - wishlist items get position = order of their ids; exclusion rules are kept only when
#   both users are members and not the same user; email_templates is not migrated
#
# Env: OLD_IMAGE (default mariadb:<version from mariadb_upgrade_info>), TARGET_IMAGE
# (default: the mariadb image of compose.yml), SOURCE_TIMEZONE, KEEP_WORKDIR=1.
set -euo pipefail

usage() { sed -n '2,4p' "$0" >&2; exit 2; }
[ $# -ge 1 ] && [ $# -le 2 ] || usage
# Paths are relative to the caller's directory, not to rust/.
input="$(realpath -e "$1")"
output="$(realpath -m "${2:-php-data-migrated.sql}")"
cd "$(dirname "$0")/.."
SOURCE_TIMEZONE="${SOURCE_TIMEZONE:-Europe/Berlin}"
TARGET_IMAGE="${TARGET_IMAGE:-$(sed -n '/^  mariadb:/,/image:/s/^ *image: *//p' compose.yml | head -1)}"
[ -n "$TARGET_IMAGE" ] || { echo "no mariadb image found in compose.yml; set TARGET_IMAGE" >&2; exit 1; }

tables="users groups group_members wishlists wishlist_items exclusion_rules gift_assignments"
migrations_dir="$PWD/migrations"
schema_version=$(ls "$migrations_dir" | sed -n 's/^0*\([0-9][0-9]*\)_.*\.sql$/\1/p' | sort -n | tail -1)

work="$(mktemp -d "${TMPDIR:-/tmp}/php-migrate.XXXXXX")"
old_ctr="php-migrate-old-$$"
new_ctr="php-migrate-new-$$"
cleanup() {
    podman rm -f "$old_ctr" "$new_ctr" >/dev/null 2>&1 || true
    if [ "${KEEP_WORKDIR:-}" = 1 ]; then echo "work dir kept: $work" >&2; else rm -rf "$work"; fi
}
trap cleanup EXIT

log() { printf '==> %s\n' "$*" >&2; }

# --- 1. Old data directory -------------------------------------------------------------
if [ -d "$input" ]; then
    cp -a "$input" "$work/extract"
else
    mkdir "$work/extract"
    tar -xzf "$input" -C "$work/extract"
fi
upgrade_info="$(find "$work/extract" -name mariadb_upgrade_info -print -quit)"
[ -n "$upgrade_info" ] || { echo "no MariaDB data directory (mariadb_upgrade_info) in $input" >&2; exit 1; }
datadir="$(dirname "$upgrade_info")"
[ -d "$datadir/irgendwas_db" ] || { echo "no irgendwas_db in $datadir" >&2; exit 1; }
old_version="$(sed -n 's/^\([0-9][0-9.]*\)-MariaDB.*/\1/p' "$upgrade_info")"
OLD_IMAGE="${OLD_IMAGE:-docker.io/library/mariadb:$old_version}"

# Runs mariadb as the invoking user (keep-id), so every file stays deletable without root.
# Podman options go in $DB_ENV (array of -e ...), server arguments after the datadir.
run_db() { # name image datadir [server args...]
    local name="$1" image="$2" dir="$3"; shift 3
    podman run -d --name "$name" --network none --userns=keep-id --user "$(id -u):$(id -g)" \
        -v "$dir:/var/lib/mysql:Z" "${DB_ENV[@]}" "$image" "$@" >/dev/null
}
wait_ready() { # name
    for _ in $(seq 1 120); do
        podman exec "$1" mariadb -uroot -e 'SELECT 1' >/dev/null 2>&1 && return 0
        sleep 1
    done
    podman logs "$1" >&2; echo "$1 did not start" >&2; exit 1
}

log "old data: MariaDB $old_version ($OLD_IMAGE), read-only copy, no network"
DB_ENV=()
run_db "$old_ctr" "$OLD_IMAGE" "$datadir" --skip-grant-tables --skip-networking
wait_ready "$old_ctr"
podman exec "$old_ctr" mariadb-dump -uroot --skip-comments --hex-blob --tz-utc \
    --skip-add-locks --skip-disable-keys --skip-triggers irgendwas_db >"$work/old.sql"
podman exec "$old_ctr" mariadb -uroot -N irgendwas_db -e "
    SELECT 'users', COUNT(*) FROM users UNION ALL
    SELECT 'groups', COUNT(*) FROM \`groups\` UNION ALL
    SELECT 'group_members', COUNT(*) FROM group_members UNION ALL
    SELECT 'wishlists', COUNT(*) FROM wishlists UNION ALL
    SELECT 'wishlist_items', COUNT(*) FROM wishlist_items UNION ALL
    SELECT 'exclusion_rules', COUNT(*) FROM exclusion_rules UNION ALL
    SELECT 'gift_assignments', COUNT(*) FROM gift_assignments" >"$work/old-counts.tsv"
podman rm -f "$old_ctr" >/dev/null

# --- 2. Target: production image, every Rust migration, old data in php_import ---------
log "target: $TARGET_IMAGE with migrations 1..$schema_version"
mkdir "$work/new"
DB_ENV=(-e MARIADB_ALLOW_EMPTY_ROOT_PASSWORD=1 -e MARIADB_DATABASE=irgendwas_db)
run_db "$new_ctr" "$TARGET_IMAGE" "$work/new"
# The entrypoint's init server also answers on the socket: wait for the real one.
for _ in $(seq 1 120); do
    podman logs "$new_ctr" 2>&1 | grep -q 'init process done' && break
    sleep 1
done
wait_ready "$new_ctr"

sql() { podman exec -i "$new_ctr" mariadb -uroot "$@"; }
for f in $(ls "$migrations_dir"/*.sql | sort); do
    sql irgendwas_db <"$f"
done
sql -e 'CREATE DATABASE php_import'
sql php_import <"$work/old.sql"

# --- 3. Transform -----------------------------------------------------------------------
log "transform"
sql irgendwas_db <<SQL
SET time_zone = '+00:00';
SET FOREIGN_KEY_CHECKS = 1;

-- CONVERT_TZ returns NULL without the time zone tables; that must not drop dates silently.
DELIMITER ;;
BEGIN NOT ATOMIC
    IF CONVERT_TZ('2025-01-01 12:00:00', '$SOURCE_TIMEZONE', '+00:00') IS NULL THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'time zone $SOURCE_TIMEZONE unknown to the target server';
    END IF;
END;;
DELIMITER ;

START TRANSACTION;

INSERT INTO users (id, email, password, name, created_at, updated_at, last_login,
                   reset_token, reset_token_expires, pending_email,
                   email_verification_token, email_verification_expires,
                   locale, session_epoch)
SELECT id, LOWER(TRIM(email)), password, TRIM(name), created_at, updated_at, last_login,
       NULL, NULL, NULL, NULL, NULL, NULL, 0
FROM php_import.users ORDER BY id;

INSERT INTO \`groups\` (id, name, description, admin_id, invitation_code,
                      registration_deadline, draw_date, is_drawn, drawn_at,
                      created_at, updated_at)
SELECT g.id, TRIM(g.name),
       NULLIF(TRIM(REPLACE(REPLACE(g.description, '\r\n', '\n'), '\r', '\n')), ''),
       g.admin_id, g.invitation_code,
       CONVERT_TZ(g.registration_deadline, '$SOURCE_TIMEZONE', '+00:00'),
       CONVERT_TZ(g.draw_date, '$SOURCE_TIMEZONE', '+00:00'),
       COALESCE(g.is_drawn, FALSE),
       CASE WHEN g.is_drawn THEN COALESCE(
           (SELECT MAX(a.created_at) FROM php_import.gift_assignments a WHERE a.group_id = g.id),
           g.updated_at, g.created_at) END,
       g.created_at, g.updated_at
FROM php_import.\`groups\` g ORDER BY g.id;

INSERT INTO group_members (id, group_id, user_id, joined_at)
SELECT id, group_id, user_id, joined_at FROM php_import.group_members ORDER BY id;

INSERT INTO wishlists (id, user_id, group_id, created_at, updated_at)
SELECT id, user_id, group_id, created_at, updated_at FROM php_import.wishlists ORDER BY id;

INSERT INTO wishlist_items (id, wishlist_id, title, description, link, position,
                            created_at, updated_at)
SELECT id, wishlist_id, TRIM(title),
       NULLIF(TRIM(REPLACE(REPLACE(description, '\r\n', '\n'), '\r', '\n')), ''),
       NULLIF(TRIM(link), ''),
       ROW_NUMBER() OVER (PARTITION BY wishlist_id ORDER BY id),
       created_at, updated_at
FROM php_import.wishlist_items ORDER BY id;

INSERT INTO exclusion_rules (id, group_id, user_id, excluded_user_id, created_at)
SELECT e.id, e.group_id, e.user_id, e.excluded_user_id, e.created_at
FROM php_import.exclusion_rules e
JOIN php_import.group_members giver
    ON giver.group_id = e.group_id AND giver.user_id = e.user_id
JOIN php_import.group_members excluded
    ON excluded.group_id = e.group_id AND excluded.user_id = e.excluded_user_id
WHERE e.user_id <> e.excluded_user_id
ORDER BY e.id;

INSERT INTO gift_assignments (id, group_id, giver_id, receiver_id, created_at,
                              notification_sent, notification_attempts,
                              notification_last_attempt_at)
SELECT a.id, a.group_id, a.giver_id, a.receiver_id, a.created_at, TRUE, 0, NULL
FROM php_import.gift_assignments a
JOIN php_import.\`groups\` g ON g.id = a.group_id AND g.is_drawn
WHERE a.giver_id <> a.receiver_id
ORDER BY a.id;

COMMIT;
SQL

sql -N irgendwas_db -e "
    SELECT 'users', COUNT(*) FROM users UNION ALL
    SELECT 'groups', COUNT(*) FROM \`groups\` UNION ALL
    SELECT 'group_members', COUNT(*) FROM group_members UNION ALL
    SELECT 'wishlists', COUNT(*) FROM wishlists UNION ALL
    SELECT 'wishlist_items', COUNT(*) FROM wishlist_items UNION ALL
    SELECT 'exclusion_rules', COUNT(*) FROM exclusion_rules UNION ALL
    SELECT 'gift_assignments', COUNT(*) FROM gift_assignments" >"$work/new-counts.tsv"

# Every drawn group must have exactly one assignment per member (the Rust app reveals
# assignments only for complete draws; anything else needs a look before importing).
incomplete="$(sql -N irgendwas_db -e "
    SELECT g.id FROM \`groups\` g WHERE g.is_drawn AND
        (SELECT COUNT(*) FROM group_members m WHERE m.group_id = g.id) <>
        (SELECT COUNT(*) FROM gift_assignments a WHERE a.group_id = g.id)")"

# --- 4. Output --------------------------------------------------------------------------
log "write $output"
umask 077
{
    cat <<SQL
-- PHP data migrated to the Rust schema (migration $schema_version) by scripts/migrate-php-data.sh
-- on $(date -u +%Y-%m-%dT%H:%M:%SZ). Contains personal data and password hashes.
DELIMITER ;;
BEGIN NOT ATOMIC
    IF (SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success) <> $schema_version THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT =
            'schema is not at migration $schema_version: regenerate this file against the deployed migrations';
    END IF;
    IF EXISTS (SELECT 1 FROM users) OR EXISTS (SELECT 1 FROM \`groups\`)
        OR EXISTS (SELECT 1 FROM pending_registrations) THEN
        SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'target database is not empty: nothing imported';
    END IF;
END;;
DELIMITER ;
START TRANSACTION;
SQL
    # shellcheck disable=SC2086 # $tables is a word list
    podman exec "$new_ctr" mariadb-dump -uroot --skip-comments --hex-blob --tz-utc \
        --no-create-info --complete-insert --skip-add-locks --skip-disable-keys \
        --skip-triggers irgendwas_db $tables \
        | grep -v '^/\*M!999999\\- enable the sandbox mode \*/'
    echo 'COMMIT;'
} >"$output.tmp"
mv "$output.tmp" "$output"

# --- 5. Summary -------------------------------------------------------------------------
printf '\n%-18s %6s %6s\n' table old new >&2
join -t "$(printf '\t')" <(sort "$work/old-counts.tsv") <(sort "$work/new-counts.tsv") \
    | while IFS="$(printf '\t')" read -r t o n; do
        mark=""; [ "$o" = "$n" ] || mark="  <- differs"
        printf '%-18s %6s %6s%s\n' "$t" "$o" "$n" "$mark" >&2
    done
if [ -n "$incomplete" ]; then
    echo "WARNING: drawn groups without one assignment per member: $(echo $incomplete)" >&2
fi
echo "Written: $output (schema migration $schema_version)" >&2
