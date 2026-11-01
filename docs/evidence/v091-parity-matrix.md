# v0.9.1-e — parity matrix (casilla 15) evidence

- Date: 2026-11-01
- Scope: classify every row of HAProxy 3.4.6 LTS, nginx, ingress-nginx and
  Envoy API v3 as E1/E2/E3; publish [docs/parity-matrix.md](../parity-matrix.md).
- Result: **120 rows — 11 E1 · 88 E2 · 21 E3 · 0 unclassified**.
- New public decisions: [ADR-0047](../adr/0047-parity-scope-remaining-rows.md)
  for the rows that were still `Designed (scope)`.

## 1. Differential corpus (E1 evidence)

The 23 shared-semantics cases ran unchanged against EQProxy (freshly built
debug binary), HAProxy 3.4.6, the ingress-nginx 1.27.1 data plane driven by a
static config, and Envoy 1.34.2 with a static bootstrap.

```
cargo test -p eqproxy-cli --test differential -- --ignored --nocapture
```

- Exit: **0**; `1 passed; 0 failed; 0 ignored`; wall time **73.71 s**.
- Verdict in the report: **23/23 cases agree** across their applicable
  implementations; **0 documented exclusions**.
- Binary: `/Volumes/Lexar/Cursor/EQProxy/target/debug/eqproxy`, sha256
  `d779bccc682e5a6c422cdea5dd55b2e094d84d2eb21a325f9dcfdab1075f5a40`.
- Reference images:
  - `haproxy:3.4.6` — `sha256:e273915691a8eab3f105bf3e39fd4c29a4ecf334e33421903b5589d25e14db26`
  - `registry.k8s.io/ingress-nginx/controller:v1.13.0` —
    `sha256:dc75a7baec7a3b827a5d7ab0acd10ab507904c7dad692365b3e3b596eca1afd2`
  - `envoyproxy/envoy:v1.34.2` —
    `sha256:daca6a3f353ba289cc786d2162d13d4ec2b16d921c6c3f2fc57ce6f7900ab3d9`
  - `nginx:alpine` backend —
    `sha256:a3554c0d0b48177b1413c26e01f695334428020d9df01243fbae3e11cfd17417`
- Report: [raw/differential-report.md](v091-parity-matrix/raw/differential-report.md)
  (the source was `target/tmp/differential-70638/differential-report.md`).

Cases: `c1-prefix`, `c1-boundary`, `c2-host`, `c2-port`, `c2-near`,
`c3-method`, `c3-near`, `c4-header`, `c4-near`, `c5-first`, `c5-second`,
`c6-redirect`, `c7-direct`, `c8-rewrite`, `c11-regex`, `c11-boundary`,
`h1-xff-append`, `h2-xff-xri-replace`, `h4-xfp`, `h5-set-header`,
`h6-del-header`, `h7-add-header`, `h8-response-marker`.

The run was made before the IPv6 test was added. That change touches only
`eqproxy-net/tests/e2e_tcp.rs` (test target), not the `eqproxy` binary whose
hash is cited; the full workspace suite below compiles and runs it.

## 2. IPv6 (A6)

```
cargo test -p eqproxy-net --test e2e_tcp an_ipv6_listener_proxies_to_an_ipv6_backend -- --nocapture
```

- Exit: **0**; `1 passed; 0 failed; 0 ignored`; `7 filtered out`.
- The test binds `[::1]:0`, dials an IPv6 backend and asserts a byte-exact
  echo plus the v6 family on the listener address. It is the evidence for A6,
  not a dual-stack claim: the reference's `v4v6` wildcard keyword has no
  direct equivalent (documented in the matrix).

## 3. Workspace checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit **0**, finished in 39.67 s |
| `cargo test --workspace` | exit **0** — **36 targets, 698 passed, 0 failed, 2 ignored** |

The two ignored tests are the declared Docker harnesses; they are not counted
as passes. The differential run above is the one that exercises reference
containers.

## 4. Rust Audit

```
python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high
```

- Tool: rust-ai-audit **0.2.7**, engine sha256
  `84b5ba5ba05931e7e318b1ee345ffe32f18cb473c051b33066f5938f0fb79b26`.
