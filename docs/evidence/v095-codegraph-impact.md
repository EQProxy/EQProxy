# v0.9.5 working tree — CodeGraph impact evaluation

Date: 2026-11-14. Branch `release/v0.9.5`, HEAD `ae85cf9` (4 commits ahead of
`origin/release/v0.9.5`), no commit, no push. The baseline has 25 modified
tracked files: **19 code/manifest files** (16 `.rs`, `eqproxy-cli/Cargo.toml`,
`Cargo.lock`, `eqproxy-net/build-support/gen-test-certs.sh`) and 6 docs/meta
files (three `docs/`, `README.md`, `docker/Dockerfile`,
`.github/workflows/release.yml`). Six untracked entries exist alongside them:
the new test-only `eqproxy-config/src/import/security.rs`, the new
`eqproxy-cli/tests/cli_http3_reload_e2e.rs`, ADR-0051, this report and its
`coverage/` directory, and `dist-v095-p04-src.tar.gz`. This record evaluates
CodeGraph against that known, uncommitted change set.

## Index

The baseline 13-query run used the index recorded at `11:08:39Z`. The tree
changed afterwards; the coverage supplement below uses a second index built
over the current tree. The two epochs are not interchangeable.

| Epoch | Used by | files | lines | nodes | edges | chunks | index_ms |
|---|---|---:|---:|---:|---:|---:|---:|
| `11:08:39Z` | baseline 13 queries | 98 | 96 030 | 10 545 | 21 850 | 10 553 | 125 640 |
| `13:59:28Z` | coverage supplement | 98 | 96 719 | 10 647 | 22 050 | 10 655 | 929 817 |

Both indexes report `graph_complete: true` and `vector_index_ready: true`. The
identity is `aa330baf93b96a1ffba64f2a1e4a2143dd357eafbff946a9155fde6229ecb51f`
for both, with serving context 32 768; it is recorded in
`docs/evidence/v095-codegraph-impact/coverage/run-summary.json` (the raw stats
file has no identity field). The fresh stats are saved at
`coverage/index-stats.json` and the log at `coverage/index.log`; the index
command was `codegraph-rust index . --stats-json
/tmp/cg-index-20261114T134127Z.json` (the log reports 8 effective workers).
The fresh index still covers 98 tracked files; the two untracked source/test
files are not in it. `codegraph-identity-check` exits 0 against
`.codegraph/index-identity.json`.

Binary epochs also differ: the baseline used the pre-fix binary
(`aa3497afe928ce1e2bd4ee775f546fdbb54de75803c24ec49bf323a951293bbb`); the
coverage supplement uses the fixed binary
(`8fcdca9b9344e51ebb7cfc4dad6de195d1e32678a4f930bd75ea68be54b996d6`).

## Method

The baseline is thirteen `agent impact` queries over the behavioral surface of
the changed source files (one per symbol; `nginx.rs` contributed four and
`cache.rs` two). The coverage supplement adds five queries for the four source
files that had no query: `ingress.rs` two, `envoy.rs` one, `haproxy.rs` one and
`tls.rs` one. The full touched-symbol inventory is saved at
`docs/evidence/v095-codegraph-impact/touched-symbols.txt`; it has 110 symbol
entries in 13 tracked source files. The 18 queries are a behavioral subset, not
a full symbol pass; the coverage table below states exactly what remains
unqueried.

```bash
codegraph-rust agent impact "<query>" --focus call_chain --format json \
  --timeout-secs 240
```

The 18 answers used below exited 0, tier `Medium`, framework `Rig`, 4–11 tool
calls each. The first coverage batch at `15:00:14` exited 1 with
`.codegraph/db/LOCK is already locked`; those attempts are retained in
`coverage/run.log` and are not counted. The five valid answers are the
post-lock reruns; the Envoy and TLS retries are separate. Configured LLM:
`deepseek-v4.1-flash:1017-cloud`
(`~/.codegraph/config.toml`, context window 60 000). Baseline raw JSON, answers
and stderr are in `docs/evidence/v095-codegraph-impact/raw/`; coverage raw
files are in `coverage/`; the machine-readable citation audit is
`citations.json`; the store probe is `store-probe.txt`; the `rg` ground truth is
`rg-callers.txt`; the baseline per-query summary is `run-summary.json` and the
coverage summary is `coverage/run-summary.json`.

