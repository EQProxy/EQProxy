# v0.9.1-c — TLS version range and `errorloc` (B-I16 · B-N16 · B-H16)

**Objective.** Close the remainder of the 0.9.1 box «TLS avanzado y brechas
menores» as [ADR-0046](../adr/0046-tls-version-range-and-minor-gaps.md)
decides: a real TLS **upper** bound (`tls.max_version`) that the handshake
enforces, HAProxy `errorloc` as a first-class redirect error page, conversions
from the three references, and an explicit E2/E3 table for the rest of B-H16
instead of an open-ended «resto».

## 1. Configuration surface

| Knob | Values | Notes |
|---|---|---|
| `[listener.*.tls] max_version` | `"1.2"`, `"1.3"` | Optional; absent keeps TLS 1.3. Must not be lower than `min_version`. Part of the TLS material fingerprint |
| `[[listener.*.error_pages]] location` | `/path` or `http(s)://…` | Mutually exclusive with `file` and `body`; `format = "raw"` is refused; placeholders (`$host`, `$uri`) are refused |
| `.status` with `location` | 300-399 except 304 | Defaults to 302; any other 3xx overrides it |
| `.intercept` with `location` | bool | Allowed: a matching origin response becomes the redirect |

`schema_version` stays `1`. The JSON schema
([eqproxy.schema.json](../../eqproxy.schema.json)) and
[eqproxy.toml.example](../../eqproxy.toml.example) document both keys.

## 2. Semantics

- **The cap is a handshake fact, not a preference.** The listener builds
  rustls with `builder_with_protocol_versions(protocol_versions(min, max))`, so
  a TLS 1.3-only client receives `protocol_version` (alert 70) and no
  certificate; TLS 1.2 still completes and carries the proxied request.
