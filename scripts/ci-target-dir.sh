#!/bin/sh
# Run an entire CI build/test phase under a lock outside its shared target dir.
# Usage: sh scripts/ci-target-dir.sh /ci-cache/target-check command [args...]
# If the volume is absent, execute the command against the checkout's own target.
set -eu
cd "$(dirname "$0")/.."

if [ "${1:-}" != "--locked" ]; then
    dir="$1"
    shift
    if [ ! -d "$(dirname "$dir")" ]; then
        echo "$(dirname "$dir") is not mounted: building without a persistent target dir" >&2
        exec "$@"
    fi
    # Keep the same inode for every job; never remove this lock file with the cache.
    exec flock -x "$dir.lock" sh "$0" --locked "$dir" "$@"
fi

shift
dir="$1"
shift
max="${TARGET_CACHE_MAX_MB:-20000}"
mkdir -p "$dir"
size="$(du -sm "$dir" | cut -f1)"
echo "persistent target dir $dir: ${size} MB"
if [ "$size" -gt "$max" ]; then
    # Old workflow runs do not hold our lock. Even under flock, deleting this
    # directory could destroy one of those builds. An operator may clean it only
    # after all old and new jobs have stopped.
    echo "above ${max} MB: clean the cache manually after all CI jobs stop" >&2
fi
rm -rf target
ln -s "$dir" target
exec "$@"
