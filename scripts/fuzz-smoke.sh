#!/bin/sh
# Run a bounded exploration against a temporary copy of the checked-in seeds.
set -eu
cd "$(dirname "$0")/.."
seconds="${FUZZ_SECONDS:-20}"
case "$seconds" in
    *[!0-9]*|'') echo "FUZZ_SECONDS must be an integer from 1 to 86400" >&2; exit 2 ;;
esac
if ! [ "$seconds" -ge 1 ] 2>/dev/null || ! [ "$seconds" -le 86400 ] 2>/dev/null; then
    echo "FUZZ_SECONDS must be an integer from 1 to 86400" >&2
    exit 2
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/corpus"
cp fuzz/corpus/redirect_paths/* "$work/corpus/"
# Print and retain any crash outside the temporary directory so it can be replayed.
artifacts="${FUZZ_ARTIFACTS_DIR:-$PWD/fuzz/artifacts/redirect_paths}"
mkdir -p "$artifacts"
# Keep AddressSanitizer quarantine bounded on the shared CI runner.
ASAN_OPTIONS="${ASAN_OPTIONS:-quarantine_size_mb=32:thread_local_quarantine_size_kb=256}"
export ASAN_OPTIONS
case "$artifacts" in /*) ;; *) artifacts="$PWD/$artifacts" ;; esac
cargo +nightly-2026-11-01 metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps > /dev/null
cargo +nightly-2026-11-01 fuzz run redirect_paths "$work/corpus" -- \
    -max_total_time="$seconds" -timeout=5 -max_len=2048 \
    -rss_limit_mb=1024 -verbosity=0 -print_final_stats=1 -artifact_prefix="$artifacts/"
