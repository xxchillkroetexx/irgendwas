#!/bin/sh
# Run a bounded exploration against a temporary copy of the checked-in seeds.
#
# FUZZ_SECONDS        exploration time, 1..86400 (default 20)
# FUZZ_ARTIFACTS_DIR  where crashes are kept (default fuzz/artifacts/redirect_paths); a
#                     relative path is taken from the caller's directory
# ASAN_OPTIONS        appended to, not replaced: the quarantine limits below are added
#                     unless the caller already sets that option
set -eu
seconds="${FUZZ_SECONDS:-20}"
case "$seconds" in
    *[!0-9]*|'') echo "FUZZ_SECONDS must be an integer from 1 to 86400" >&2; exit 2 ;;
esac
if ! [ "$seconds" -ge 1 ] 2>/dev/null || ! [ "$seconds" -le 86400 ] 2>/dev/null; then
    echo "FUZZ_SECONDS must be an integer from 1 to 86400" >&2
    exit 2
fi
# Resolve a relative FUZZ_ARTIFACTS_DIR against the caller's directory, before the cd.
artifacts="${FUZZ_ARTIFACTS_DIR:-}"
case "$artifacts" in
    '') ;;
    /*) ;;
    *) artifacts="$PWD/$artifacts" ;;
esac
cd "$(dirname "$0")/.."
artifacts="${artifacts:-$PWD/fuzz/artifacts/redirect_paths}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/corpus"
cp fuzz/corpus/redirect_paths/* "$work/corpus/"
# Print and retain any crash outside the temporary directory so it can be replayed.
mkdir -p "$artifacts"
# Keep AddressSanitizer quarantine bounded on the shared CI runner. Options the caller
# set win: each limit is appended only if ASAN_OPTIONS does not name it yet.
asan="${ASAN_OPTIONS:-}"
case "$asan" in *quarantine_size_mb=*) ;; *) asan="${asan:+$asan:}quarantine_size_mb=32" ;; esac
case "$asan" in *thread_local_quarantine_size_kb=*) ;; *) asan="$asan:thread_local_quarantine_size_kb=256" ;; esac
ASAN_OPTIONS="$asan"
export ASAN_OPTIONS
cargo +nightly-2026-11-01 metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps > /dev/null
cargo +nightly-2026-11-01 fuzz run redirect_paths "$work/corpus" -- \
    -max_total_time="$seconds" -timeout=5 -max_len=2048 \
    -rss_limit_mb=1024 -verbosity=0 -print_final_stats=1 -artifact_prefix="$artifacts/"