Ground truth for every caller below was fixed with `rg` on this tree, not with
CodeGraph.

## Baseline caller accuracy

| # | Symbol | Changed file | Production callers (`rg`) | Answer's callers | Verdict |
|---|---|---|---|---|---|
| 1 | `healthcheck` | `eqproxy-cli/src/main.rs` | `main` (`:225`) | `main` | correct |
| 2 | `check_listener_addresses` | `eqproxy-config/src/validate.rs` | `check_listeners` (`:601`) | `check_listeners` | correct |
| 3 | `multi_vhost` | `eqproxy-config/src/import/nginx.rs` | `http` (`:464`) | `http` | correct |
| 4 | `report_secondary_listener_knob` | `eqproxy-config/src/import/nginx.rs` | `server`, 9 call sites (`:1817`–`:2084`) | `server`, but "5 edges" | caller correct, count wrong (graph has 9) |
| 5 | `classify_proxy_ssl` | `eqproxy-config/src/import/nginx.rs` | `http_directive` (`:1001`), `server` (`:2351`) | same | correct |
| 6 | `collect_proxy_ssl` | `eqproxy-config/src/import/nginx.rs` | `http` (`:413`), `server` (`:1265`) | same | correct |
| 7 | `store_representation` | `eqproxy-net/src/cache.rs` | `ResponseAssembly::run` (`http1.rs:4183`) | same | correct |
| 8 | `StoredRepresentation` | `eqproxy-net/src/cache.rs` | `store_representation`, `ResponseAssembly::run` | same | correct |
| 9 | `serve_http` | `eqproxy-net/src/http1.rs` | `http2::exchange` (`:316`), `http3::exchange` (`:337`), `proxy::handle_connection` (`:3158`), `ws::serve` (`:119`) | "the indexed graph contains no symbol named `serve_http`" | **wrong** |
| 10 | `spawn_quic` | `eqproxy-net/src/proxy.rs` | `make_entry` (`:1557`), `apply` (`:1785`) | same | correct |
| 11 | `report_with_paths` | `eqproxy-config/src/import/report.rs` | CLI `import` (`main.rs:1238`) | same | correct |
| 12 | `declared_host` | `eqproxy-config/src/schema.rs` | `resolve_with` (`main.rs:2985`) | same | correct |
| 13 | `convert_with_degraded` | `eqproxy-config/src/import/mod.rs` | `convert` (`:900`), CLI `import` (`main.rs:1227`) | same | correct |

**12/13 baseline caller sets are correct; one is a false absence.** The edge
counts and line spans are a separate matter: at least one answer miscounted the
graph (no. 4) and several could not pin exact call lines because the tool
result was truncated. The coverage supplement below adds five symbols in four
previously unqueried files: 3/5 are correct on the first pass, 4/5 after
retrying Envoy, and `tls::client_config` remains wrong. Over distinct symbols,
16/18 caller sets are correct.

### The `serve_http` false absence

The answer states the graph contains no `serve_http` node and refuses to infer
callers. That is false in this pre-fix baseline run. A direct probe of the
rebuilt store finds the node
`nodes:3581c135-ed83-4426-db20-1accddfb6d92`
(`./eqproxy-net/src/http1.rs::serve_http`, async, generics `CR, CW`) and **18
incoming `calls` edges** from eight caller names:

```
exchange (2)            -> http2.rs:316 and http3.rs:337
handle_connection (1)   -> proxy.rs:3158
serve (1)               -> ws.rs:119
5 e2e test functions (14 edges)
```

