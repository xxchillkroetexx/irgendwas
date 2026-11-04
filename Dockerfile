# Production image for the Rust/Leptos app.
#   podman build --format docker -t irgendwas .
#   --build-arg CARGO_BUILD_JOBS=4 limits parallel rustc jobs (default: all CPUs).
# `--format docker` keeps the HEALTHCHECK (OCI images drop it); compose.yml defines the
# same healthcheck, so the flag only matters for plain `podman run`.

ARG RUST_VERSION=1.99.1

# ---- build: cargo-leptos release build (server binary + wasm/JS/CSS site) -------------
FROM docker.io/library/rust:${RUST_VERSION}-slim-trixie AS builder

ARG CARGO_LEPTOS_VERSION=0.3.11
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Prebuilt cargo-leptos from the GitHub release, checked against its published sha256.
COPY scripts/install-cargo-leptos.sh /tmp/install-cargo-leptos.sh
RUN CARGO_LEPTOS_VERSION="${CARGO_LEPTOS_VERSION}" sh /tmp/install-cargo-leptos.sh

WORKDIR /build
# rust-toolchain.toml pins the toolchain and the wasm target; install them up front.
COPY rust-toolchain.toml ./
RUN rustup toolchain install && rustup target list --installed

COPY . .
ARG CARGO_BUILD_JOBS=
# Cache mounts keep the registry and target dir between builds (buildah/BuildKit); the
# artifacts are copied out because cache mounts are not part of the image.
# Normalize permissions: source assets may be 0640 (checkout with umask 027), but
# the non-root runtime must be able to read them regardless of builder ownership.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    set -eux; \
    if [ -z "${CARGO_BUILD_JOBS}" ]; then unset CARGO_BUILD_JOBS; fi; \
    cargo leptos build --release; \
    mkdir -p /out; \
    cp target/release/server /out/server; \
    cp -r target/site /out/site; \
    chmod -R a+rX /out

# ---- runtime: distroless, non-root, glibc + CA certificates, no shell ---------------
FROM gcr.io/distroless/cc-debian13:nonroot

WORKDIR /app
COPY --from=builder /out/server /app/server
COPY --from=builder /out/site /app/site

# Leptos reads these instead of Cargo.toml at runtime. Migrations, locales and the
# Leptos route list are compiled into the binary.
ENV LEPTOS_OUTPUT_NAME=irgendwas \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    LEPTOS_RELOAD_PORT=3001 \
    LEPTOS_ENV=PROD \
    RUST_LOG=info

USER nonroot
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD ["/app/server", "--healthcheck"]
ENTRYPOINT ["/app/server"]
