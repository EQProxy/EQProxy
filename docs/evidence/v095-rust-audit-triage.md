# Rust Audit — triage of the 6 `high` findings (v0.9.5, AUD-01 tree)

Date: 2026-11-09. Tree: branch `release/v0.9.5`, HEAD `ecda55a`
plus the uncommitted AUD-01 changes (`cache.rs`, `http1.rs`, `metrics.rs`,
`e2e_cache.rs`). No commit, tag or publication was made.

## Scan

```text
$ python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py \
    scan . --format md --out report --fail-on high
exit 1
wrote report/rust-audit-20261109-213413.md
not-executed report: report/not-executed-2026-11-09.md
```

Rust Audit **0.2.9** (`scripts/rust_audit.py`, sha256
`6b0862996455253bf59590d6fbe88aca95feb35aad1ea205a0239c3282c7f38d`),
Python 3.13.13, cargo 1.99.1. Reports are local-only (`/report/` is in
`.git/info/exclude`).

| Severity | Count |
|---|---|
| critical | 0 |
| high | 6 |
| medium | 456 |
| low | 123 |

The gate exits 1 **by rule** under `--fail-on high`. Every high is reviewed
below; none is introduced by AUD-01 and none is in `cache.rs`, `http1.rs` or
`e2e_cache.rs`. This is the final-tree scan, including the second-`304` E2E;
the six locations are identical to the pre-final scan.

## Triage

| # | Rule | Location | Verdict |
|---|---|---|---|
| 1 | R20 | `Cargo.toml:1` (`pprof ~ approx`) | false positive |
| 2 | R21 | `eqproxy-cli/build.rs:14` | false positive (intentional build step) |
| 3 | R21 | `eqproxy-net/build.rs:14` | false positive (intentional build step) |
| 4 | R18c | `lints/eqproxy-lints/src/lib.rs:7` (`rustc_ast`) | false positive (rustc_private) |
| 5 | R18a | `lints/eqproxy-lints/src/lib.rs:8` (`rustc_errors`) | false positive (rustc_private) |
| 6 | R18a | `lints/eqproxy-lints/src/lib.rs:9` (`rustc_lint`) | false positive (rustc_private) |

### R20 — `pprof` is a real crate, not a typosquat of `approx`

`pprof` is declared as an optional dependency behind the profiling-only
feature in `eqproxy-cli/Cargo.toml`:

```toml
[features]
profile = ["dep:pprof", "dep:inferno"]

pprof = { version = "0.15", default-features = false, optional = true }
```

`Cargo.lock` pins `pprof 0.15.0` with registry checksum
`38a01da47675efa7673b032bf8efd8214f1917d89685e07e395ab125ea42b187`.
It is used by the profiler path documented in
`docs/evidence/v092-profiler-inferno.md`; the release image never enables
the `profile` feature. The distance-to-`approx` heuristic does not describe a
dependency substitution: `approx` is not declared in this workspace.

### R21 — the build scripts run a local, debug/test-only certificate fixture

Both `build.rs` files return before the command when `PROFILE=release`
(`eqproxy-cli/build.rs:6-15`, `eqproxy-net/build.rs:6-15`), so the release
artifact does not need `openssl` or generated material. For debug/test builds
they run the repository's `eqproxy-net/build-support/gen-test-certs.sh` with
`sh`; that script creates ephemeral loopback RSA certificates in `OUT_DIR`
and removes its CSRs. It fetches nothing from the network and is not a
hallucinated dependency. The static rule flags any `Command::new`, not a
network or supply-chain call.

### R18c/R18a — `rustc_*` are rustc sysroot crates, not crates.io deps

`lints/eqproxy-lints/src/lib.rs:1` enables `#![feature(rustc_private)]`;
`lints/eqproxy-lints/Cargo.toml` is its own workspace and sets
`package.metadata.rust-analyzer.rustc_private = true`. The crate is loaded by
Dylint through `[workspace.metadata.dylint]` in the root `Cargo.toml`; it is
not a root workspace member, not linked into the `eqproxy` binary, and is not
built by `cargo test --workspace`. `rustc_ast`, `rustc_errors` and
`rustc_lint` are supplied by the compiler toolchain when Dylint builds the
lint, so their absence from `Cargo.toml` is the expected rustc_private
contract.

## Not executed

The optional deeper legs remain **NOT EXECUTED** (see
`report/not-executed-2026-11-09.md`): `cargo --deep` (mutants, llvm-cov,
udeps), `cargo --supply-chain` (audit/deny/vet, guarddog, osv-scanner),
`cargo --miri`, and `scan --online`. `cargo-udeps`, `guarddog` and
`osv-scanner` are not installed; `cargo-mutants`, `cargo-llvm-cov` and
`cargo-miri` are installed but were not run.

## Historical note

`docs/evidence/v085-rust-audit-triage.md` records an earlier scan whose seven
R16/R09 highs were refactored to zero. That result belongs to that tree; this
page triages the current scan and does not reopen or suppress those rules.