The production edges are exactly the four `rg` call sites. The failure is in
retrieval: the agent's semantic search returned no node named `serve_http` and
it concluded absence instead of falling back to a name lookup. This is the
highest-fan-in symbol of the change set (it is the HTTP/1.1 handler used by
HTTP/2, HTTP/3, WebSocket and the proxy accept path), so the false negative is
material for the release.

### The `report_secondary_listener_knob` count

The answer says "the graph records as 5 incoming `calls` edges". The store has
**9 edges from `server`**, matching the nine `rg` call sites
(`nginx.rs:1817,1848,1883,1915,1950,1985,2019,2048,2084`). The graph is right;
the answer undercounted. A parity test that trusts the "5" would miss four
listener knobs.

## Coverage supplement

The baseline table covers 13 of the 110 touched-symbol entries in 9 of the 13
tracked source files. This supplement adds the four tracked source files that
had no query:
`ingress.rs` (`annotation`, `global_config`), `envoy.rs` (`listener`),
`haproxy.rs` (`frontend`) and `tls.rs` (`client_config`). All five answers are
saved under `coverage/`; the machine-readable summary is
`coverage/run-summary.json`. The index and binary epochs differ from the
baseline, as stated in the Index section.

| # | Symbol | File | Production caller (`rg`) | Answer's caller | Verdict |
|---|---|---|---|---|---|
| 14 | `annotation` | `eqproxy-config/src/import/ingress.rs` | `convert` (`:183`) | `convert` | correct; exact call line not cited |
| 15 | `global_config` | `eqproxy-config/src/import/ingress.rs` | `convert` (`:317`) | `convert` | correct; exact call line not cited |
| 16 | `listener` | `eqproxy-config/src/import/envoy.rs` | `convert` (`:43`) | first run: none; retries: `convert` | **first run failed with budget**, correct in both retries |
| 17 | `frontend` | `eqproxy-config/src/import/haproxy.rs` | `convert` (`:496`, `:499`) | `convert` | caller correct; depth-1 edge set noisy |
| 18 | `client_config` | `eqproxy-net/src/tls.rs` | `resolve_with` (`eqproxy-cli/src/main.rs:2986`) | primary: none; retry: admits the graph misses `main.rs:2986`, does not name `resolve_with` | **wrong/incomplete** |

First pass: 3/5 correct; after retrying Envoy, 4/5. `tls::client_config`
remains a graph gap: the source call at `main.rs:2986` is inside `resolve_with`
(`fn` at `:2844`), but the reverse-dependency result contains only the two unit
tests in `tls.rs`. The retry cites the exact production line but still does not
attribute it to `resolve_with`. The Envoy first failure is retained in
`coverage/envoy_listener.*`; the successful retries are
`coverage/envoy_listener_retry1.*` and `retry2.*`.

### File and symbol coverage

