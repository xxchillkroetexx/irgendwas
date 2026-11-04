#!/bin/sh
# Installs the prebuilt cargo-leptos release binary from GitHub, checked against the
# published sha256. Used by the Dockerfile and the Forgejo workflow.
#   CARGO_LEPTOS_VERSION  default 0.3.11 (keep in sync with README)
#   CARGO_BIN             target dir, default $CARGO_HOME/bin or ~/.cargo/bin
set -eu

version="${CARGO_LEPTOS_VERSION:-0.3.11}"
dest="${CARGO_BIN:-${CARGO_HOME:-$HOME/.cargo}/bin}"
arch="$(uname -m)"
pkg="cargo-leptos-${arch}-unknown-linux-gnu.tar.gz"
base="https://github.com/leptos-rs/cargo-leptos/releases/download/v${version}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
curl --proto '=https' --tlsv1.2 -fsSLO "${base}/${pkg}"
curl --proto '=https' --tlsv1.2 -fsSLO "${base}/${pkg}.sha256"
sha256sum -c "${pkg}.sha256"
mkdir -p "$dest"
tar -xzf "$pkg" --strip-components=1 -C "$dest" "cargo-leptos-${arch}-unknown-linux-gnu/cargo-leptos"
"$dest/cargo-leptos" --version
