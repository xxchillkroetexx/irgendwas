# Redirect-path fuzzing pilot

This is a small coverage-guided `cargo-fuzz`/libFuzzer pilot for the production
`app/src/paths.rs` module. It complements the existing deterministic tests and does
not claim to establish application security. The target compiles the production
module directly, avoiding the full Leptos build. The checked-in seeds are synthetic;
never place credentials, real customer data, or production logs in the corpus.

The target mutates arbitrary bytes (decoded lossily as UTF-8) and tests each input
both as-is and as a slash-prefixed path. It checks that accepted redirect paths are
returned unchanged and resolve to the same origin via the WHATWG `url` parser;
`safe_next` remains a subset of `safe_return_path`, invalid values fall back to the
dashboard, and encoding a query value round-trips without creating extra parameters.
It also exercises the 512-byte limit and boundary characters through the seed corpus.
URL parser checks are a useful independent oracle, but cannot replace a test of the
actual HTTP/browser consumer. For example dot segments can normalize after validation;
the target deliberately does not claim that accepted paths avoid normalized `/auth`
or `/api` paths. A concrete issue there needs a separate HTTP/browser regression.

## Setup and bounded smoke run

Install a C/C++ toolchain (`build-essential` on Debian), Rust via rustup, and the
pinned tools in an isolated environment:

```sh
rustup toolchain install nightly-2026-11-01 --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
CARGO_BUILD_JOBS=1 sh scripts/fuzz-smoke.sh
```

The script copies the tracked seeds to a temporary corpus and explores for 20 seconds
by default. It caps input length, individual test duration and RSS, bounds AddressSanitizer quarantine memory, uses one build job
when the caller sets `CARGO_BUILD_JOBS=1`, and does not rewrite the tracked corpus.
Set `FUZZ_SECONDS=60` for a longer bounded run. `fuzz/Cargo.lock` pins the target's
Rust dependencies; the normal stable toolchain remains unchanged.

Crashes are kept under the ignored `fuzz/artifacts/redirect_paths/` directory (or
`FUZZ_ARTIFACTS_DIR`). Preserve a failing synthetic input and replay it with:

```sh
cargo +nightly-2026-11-01 fuzz run redirect_paths path/to/crash -- -runs=1
```

To check the failure-and-replay workflow, use a disposable checkout with its own
`fuzz/target`: temporarily add `assert_ne!(candidate, "/")` inside the target's loop,
run the smoke script, and replay the generated crash with the command above. Both runs
should fail on the deliberately introduced assertion; discard the checkout afterwards.
Never share a target directory between this experiment and a normal checkout.

Minimize with `cargo +nightly-2026-11-01 fuzz tmin redirect_paths path/to/crash`.
Investigate the cause, add a focused deterministic test to the existing suite, and
only commit a reviewed reproducer that contains no sensitive data. A fuzzer finding
is not automatically proof of an exploitable application issue.

`.forgejo/workflows/fuzz.yml` runs the bounded pilot on pushes and PRs affecting this
target, its production module or workflow. It uploads failure artifacts; it does not
replace the normal checks, broader browser coverage, longer fuzz campaigns or a
scheduled job. Repository maintainers own reviewing crashes and updating the pinned
tools and synthetic corpus. Installation and compile time count toward the job timeout, not the
20-second fuzz budget. CI cost depends on the runner and network.