| File | Changed entries | Queried | Not queried |
|---|---:|---|---|
| `eqproxy-cli/src/main.rs` | 11 | `healthcheck` | `profile`, `Command`, `main`, `rollback`, `probe_admin`, `exchange_admin`, `probe_listeners`, `loopback_for`, `resolve_with`, `resolve_with` |
| `eqproxy-config/src/import/envoy.rs` | 1 | `listener` | — |
| `eqproxy-config/src/import/haproxy.rs` | 1 | `frontend` | — |
| `eqproxy-config/src/import/ingress.rs` | 2 | `annotation`, `global_config` | — |
| `eqproxy-config/src/import/mod.rs` | 11 | `convert_with_degraded` | `report`, `yaml`, `security`, `yaml`, `ListenerTls`, `SniCertIr`, `Rule`, `Rule`, `convert`, `convert` |
| `eqproxy-config/src/import/nginx.rs` | 48 | `multi_vhost`, `report_secondary_listener_knob`, `collect_proxy_ssl`, `classify_proxy_ssl` | `directive`, `first_arg`, `ProxySslDefaults`, `HttpTlsDefaults`, `HttpDefaults`, `http`, `convert`, `downgrade_unfulfilled_proxy_ssl_server_name`, `http`, `http_directive`, `VhostRole`, `VhostRole`, `is_primary`, `primary_listener`, `exact_hosts`, `ciphers_of`, `http_directive`, `server`, `upstream`, `Planned`, `server`, `location`, `Planned`, `resolve_backend`, `location`, `resolve_backend`, `proxy_ssl_toggle`, `proxy_ssl_path`, `has_https_target`, `tls_arg`, `collect_http_tls`, `has_tls_server`, `ClientVerifyMode`, `client_verify_mode`, `client_verify_active`, `reject_optional_client_verify`, `http_client_verify_active`, `http_has_client_ca`, `TlsProtocolRange`, `tls_protocol_range`, `unknown_proxy_ssl`, `proxy_ssl_scope_mapped`, `proxy_ssl_scope_lost`, `stream` |
| `eqproxy-config/src/import/report.rs` | 5 | `report_with_paths` | `emit_toml`, `render`, `declared_degraded_count`, `check_degraded_names` |
| `eqproxy-config/src/schema.rs` | 2 | `declared_host` | `server_entries` |
| `eqproxy-config/src/validate.rs` | 5 | `check_listener_addresses` | `check_listeners`, `addresses_overlap`, `ipv4_endpoint`, `alpn_problems` |
| `eqproxy-net/src/cache.rs` | 7 | `StoredRepresentation`, `store_representation` | `cache_304_invalidations`, `max_entry_bytes`, `settings`, `end_to_end`, `CachedObjectView` |
| `eqproxy-net/src/http1.rs` | 8 | `serve_http` | `CacheFetch`, `StorePlan`, `drop`, `store_plan`, `ResponseAssembly`, `run`, `run` |
| `eqproxy-net/src/proxy.rs` | 4 | `spawn_quic` | `QuicEntry`, `apply`, `apply` |
| `eqproxy-net/src/tls.rs` | 5 | `client_config` | `ServerTls`, `ServerTls`, `fixed_material_fingerprint`, `fixed_material_fingerprint` |

**Totals:** 110 changed-symbol entries, **18 queried (16.4 %), 92 not queried
(83.6 %)**. All **13/13 tracked source files** have at least one query, but
most symbols overall remain unqueried. Three small files are fully covered
(`envoy.rs` 1/1, `haproxy.rs` 1/1, `ingress.rs` 2/2); the largest absolute gap
is `nginx.rs` (4/48). No tracked source file remains entirely unqueried; the
file-level gaps are the untracked test-only `security.rs` and the four test
files below. The 92 unqueried entries are **NOT EXECUTED**, not negative
results: absence from this table is not evidence that a symbol has no impact.

`eqproxy-config/src/import/security.rs` is an untracked, test-only oracle with
8 symbols (`SecurityField`, `table`, `looks_security`, `wildcard`, `matches`,
`field_at`, `field_at_segments`, `field_present`); it is not production code and
was not queried (**NOT EXECUTED**). The four changed test files
(`eqproxy-cli/tests/cli_e2e.rs`, `eqproxy-cli/tests/cli_http3_reload_e2e.rs`,
`eqproxy-config/tests/import_corpus.rs`, `eqproxy-net/tests/e2e_cache.rs`) are
not production impact targets and were **NOT EXECUTED**; their test functions
are not in the 110-entry source inventory.

### Supplement citations

Only `coverage/tls_client_config_retry` cites the exact production call line
(`eqproxy-cli/src/main.rs:2986`, four occurrences), and it does not name
`resolve_with`. The other four answers cite the definition, the caller function
or a range, not the call line: `annotation` (`:183`), `global_config` (`:317`),
`listener` (`:43`) and `frontend` (`:496`, `:499`) are absent from the citation
sets. This repeats the baseline's 0/4 `serve_http` and 0/9
`report_secondary_listener_knob` exact-call-site gap.

## Citation audit

185 citation instances across the 13 answers (169 distinct `file:line`
strings; a location cited by two answers is counted once per answer):

