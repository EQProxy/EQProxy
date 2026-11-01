# v0.9.1-b — Custom error pages (B-H15 · B-E18 · B-N14)

**Objective.** Replace the fixed minimal error bodies with pages the operator
declares, as [ADR-0045](../adr/0045-custom-error-pages.md) decides: one page
set per listener, a body or a file, an optional status override, HAProxy's
`errorfile` shape (`format = "raw"`) and an `intercept` flag that also replaces
a matching **upstream** response. The converter maps the three references'
spellings and says what it cannot express.

## 1. Configuration surface

| Knob | Values | Notes |
|---|---|---|
| `[[listener.*.error_pages]]` | array of pages | `mode = "http"` only; at most `MAX_ERROR_PAGES` (32) |
| `.codes` | 400-599 | One page may cover several; a code belongs to one page per listener |
| `.file` / `.body` | path / literal text | Mutually exclusive. `file` resolves against the TOML's directory; payload bounded to 64 KiB and read at snapshot time |
| `.format` | `body` (default), `raw` | `raw` = a complete HTTP response (status line, headers, body), parsed at load; `content-length` must match, `transfer-encoding` is refused |
| `.status` | 200-599 (not 204/304) | Answers with another status than the one that fired; refused on `raw` (the file owns the status) |
| `.content_type` | media type | Default `text/html; charset=utf-8`; a raw file's header wins |
| `.intercept` | bool (default false) | `true` also replaces a matching **origin** response; refused on `raw` |

`schema_version` stays `1`. The schema, the generated JSON schema
([eqproxy.schema.json](../../eqproxy.schema.json)) and
[eqproxy.toml.example](../../eqproxy.toml.example) document every key.

## 2. Semantics

- **Local failures** (connect failure 502, header timeout 504, no server 503,
  rejected request, rate limit 429) use the page covering the fired code.
  Without a page, the previous minimal error is unchanged.
- **Interception** (`intercept = true`) replaces the origin's status and body
  *after* its head is read and *before* it is forwarded. The origin connection
  is closed, never pooled: its body was not read, so parking it would desync
  the next exchange.
- **Status**: a page without `status` answers with the code that fired; with
  `status` it answers that one (validated 200-599, no 204/304 because neither
  may carry a body).
- **`HEAD`** gets the headers of the page (including `content-length`) and no
  body.
- **Resolution once.** A missing file, an oversized payload (inline `body` or
  a file of either format), a raw file whose
  status line is not in `codes`, a `content-length` mismatch or a chunked raw
  file rejects the snapshot at startup/reload with listener, field and path —
  never a per-request failure.
- **Metrics.** Every served page increments
  `eqproxy_error_pages_served_total`; the periodic log line carries
  `proxy[... error_pages=N ...]`. An intercepted exchange logs the page's
  status; a local failure logs the status that fired (a page's `status`
  override changes the wire response, not that local record).
- **Auth denials keep their protocol headers.** A 401 from Basic auth carries
  `WWW-Authenticate`; replacing it with a generic page would break the
  challenge, so auth denials (and external-auth denials) keep their response
  even when a page covers 401/403. Plugin denials and parsed-request
  rejections do use pages.

## 3. Conversion from the three references

| Reference | Construct | Result |
|---|---|---|
| HAProxy | `errorfile <code> <file>` in `defaults` or a `frontend` | `format = "raw"` page with the file path; `defaults` reaches every generated listener. A backend-scoped `errorfile` is a **review** (EQProxy pages are listener-scoped) |
| nginx | server-level `error_page <codes…> [=NNN] <uri>` | one page per directive, `status = NNN` for `=NNN`; the URI's leading `/` becomes a config-relative `file` (the report says it is not nginx's `root`) |
| nginx | `proxy_intercept_errors on` | `intercept = true` on every page of that server, whatever the directive order |
| nginx | location-scoped `error_page` | **review**: widening it to the listener would change which requests it covers |
| Envoy | `local_reply_config.mappers[]` with `status_code` or an `EQ` `status_code_filter` | one page per mapper; `body.inline_string`, `body.filename` and substitution-free `body_format.text_format` are translated |
| Envoy | status ranges/negations, other filters, `json_format`, `%…%` substitutions, `headers_to_add` | **review** with the exact reason |
| ingress-nginx | `custom-http-errors: "404,503"` | **review** naming the codes and the missing body: the content comes from the default backend, which the manifest does not carry |

