# Redirect-path fuzzing pilot

This is a small coverage-guided `cargo-fuzz`/libFuzzer pilot for the production
`app/src/paths.rs` module. It complements the existing deterministic tests
(`server/tests/it/paths.rs`, `server/tests/it/auth_unit.rs`) and does not claim to
establish application security. The target compiles the production module directly,
avoiding the full Leptos build. The checked-in seeds are synthetic; never place
credentials, real customer data, or production logs in the corpus.

The target mutates arbitrary bytes (decoded lossily as UTF-8) and tests each input
both as-is and as a slash-prefixed path. For every accepted input it resolves the
returned path with the WHATWG `url` parser, as a browser resolves a `Location` header,
and checks the resulting URL rather than the raw string:

- it stays on the same origin (catches a regression in the `//` and `/\` rules);
- for `safe_return_path`, its path is neither `/api` nor under `/api/`;
- for `safe_next`, its path is additionally neither `/auth` nor under `/auth/`.

So inputs the parser normalizes (dot segments such as `/x/../auth/login`, also
percent-encoded as `%2e`) are checked by where they land; the `normalized` and
`normalized-api` seeds start the exploration there. It also checks that encoding an
arbitrary value for `?next=` round-trips as a single query field. The deterministic
tests cover the exact accepted and rejected values; the target does not repeat them.

The `url` crate is an independent oracle, but not the actual consumer: browsers and
the server's router may differ in details (for example `%61pi` is not decoded by
either, so it is not flagged). A concrete finding there needs an HTTP or browser
regression test.

## Setup and bounded smoke run

Install a C/C++ toolchain (`build-essential` on Debian), Rust via rustup, and the
pinned tools in an isolated environment:

```sh
rustup toolchain install nightly-2026-11-01 --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
sh scripts/fuzz-smoke.sh
```

The script copies the tracked seeds to a temporary corpus and explores for 20 seconds
by default; set `FUZZ_SECONDS=60` for a longer bounded run. It caps input length,
individual test duration and RSS, and does not rewrite the tracked corpus. It appends
AddressSanitizer quarantine limits to `ASAN_OPTIONS` (`quarantine_size_mb=32`,
`thread_local_quarantine_size_kb=256`) unless the caller already sets them; other
options in `ASAN_OPTIONS` are kept. `fuzz/Cargo.lock` pins the target's Rust
dependencies (the script checks it with `--locked`); the normal stable toolchain
remains unchanged.

Crashes are kept under the ignored `fuzz/artifacts/redirect_paths/` directory, or
`FUZZ_ARTIFACTS_DIR` (a relative path is taken from the directory the script is run
from). Preserve a failing synthetic input and replay it with:

```sh
cargo +nightly-2026-11-01 fuzz run redirect_paths path/to/crash -- -runs=1
```

To verify that the target can fail, use a disposable checkout with its own
`fuzz/target`: in its copied `app/src/paths.rs`, remove the dot-segment check in
`safe_return_path` (or the `/api` or `/auth` rule), run the smoke script, and replay the
generated crash with the command above. Both runs must fail with "accepted ..., which
resolves to ...". Discard the checkout afterwards. Never change the production checkout
or share a target directory between this experiment and a normal checkout.

Minimize with `cargo +nightly-2026-11-01 fuzz tmin redirect_paths path/to/crash`.
Investigate the cause, add a focused deterministic test to the existing suite, and
only commit a reviewed reproducer that contains no sensitive data. A fuzzer finding
is not automatically proof of an exploitable application issue.

## CI

`.forgejo/workflows/fuzz.yml` runs the bounded pilot on pushes and PRs affecting this
target, its production module or the workflow, and uploads failure artifacts. It
installs `cargo-fuzz` once per version into the runner's named volume
`irgendwas-ci-cache` (`/ci-cache/tools/cargo-fuzz-<version>`, under a lock) and reuses
it afterwards. The nightly toolchain and the fuzz build are not cached: the runner's
disk is small, and they are rebuilt within the job's 20-minute timeout. It does not
replace the normal checks, broader browser coverage, longer fuzz campaigns or a
scheduled job. Repository maintainers own reviewing crashes and updating the pinned
tools and synthetic corpus.
