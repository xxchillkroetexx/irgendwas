#!/bin/sh
# Exercise the real wrapper with two independent checkouts and one shared cache.
set -eu
script_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
test_dir="$(mktemp -d)"
first_pid=""
second_pid=""
cleanup() {
    [ -z "$first_pid" ] || kill "$first_pid" 2>/dev/null || :
    [ -z "$second_pid" ] || kill "$second_pid" 2>/dev/null || :
    rm -rf "$test_dir"
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$test_dir/a/scripts" "$test_dir/b/scripts" "$test_dir/cache/target-check"
cp "$script_dir/ci-target-dir.sh" "$test_dir/a/scripts/"
cp "$script_dir/ci-target-dir.sh" "$test_dir/b/scripts/"
dd if=/dev/zero of="$test_dir/cache/target-check/existing" bs=1048576 count=2 2>/dev/null
export CI_CACHE_TEST_DIR="$test_dir" TARGET_CACHE_MAX_MB=1
cache="$test_dir/cache/target-check"

# Hold the first job in its build phase, above the configured threshold.
sh "$test_dir/a/scripts/ci-target-dir.sh" "$cache" sh -ec '
    test -L target
    test -f target/existing
    : > "$CI_CACHE_TEST_DIR/first-started"
    while [ ! -e "$CI_CACHE_TEST_DIR/release" ]; do sleep 0.05; done
    test -f target/existing
' >"$test_dir/first.log" 2>&1 &
first_pid=$!
attempt=0
while [ ! -e "$test_dir/first-started" ]; do
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 200 ] || ! kill -0 "$first_pid" 2>/dev/null; then
        cat "$test_dir/first.log" >&2
        echo "first job did not start" >&2
        exit 1
    fi
    sleep 0.05
done

sh "$test_dir/b/scripts/ci-target-dir.sh" "$cache" sh -ec '
    test -L target
    test -f target/existing
    : > "$CI_CACHE_TEST_DIR/second-started"
' >"$test_dir/second.log" 2>&1 &
second_pid=$!
sleep 1
if [ -e "$test_dir/second-started" ] || ! kill -0 "$second_pid" 2>/dev/null; then
    cat "$test_dir/second.log" >&2
    echo "second job did not wait for the shared cache" >&2
    exit 1
fi
test -f "$cache/existing"
: > "$test_dir/release"
wait "$first_pid"
first_pid=""
wait "$second_pid"
second_pid=""
test -e "$test_dir/second-started"
test -f "$cache/existing"
grep -q 'clean the cache manually' "$test_dir/first.log"

# A failed phase must release the lock for the next job.
if sh "$test_dir/a/scripts/ci-target-dir.sh" "$cache" sh -c 'exit 42'; then
    echo "failed build unexpectedly succeeded" >&2
    exit 1
fi
sh "$test_dir/b/scripts/ci-target-dir.sh" "$cache" sh -c 'test -f target/existing'

# Without the mounted parent directory, run the command with an ordinary target.
sh "$test_dir/a/scripts/ci-target-dir.sh" "$test_dir/unmounted/target" sh -c '
    test ! -e "$CI_CACHE_TEST_DIR/unmounted"
    : > "$CI_CACHE_TEST_DIR/fallback-ran"
'
test -e "$test_dir/fallback-ran"
echo "CI cache coordination tests passed"