- **A live QUIC endpoint freezes the range.** `max_version` joins the material
  fingerprint (`fixed_material_fingerprint`), so a reload that changes it is
  rejected on a running HTTP/3 listener, exactly like ALPN or the client CA
  (ADR-0025's contract). Without QUIC, the reload swaps the acceptor.
- **Validation refuses impossible ranges**: `max_version < min_version` has no
  version to negotiate, and `quic = true` with `max_version = "1.2"` cannot
  work because QUIC is TLS 1.3. Both errors name the field, the cause and the
  fix.
- **`errorloc` is a bodyless 3xx**: `Location` plus `content-length: 0`, then
  the connection closes like every other locally generated error. `HEAD`
  cannot differ (there is no body either way). With `intercept = true` the
  origin's matching status is replaced and its connection is dropped, never
  pooled.
- **The location is static.** It is validated at load (scheme/path and
  control-free) and written verbatim; a placeholder would put an unresolved
  `$host` in a client header, so it is refused instead.
- **Last declaration wins per code.** `upsert_error_page` replaces a page for a
  code already declared in the same HAProxy section, so `errorfile` followed by
  `errorloc` for the same status does not produce the duplicate that validation
  would reject.
- **Metrics/logs** are unchanged: a redirect page is a page, counted in
  `eqproxy_error_pages_served_total` and `error_pages=N`.

## 3. Conversion from the three references

| Reference | Construct | Result |
|---|---|---|
| nginx | `ssl_protocols TLSv1.2` | `min_version = "1.2"` + `max_version = "1.2"` |
| nginx | `ssl_protocols TLSv1.2 TLSv1.3` | `min_version = "1.2"`, no cap (1.3 is the default upper bound) |
| nginx | `ssl_protocols` naming 1.0/1.1 | **review**: those versions do not exist in EQProxy |
| HAProxy | `bind … ssl-min-ver TLSv1.2 ssl-max-ver TLSv1.2` | both fields on the listener TLS |
| HAProxy | `errorloc <code> <url>` | page with `location` and `status = 302` |
| HAProxy | `errorloc302` / `errorloc303` | the same, with `status = 303` for the latter |
| HAProxy | `errorloc` in `defaults` / `frontend`/`listen` | reaches every generated listener / that listener; a `backend`-scoped one is a review (pages are listener-scoped) |
| Envoy | `common_tls_context.tls_params.tls_{minimum,maximum}_protocol_version` | `min_version` / `max_version`; other values are a review |
| ingress-nginx | ConfigMap `ssl-protocols`, `ssl-ciphers`, `ssl-dhparam` | **review**: controller-wide and no certificate file in the generated listener; the report names the field |

The conversion reports B-N16's remaining knobs as **review/E2**, never silently
applied: `ssl_ciphers`, `ssl_prefer_server_ciphers`, HAProxy
`ciphers`/`ciphersuites`/`curves`, Envoy `cipher_suites`, OCSP stapling,
`ssl_dhparam`, `ssl_session_cache`/`_timeout`/`_tickets`. ADR-0046 §2 records
why (rustls negotiates the modern policy; the project bans TLS 1.0/1.1; OCSP is
an unauthenticated background fetch; rustls owns a bounded resumption cache).

## 4. The rest of B-H16

| HAProxy directive | Outcome | Alternative |
|---|---|---|
| `maxconn` per server | **Done** (0.8.5) | `servers[].maxconn` |
| `errorloc`/`errorloc302`/`errorloc303` | **Done** (0.9.1-c) | `error_pages[].location` |
| `monitor-uri` | **E2** | a `path` rule with `direct_response = 200` |
| `redir` | **E2** | a `redirect` rule |
| `http-reuse` | **E2** | the documented pool policy (A16, ADR-0015) |
| `option httpclose`/`http-server-close`/`http-keep-alive` | **E2** | the pool's keep-alive policy |
| `backlog` | **E3** | the operating system's listen backlog |
| `peer`/`peers` | **E3** | each instance keeps its own view; no state sync |
| `nbthread`, `daemon`, `pidfile`, `chroot`, `ulimit-n`, `tune.*` | **E3** | tokio runtime + systemd; `eqproxy doctor` checks FDs |
| `lua-load`, `filter spoe`, `program`, `mailers`, `userlist` | **E3** | WASM plugins for request filters; no Lua/SPOE |
| `h1-case-adjust-*` | **E3** | strict HTTP/1.1 framing is deliberate (SPEC §7.6) |

## 5. Real-binary journey

Script (maintainer tree, local-only): `scripts/evidence/v091-tls-gaps/journey.sh`.
Raw output: [raw/](v091-tls-gaps/raw/). Artifact: `target/release/eqproxy`,
SHA-256 `8c334c3821c9a659e8f8cfac7da9c2593a2c5f665acde1df8081d234c3d815cf`
(environment in [00-environment.txt](v091-tls-gaps/raw/00-environment.txt)).

| # | What it proves | Raw |
|---|---|---|
| 1 | `max_version = "1.2"`: a TLS 1.3-only `openssl s_client` is refused (alert 70, `Cipher is (NONE)`), TLS 1.2 negotiates and a real proxied `GET` returns `200 ok` | [01](v091-tls-gaps/raw/01-max-version-12.txt) |
| 2 | An explicit `max_version = "1.3"`: TLS 1.3 negotiates (`AEAD-CHACHA20-POLY1305-SHA256`) and the proxied `GET` returns `200 ok` | [02](v091-tls-gaps/raw/02-max-version-13.txt) |
| 3 | `config validate` refuses `min 1.3` + `max 1.2` ("no TLS version could be negotiated") and `quic = true` + `max 1.2` ("needs TLS 1.3"), each exit 1 with the field and example | [03](v091-tls-gaps/raw/03-validation.txt) |
| 4 | A refused backend (502) is answered by `errorloc` with `302`, the configured `Location` and `content-length: 0` | [04](v091-tls-gaps/raw/04-errorloc-local.txt) |
| 5 | `status = 303` + `intercept = true` replaces a real origin 503 with `303 see other` and `Location: /status`; a `/` 200 passes unchanged; the origin log shows the `/fail` request | [05](v091-tls-gaps/raw/05-errorloc303-intercept.txt) |
| 6 | HAProxy `ssl-min-ver`/`ssl-max-ver` + `errorloc`, nginx `ssl_protocols TLSv1.2` and Envoy `tls_params` convert to `max_version = "1.2"` / `location`; each generated TOML passes `config validate` (exit 0) | [06-haproxy](v091-tls-gaps/raw/06-haproxy-import.txt) · [06-nginx](v091-tls-gaps/raw/06-nginx-import.txt) · [06-envoy](v091-tls-gaps/raw/06-envoy-import.txt) |

The journey runs with `set -euo pipefail` and asserts every invariant above;
it exited 0 on 2026-11-01. The origin and the clients are real sockets; the
release binary is the one whose hash is recorded above.

## 6. Tests

| Level | Command | Result |
|---|---|---|
| Unit, config | `cargo test -p eqproxy-config` | **107 lib** + corpus/schema targets, all 0 failed (see the workspace run) |
| Integration, dataplane | `cargo test -p eqproxy-net --test e2e_error_pages` | **9 passed**, 0 failed (two new `errorloc` cases) |
| Integration, CLI + real TLS | `cargo test -p eqproxy-cli --test cli_e2e` | **81 passed**, 0 failed |
| Full workspace | `cargo test --workspace` | **696 passed, 0 failed, 2 ignored**, 33 test targets (2026-11-01) |
| Format / lint / build | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --release` | all exit 0 |

Named tests a reader can trace to the behaviour:

- `eqproxy-config::validate::tests::tls_max_version_must_not_be_lower_than_min_and_quic_needs_13`
- `eqproxy-config::validate::tests::error_page_location_is_static_and_3xx_only`
- `eqproxy-config::import::haproxy::tests::errorloc_and_tls_version_flags_become_real_configuration`
- `eqproxy-config::import::nginx::tests::ssl_protocols_becomes_a_min_max_version_range`
- `eqproxy-net::tls::tests::protocol_range_caps_tls12_and_keeps_tls13_by_default`
- `eqproxy-net::tls::tests::a_live_acceptor_does_not_adopt_a_changed_version_range`
- `e2e_error_pages::errorloc_answers_a_local_failure_with_a_bodyless_redirect`
- `e2e_error_pages::errorloc_intercepts_an_origin_error_and_keeps_its_connection_out_of_the_pool`
- `cli_e2e::run_with_tls_max_version_1_2_refuses_a_tls13_client`
- `cli_e2e::run_with_errorloc_answers_a_local_failure_with_a_redirect`

The Envoy `tls_params` version mapping is proven by journey step 6 with the
real binary (import → generated TOML → `config validate`), which is the
stronger level. Review follow-up:
`envoy::tests::tls_params_cipher_suites_are_reported_not_silently_ignored`
now also asserts that `cipher_suites` is named as a review instead of being
dropped. No mock is used anywhere in the E2E paths.

## 7. Audits of this increment

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit 0 (2026-11-01, over the final Rust tree): rustfmt, cargo check, clippy, test, nextest, llvm-cov, audit, deny, machete, veyra-e2e and anti-theater all OK. `cargo-geiger` **INCONCLUSIVE**: its coverage warnings mean the unsafe counts are a lower bound, not a pass. Skipped: dylint (no configuration) and semver-checks (no baseline) | `.veyra/evidence/20261101T015109Z/report.md` |
| Rust Audit | `python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit 1 by rule: **0 critical / 7 high / 417 medium / 110 low**. The seven highs are the same locations triaged in [v085-rust-audit-triage](v085-rust-audit-triage.md) (3 R16 cache false positives, 4 R09 fuzz-harness panics); this increment adds none | `report/rust-audit-20261101-025148.md` (local-only) + `report/not-executed-2026-11-01.md` |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091-tls.json`; `agent impact "what breaks if the TLS version range or errorloc redirects change"`; `agent context "where is a listener's TLS version range and its errorloc redirect resolved and enforced"` | index **exit 0**: `files 84`, `lines 81 015`, `nodes 8 859`, `edges 15 594`, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms 1 237 720` (20.6 min). Claims checked in source: `protocol_versions` at [tls.rs:339](../../eqproxy-net/src/tls.rs#L339), used by the listener at [:457](../../eqproxy-net/src/tls.rs#L457) and the client at [:573](../../eqproxy-net/src/tls.rs#L573); `server_config_with_sni_and_max` resolved from [main.rs:3025](../../eqproxy-cli/src/main.rs#L3025); `ErrorPage.location` at [error_pages.rs:38](../../eqproxy-net/src/error_pages.rs#L38). The impact answer claimed `ErrorPages::from_pages` has **no production caller**, which is wrong: [main.rs:3166](../../eqproxy-cli/src/main.rs#L3166) resolves every listener's pages. That sentence is not used as evidence | `/tmp/cg-v091-tls.json` |
| Hindsight | `hindsight-note --title "v0.9.1-c TLS max_version and errorloc (B-I16/B-N16/B-H16)" --project EQProxy --tags tls,errorloc,v091` | stored (`status: completed`), 9 memory ids; first `7d86c34d-cb24-4e1a-a57b-174ab62dd4d8` | bank `eqproxy`, `http://127.0.0.1:8888/mcp/eqproxy/` |

The CodeGraph index was rebuilt over the final Rust tree after the last code
edit (the `cargo fmt` pass included) and before any documentation change that
matters to the graph. Its `impact` answer also names `main.rs:3050`/`:2526`
and the validation tests at `validate.rs:2825`/`:2858`; those were checked in
the source and match. Its declared gaps — no cycle result, no reverse closure
for `from_pages` — stay gaps, not passes.

## 8. Limits

- **Scope of the TLS knob.** Only the version bounds are operator-controlled.
  Cipher suites, OCSP, finite-field DH and the session-cache knobs stay
  provider-managed by ADR-0035/ADR-0046 §2; migrations report them as reviews,
  not translations.
- **`errorloc` is static and listener-scoped.** No `%[…]` expansion, no
  per-backend or per-rule pages; placeholders are refused rather than
  half-resolved. A backend-scoped HAProxy declaration is a manual review.
- **One dataplane.** The redirect and local pages are written by the shared
  HTTP/1.1 path (`serve_http`), which HTTP/2 streams also use; QUIC/HTTP-3 is
  not exercised by this journey. The QUIC rule tested here is the *config*
  freeze (fingerprint), not a live reload over QUIC.
- **Platforms.** The journey ran on macOS arm64 (Darwin 25.5.0, LibreSSL
  s_client). Linux and the container image are not re-run for this increment;
  the image is unchanged by the feature.
- **Differential tests** against HAProxy/nginx/Envoy for `errorloc` and
  `ssl_protocols` remain part of the 0.9.1 certification box, not of this
  increment.