| Property | Count | Share |
|---|---:|---:|
| basename-only (`main.rs:1406`, no path) | 78 | 42.2 % |
| line on a working-tree changed line | 57 | 30.8 % |
| direct call site of the target | 3 | 1.6 % |
| enclosing function is a `rg`-verified caller | 32 | 17.3 % |
| definition of the target | 13 | 7.0 % |
| upstream/helper/test location ("other") | 92 | 49.7 % |
| invalid (out of range or unresolvable) | 6 | 3.2 % |

Each citation was resolved against the tree and the working diff (existence,
line content, diff membership); the caller claims were separately checked
against the `rg` ground truth. A citation outside the diff is not wrong—it is
context.

The "other" bucket is not all noise: many are legitimate upstream entry points
(`main`, `config_command`, `gate`, `resolve_with`) or tests. But it also
contains clearly unrelated locations, for example `eqproxy-net/src/health.rs:100`
cited from `multi_vhost` and `collect_proxy_ssl`, and locations the answers
themselves flag as spurious (`serve_connection` as a caller of `resolve_with`).

Invalid citations found:

- `eqproxy-config/src/validate.rs:7953a90b` — a node id in `file:nodeid` form,
  parsed by the reader as line 7953; the file has 5 148 lines.
- Five basename-only citations could not be resolved uniquely: three in
  `multi_vhost` (`mod.rs:499`, `mod.rs:222`, `mod.rs:419`, ambiguous between
  `eqproxy-config/src/import/mod.rs` and `eqproxy-net/tests/common/mod.rs`),
  one in `declared_host`, and one in `store_representation`.

Truncation is a recurring limit: `serve_http` says a third search could not run
("tool-result budget exhausted"), `spawn_quic` says the graph did not expose
the exact call lines, `multi_vhost` says the dispatcher node was not retrieved,
and `report_secondary_listener_knob` says the exact lines were unavailable.
The answers are honest about it, but the missing lines are exactly the ones a
release note wants.

## What worked

- **Caller sets**: 12 of 13 matched `rg`, including the cross-crate paths
  (`report_with_paths` → CLI `import`, `declared_host` → `resolve_with`,
  `store_representation` → `http1::ResponseAssembly::run`).
- **Definitions and ranges**: the `fn`/`struct` locations it cites as
  definitions exist and are in the right files, even when the citation omits
  the directory.
- **Diff awareness**: 57 citations land on lines that the working tree actually
  changes, which is the useful part for a release review.
- **Self-correction**: `declared_host` and `store_representation` flag spurious
  graph edges (`starts_with_ignore_ascii_case`, `zero_is_none`) instead of
  repeating them.
- **Coverage supplement**: `ingress::annotation`, `ingress::global_config`,
  `envoy::listener` (after retry) and `haproxy::frontend` all name `convert`,
  matching the `rg` call sites. `tls::client_config` correctly exposes the
  opposite problem: the graph records only unit tests and misses the real
  `resolve_with` call at `main.rs:2986`, so that query must not be used as
  evidence that no production caller exists.

## Release / parity material

The change set to carry into the release notes and the parity matrix:

- **`serve_http`** (`http1.rs:4345`) is the shared HTTP/1.1 handler behind
  `http2::exchange`, `http3::exchange`, `ws::serve` and
  `proxy::handle_connection`. The pre-fix baseline answer denied it exists;
  the release must use the source/`rg` caller set, not that answer. The
  coverage supplement did not rerun this symbol on the fixed binary.
- **`spawn_quic`** (`proxy.rs:1595`) has two production paths: startup
  (`bind` → `make_entry`) and live reload (`apply`, `KeptQuic::Adopt`). The
  reload path is the QUIC certificate-adoption change; it is not a one-liner in
  the risk list.
- **`store_representation` / `StoredRepresentation`** (`cache.rs:1443/1467`)
  have a single production consumer, `ResponseAssembly::run` (`http1.rs:4140`),
  the cache write path behind the P0-10 storage rules.
- **`declared_host`** (`schema.rs:1573`) is consumed only by `resolve_with`
  (`main.rs:2985`); it affects TLS SNI resolution, not runtime routing.
