#!/bin/sh
# CI: makes rust/target a symlink to a persistent directory on the runner host (see the
# header of .forgejo/workflows/rust.yml). Cargo locks the target dir, so two runs of the
# same job wait for each other instead of corrupting it. Registry crates are reused by
# version; workspace crates rebuild because the checkout gives their sources new mtimes.
#   $1                   the directory (inside the mounted volume)
#   TARGET_CACHE_MAX_MB  wipe it above this size (default 20000); it grows with every
#                        dependency or toolchain update
set -eu
cd "$(dirname "$0")/.."

dir="$1"
max="${TARGET_CACHE_MAX_MB:-20000}"
if [ ! -d "$(dirname "$dir")" ]; then
    echo "$(dirname "$dir") is not mounted: building without a persistent target dir" >&2
    exit 0
fi
mkdir -p "$dir"
size="$(du -sm "$dir" | cut -f1)"
echo "persistent target dir $dir: ${size} MB"
if [ "$size" -gt "$max" ]; then
    echo "above ${max} MB: starting over"
    rm -rf "$dir"
    mkdir -p "$dir"
fi
rm -rf target
ln -s "$dir" target
