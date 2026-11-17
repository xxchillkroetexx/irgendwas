#!/bin/sh
# Run an entire CI build/test phase under a lock outside its shared target dir.
# Usage: sh scripts/ci-target-dir.sh /ci-cache/target-check command [args...]
# If the volume is absent, execute the command against the checkout's own target.
#   TARGET_CACHE_MAX_MB  wipe the directory above this size (default 20000), under the
#                        lock and before the phase starts; it grows with every dependency
#                        or toolchain update
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
    # Under the exclusive lock: every job that uses this directory (the -v2 paths) holds
    # it for its whole phase, so no build can be running in it now. The lock file lives
    # next to the directory and survives the wipe.
    echo "above ${max} MB: starting over"
    rm -rf "$dir"
    mkdir -p "$dir"
fi
rm -rf target
ln -s "$dir" target
exec "$@"