## 4. Real-binary journey

Script (maintainer tree, local-only): `scripts/evidence/v091-error-pages/journey.sh`.
Raw output: [raw/](v091-error-pages/raw/). Artifact:
`target/release/eqproxy`, SHA-256 `dc3b1f4f3a4d50e052abf1b6885f3293b1469444de4a622208169cd9d792c161`
(environment recorded in [00-environment.txt](v091-error-pages/raw/00-environment.txt)).

| # | What it proves | Raw |
|---|---|---|
| 1 | A refused backend (502) serves a page loaded from `errors/50x.html`, with its Content-Type and length; `error_pages=1` in the metrics line | [01](v091-error-pages/raw/01-local-file-page.txt) |
| 2 | `status = 503` answers 503 instead of the fired 502; `HEAD` keeps status and `content-length: 25`, body empty; counter 2 | [02](v091-error-pages/raw/02-status-and-head.txt) |
| 3 | A real origin's 503 is replaced by the page twice; the origin logs **two** connections (the intercepted one is never pooled); `error_pages=2` | [03](v091-error-pages/raw/03-intercept.txt) |
| 4 | Without `intercept` (and with a page covering only 502), the origin 503 and its body pass through; `error_pages=0` | [04](v091-error-pages/raw/04-passthrough.txt) |
| 5 | A CRLF `502.http` in HAProxy shape supplies status, Content-Type and body | [05](v091-error-pages/raw/05-raw-file.txt) |
| 6 | `config validate` refuses TCP + pages, duplicate codes and `raw` + `intercept`, each with field, cause and example, exit 1 | [06a](v091-error-pages/raw/06a-reject-tcp.txt) · [06b](v091-error-pages/raw/06b-reject-duplicate.txt) · [06c](v091-error-pages/raw/06c-reject-raw-intercept.txt) |
| 7 | A missing page file fails `run` before any listener: `could not read …`, exit 1 | [07](v091-error-pages/raw/07-missing-file.txt) |
| 8 | `config import` maps HAProxy `errorfile`, nginx `error_page` + proxy_intercept_errors and Envoy local_reply_config; each generated file passes `config validate` | [08-haproxy](v091-error-pages/raw/08-haproxy-import.txt) · [08-nginx](v091-error-pages/raw/08-nginx-import.txt) · [08-envoy](v091-error-pages/raw/08-envoy-import.txt) |

The journey fails (`set -euo pipefail`) on every invariant above; it exited 0
on 2026-11-01.

## 5. Tests