- Exit: **1** (`--fail-on high`). Report:
  `report/rust-audit-20261101-052109.md`.
- Counts: **critical 0, high 7, medium 417, low 111**.
- The seven high findings are **false positives of the static heuristics**;
  each was checked against the source:
  - `fuzz/fuzz_targets/config_import.rs:43,55,85,95` — `panic!`/`expect` in a
    **fuzz harness** whose contract is to fail loudly on a converter
    non-determinism; the crate is not the production binary.
  - `eqproxy-net/src/cache.rs:421,559,624` — the scanner sees `lock()` and
    `.await` in the same function, but every guard is inside a block
    (`{ let mut inner = self.lock(); … }`) and is dropped before
    `spawn_blocking(...).await` / `persist_variant(...).await`. No guard is
    held across an await.
- The new IPv6 test adds one **low** finding (`R02`, magic numbers in test
  assertions), not a high one.
- Not executed: `--deep` (mutations/coverage/udeps), `--supply-chain`
  (audit/deny/vet, guarddog, osv-scanner), `--miri`, `--online`; see
  `report/not-executed-2026-11-01.md`.

## 5. Veyra Proof

```
/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard
```

- Decision: **ACCEPTED** — all required profile tools executed without
  blocking findings.
- Exit: **0**; 789.15 s of tools (791.95 s wall), macOS arm64, Rust 1.99.1.
- Evidence: `.veyra/evidence/20261101T042259Z/report.md`.
- OK: rustfmt, cargo check, clippy, cargo test, cargo-audit,
  veyra-anti-theater, nextest, llvm-cov, cargo-deny, cargo-machete,
  veyra-e2e.
- Skipped, not counted as passes: **dylint** (no Dylint configuration or lint
  libraries), **cargo-semver-checks** (no registry baseline for unpublished
  crates).
- Inconclusive, not a pass: **cargo-geiger** (coverage warnings in 4 runs;
  unsafe counts are a lower bound). See `tools/geiger/stderr.log` in the
  evidence directory.

## 6. CodeGraph Rust

Index over the final Rust tree (after the IPv6 test edit; later changes were
Markdown only):

```
codegraph-rust index . --batch-size 16 --stats-json /tmp/cg-index-v091e.json
```

- Exit: **0**; `graph_complete: true`, `vector_index_ready: true`,
  `embedding_status: ready`; **84 files, 81 122 lines, 8 873 nodes,
  15 630 edges**, `index_ms: 1 084 589` (18 min 4.6 s).
- Index log: `target/parity/codegraph.log` (local-only; 83 cached files, 1
  changed).

Queries (advisory only, checked against the source):

```
codegraph-rust agent impact "what breaks if the listener bind or TcpSocket family selection changes"
codegraph-rust agent context "where is the listener bind and the IPv6 family chosen"
```

- Both exit **0**. The context answer located `eqproxy_net::proxy::bind` at
  [proxy.rs:1455](../../eqproxy-net/src/proxy.rs#L1455) and the IPv6 assertion at
  [e2e_tcp.rs:169](../../eqproxy-net/tests/e2e_tcp.rs#L169) — both checked in the
  source. It also correctly says the family comes from the `SocketAddr` in
  `listener.listen`, not from an explicit domain selector.
- The impact answer's direct-dependent list (the CLI plus the test helpers) is
  consistent with the source check; it **could not** run a full reverse
  closure or cycle query (tool budget exhausted), so it is not cited as a
  complete impact analysis.
- CodeGraph is a map, not a witness: no behavioural claim in the matrix rests
  on it; the differential run and the tests are the evidence.

## 7. Limits

- E1 covers **only** the 23 declared cases; it is not a claim that every
  directive of an E1 row is implemented. The matrix names the difference.
- E2 rows have a test and a documented difference, not a differential run.
- E3 rows are decisions with an alternative; they are not implemented.
- This increment changes documentation and one test. It does not change the
  runtime, so no release build or Docker multi-architecture journey was run.
- No external review is implied; boxes 12, 14, 16, 17 and 19 remain open in
  roadmap 0.9.2, and box 18 (external review) in 0.9.3.
