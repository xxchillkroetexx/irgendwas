#!/bin/sh
# Installs the prebuilt cargo-audit release binary (RustSec) for the CI advisory check.
# The release publishes no checksums, so the sha256 of each archive is pinned here; a
# version bump means recomputing them:
#   curl -fsSL <archive URL> | sha256sum
#   CARGO_BIN  target dir, default $CARGO_HOME/bin or ~/.cargo/bin
set -eu

version="0.22.2"
dest="${CARGO_BIN:-${CARGO_HOME:-$HOME/.cargo}/bin}"
case "$(uname -m)" in
    x86_64)
        target="x86_64-unknown-linux-musl"
        sha256="7fb9497f8594b389e5fce5ef9b92db08432996895b2e0c5a0167a69ed445c428"
        ;;
    aarch64)
        target="aarch64-unknown-linux-gnu"
        sha256="c6603814ddaa45e51263dafd31c0ac98808f688d26f7395804f9670b0fd599dd"
        ;;
    *)
        echo "no pinned cargo-audit archive for $(uname -m)" >&2
        exit 1
        ;;
esac
name="cargo-audit-${target}-v${version}"
url="https://github.com/rustsec/rustsec/releases/download/cargo-audit/v${version}/${name}.tgz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
curl --proto '=https' --tlsv1.2 -fsSL -o "${name}.tgz" "$url"
echo "${sha256}  ${name}.tgz" | sha256sum -c
mkdir -p "$dest"
tar -xzf "${name}.tgz" --strip-components=1 -C "$dest" "${name}/cargo-audit"
"$dest/cargo-audit" --version