- **`convert_with_degraded`** (`import/mod.rs:910`) has two callers: the library
  `convert` wrapper and the CLI `import`; its degraded list drives the CLI exit
  code and the summary line.
- **`report_secondary_listener_knob`** has **9** call sites, not 5; the parity
  tests for secondary-listener knobs must cover all nine.
- **`check_listener_addresses`** is reached through `check_listeners` →
  `validate::check`, i.e. every CLI command that loads a config.
- **Ingress `annotation` / `global_config`** (`ingress.rs:519/844`) have one
  production caller, `ingress::convert` (`:183` and `:317`). The HSTS branch of
  `annotation` now reports `review` instead of `done` when the annotation
  produced no `Strict-Transport-Security` header (for example without
  `enable-hsts: true`); the release note must describe this as an import-report
  classification change, not a proxy runtime change.
- **Envoy `listener`** (`envoy.rs:436`) and **HAProxy `frontend`**
  (`haproxy.rs:1025`) each have one production caller, `convert` (Envoy call at
  `envoy.rs:43`; HAProxy call sites at `haproxy.rs:496/499`); their only
  working-tree delta is the mechanical
  `certs: Vec::new()` field on `ListenerTls`, so no cross-module caller changes
  are expected. The HAProxy depth-1 edge set is noisy; only `convert` is
  confirmed.
- **`tls.rs` comments (inventory entry `client_config`, `tls.rs:595`)**: the
  working-tree delta is comment-only; the five added lines document
  `server_name_for`, and the line-based inventory attributes them to the
  preceding `client_config`. `client_config`'s production caller is `resolve_with`
  (`main.rs:2986`), which the graph does not record. Do not repeat a “no
  production caller” claim from CodeGraph.
- **Coverage boundary:** this record queried 18 of 110 changed-symbol entries
  (16.4 %), all 13 tracked source files but only 4/48 nginx symbols. The
  remaining 92 entries, the four changed test files and the untracked
  test-only `security.rs` are not evaluated here; the release material must
  not imply otherwise.

Parity rows to revisit: nginx multi-vhost listener mapping (ADR-0051), nginx
`proxy_ssl_*` inheritance, listener address-overlap validation, cache
representation storage (P0-10), QUIC certificate adoption on reload, and the
`healthcheck` container probe. This record supplies the symbol/caller facts;
the matrix state still needs its own acceptance test per row.

## Limits

- Baseline: one run per symbol; coverage supplement: one run per new symbol,
  with Envoy retried twice after a budget failure and TLS retried once. The
  model is not temperature-controlled, so caller wording can vary between
  runs. The verdicts above are from the cited run, not a determinism claim.
- The 13 baseline queries and the 5 supplement queries use different index and
  binary epochs (see Index). They are not directly comparable; the supplement
  evaluates the current tree with the fixed binary, while the baseline record
  preserves the pre-fix result.
- Coverage is 18/110 changed-symbol entries (16.4 %); 92 entries remain
  unqueried, including most of `nginx.rs`. The four changed test files and the
  untracked test-only `security.rs` are outside this evaluation. File-level
  coverage (13/13 source files) is not symbol-level coverage.
- The citation parser is a regex over the answer text. Basename-only citations
  were resolved by the conventional file (`main.rs` → `eqproxy-cli/src/main.rs`,
  `schema.rs` → `eqproxy-config/src/schema.rs`, `mod.rs` →
  `eqproxy-config/src/import/mod.rs`, etc.); an ambiguous citation is counted
  as unresolved rather than guessed. Only illegal/out-of-range lines are
  called invalid; unresolved basenames are kept separate.
- The index is structural (fast tier): no LSP, dataflow or diff awareness. It
  does not know which symbols the working tree changed; that mapping was done
  here with `git diff` and `rg`. The fresh index covers 98 tracked files and
  does not include the two untracked source/test files.
- No Veyra Proof or Rust Audit ran for this increment: it adds evidence files
  only, no product source. Those checks are **NOT EXECUTED**. No commit, tag or
  push.