| Level | Command | Result |
|---|---|---|
| Unit, config | `cargo test -p eqproxy-config` | 103 lib + 5 + 4 + 0 = **112 passed**, 0 failed |
| Integration, dataplane | `cargo test -p eqproxy-net --test e2e_error_pages` | **7 passed**, 0 failed (5 error-page cases + the 2 shared `common` guards) |
| Full workspace | `cargo test --workspace` | **686 passed, 0 failed, 2 ignored** (36 test targets, 2026-11-01) |
| Format / lint / build | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --release` | all exit 0 |

Named tests (the ones a reader can trace to behaviour):
`eqproxy-config::import::haproxy::tests::errorfile_becomes_a_raw_error_page_in_defaults_and_frontends`,
`eqproxy-config::import::nginx::tests::error_page_and_intercept_errors_become_a_listener_page`,
`eqproxy-config::import::envoy::tests::local_reply_config_mappers_become_error_pages`,
`eqproxy-config::import::ingress::tests::custom_http_errors_names_the_codes_and_the_missing_body`,
`eqproxy-config::validate::tests::error_pages_are_bounded_and_their_contradictions_are_refused`,
`eqproxy-config::import::yaml::tests::double_quoted_escapes_are_resolved_and_single_quotes_are_not`,
`eqproxy-config::error_page::tests::a_complete_response_is_parsed_into_status_type_and_body`,
`e2e_error_pages::a_local_connect_failure_serves_the_configured_page`,
`e2e_error_pages::a_page_can_override_the_status_and_head_omits_the_body`,
`e2e_error_pages::intercept_replaces_an_origin_error_and_drops_its_connection`,
`e2e_error_pages::without_intercept_the_origin_error_passes_through`,
`e2e_error_pages::no_page_keeps_the_builtin_minimal_error`,
`cli_e2e::run_refuses_a_raw_error_page_over_the_file_bound`.

The E2E cases build the resolved page set directly (the same values the CLI
loads from TOML); the file/`raw` half is covered by the config tests and by
journey steps 5–8 with the release binary. No mock is used for the origin or
the client: both are real sockets.

## 6. Audits of this increment

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit 0 (2026-11-01 00:49Z, over the final Rust tree); OK: rustfmt, check, clippy, test, nextest, llvm-cov, audit, deny, machete, e2e, anti-theater. Skipped: dylint (no config), semver-checks (no baseline). Inconclusive: cargo-geiger coverage warnings (lower-bound unsafe counts, not a pass) | `.veyra/evidence/20261101T004948Z/report.md` |
| Rust Audit | `python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit 1 by rule: 0 critical / 7 high / 413 medium / 110 low. The seven highs are the same locations triaged in [v085-rust-audit-triage](v085-rust-audit-triage.md) (3 R16 cache false positives, 4 R09 fuzz-harness panics); this increment adds none | `report/rust-audit-20261101-015328.md` (local-only) + `report/not-executed-2026-11-01.md` |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091-errors-final2.json`; `agent impact "what breaks if error page resolution or interception changes"`; `agent context "where are locally generated errors answered…"` | index **exit 0**, `graph_complete: true`, `vector_index_ready: true`: 84 files, 80 289 lines, 8 791 nodes, 15 506 edges, 228 562 ms (the earlier full build took 1 002 761 ms). Claims checked in source: `page_for` only at [http1.rs:1165](../../eqproxy-net/src/http1.rs#L1165) and [:1213](../../eqproxy-net/src/http1.rs#L1213), `intercept_for` only at [:3402](../../eqproxy-net/src/http1.rs#L3402), h2 calls `serve_http` at [http2.rs:276](../../eqproxy-net/src/http2.rs#L276), loader at [main.rs:3072](../../eqproxy-cli/src/main.rs#L3072). The tool could not confirm h3's local-error path; the h3 finding stays a pointer, not evidence | `/tmp/cg-v091-errors-final2.json` |
| Hindsight | `hindsight-note --title "v0.9.1-b custom error pages (B-H15/B-E18/B-N14)" --project EQProxy --tags error-pages,v091,http` | stored (`status: completed`), 5 memory ids; first `45e47843-eba9-4761-8d94-1c40062ee261`; re-read from bank `eqproxy` on 2026-11-01 | bank `eqproxy`, `http://127.0.0.1:8888/mcp/eqproxy/` |

The graph was rebuilt after the last Rust edit (`/tmp/cg-v091-errors-final2.json`,
2026-11-01 00:57Z) — the CLI bound fix and its test — and confirms the counts
above: 84 files, 80 289 lines, 8 791 nodes, 15 506 edges, `graph_complete:
true`, `vector_index_ready: true`. A cached pass at 00:45Z, before the bound fix,
had returned the previous 80 249 / 8 788 / 15 499; both were checked against
the tree, not reused as a witness of behaviour.

## 7. Limits

- **Scope.** Pages are per listener; nginx location scope and Envoy per-route
  mappers are reviews, not translations. ingress-nginx `custom-http-errors`
  names its codes but not the body (it lives in the default backend).
- **Bodies are literal.** No `%RESPONSE_CODE%`/nginx variable substitution;
  `json_format` is a review.
- **Auth 401/403 keep their challenge headers** (see §2); a page for those codes
  applies to locally generated rejections, not to the auth gate.
- **One dataplane, three entry paths.** HTTP/1.1 and HTTP/2 streams share
  `serve_http`, so the feature is implemented once; protocol-level h2
  refusals (CONNECT/501, malformed frames) happen before that call and do not
  consult pages. QUIC/HTTP-3 is not exercised by this increment's E2E, nor
  are intercepted `HEAD` or `1xx` edge cases.
- **Docker and other architectures** are not re-run for this increment; the
  published image is unprivileged and unchanged by this feature.
- The generated TOML may point at page files that exist on the migrated host
  but not on the machine running `config validate`; that is the same contract
  as imported certificate paths and the journey creates the files to prove the
  positive path.
