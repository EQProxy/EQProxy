# v0.9.1 — `Date` on responses the proxy generates

**Objective.** Every response the dataplane assembles itself carries one
`Date` in IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`, RFC 9110 §5.6.7), as
RFC 9110 §6.6.1 expects of the generator. A proxied response keeps the
origin's value and is never given a second one. Before this change, local
errors, custom error pages and `errorloc`, redirects, auth denials and h2/h3
refusals carried no `Date` at all; nginx and HAProxy both send one.

## Change

One formatter, no new dependency (`Cargo.toml` and `Cargo.lock` gain nothing
for this change): date arithmetic over Unix seconds, with the whole conversion
in `eqproxy-net/src/date.rs`.

| Path | Where |
|---|---|
| formatter, IMF-fixdate from `SystemTime` | `eqproxy-net/src/date.rs:18` (`current_http_date`), `:24` (`http_date`), `:51` (`civil_from_days`) |
| module wired into the crate | `eqproxy-net/src/lib.rs:17` |
| local error (400/408/413/417/431/502/504/505…) | `eqproxy-net/src/http1.rs:1116` (`write_error`) |
| custom error page and `errorloc` | `eqproxy-net/src/http1.rs:1132` (`write_page`) |
| `local_response` (no server available) | `eqproxy-net/src/http1.rs:1222` (`write_no_server`) |
| local rule answer (redirect, direct response) | `eqproxy-net/src/http1.rs:1279` (`write_rule_response`) |
| auth deny | `eqproxy-net/src/http1.rs:1328` (`write_deny`) |
| h2 stream refused before the engine | `eqproxy-net/src/http2.rs:329` (`refuse`, header at `:336`) |
| h3 stream refused before the engine | `eqproxy-net/src/http3.rs:946` (`refuse`, header at `:953`) |
| extended/classic CONNECT refused over h2 | `eqproxy-net/src/ws.rs:314` (`refuse`, header at `:317`) |
| gRPC local answer and deny | `eqproxy-net/src/grpc.rs:626` (`local_answer`, header at `:647`) |

Proxied responses (h1, the h2/h3 paths through the h1 engine, gRPC pass-through
and cached entries) are not touched by the formatter: the only head rewrite
there is the existing `Server` normalizer, which replaces `Server` and leaves
every other line byte for byte.

## Decisions

1. **No dependency.** `httpdate`/`chrono` would be a new crate for one
   conversion; the algorithm is Howard Hinnant's `civil_from_days`, with the
   epoch, a leap day and both RFC example instants under test.
2. **Only the generator writes it.** A proxied response is relayed with the
   origin's `Date`; if the origin omits it, the proxy does not invent one,
   because it is not the generator of that response. A cached entry behaves
   like the response it stores.
3. **A rule cannot forge it.** `write_rule_response` and `write_deny` skip any
   configured `Date` header, exactly as they already skip `Server`, so the
   configured `response_headers`/`deny` cannot produce a second or stale
   value.
4. **Never fatal.** A clock before the Unix epoch falls back to the epoch
   instead of panicking, and an unrepresentable header value is skipped rather
   than failing the stream: a missing `Date` is a smaller defect than a failed
   response.
5. **Raw error pages lose their own `Date`.** `format = "raw"` already keeps
   only status, `Content-Type` and body (ADR-0045); the generated `Date`
   replaces any written in the file, and this is recorded in
   [semantics.md](../semantics.md#12-error-pages).

## Unit tests

`cargo test -p eqproxy-net --lib date::` → **5 passed, 0 failed** (211
filtered out), exit **0**:

- `the_epoch_is_a_thursday` → `Thu, 01 Jan 1970 00:00:00 GMT`.
- `the_rfc_example_is_formatted_exactly` → `Sun, 06 Nov 1994 08:49:37 GMT`.
- `a_leap_day_is_not_lost` → `Thu, 29 Feb 2024 00:00:00 GMT`.
- `a_recent_instant_matches_its_utc_date` → 2023-11-14 and 2033-05-18.
- `a_clock_before_the_epoch_falls_back_instead_of_panicking` → the epoch,
  no panic.

## E2E tests (real sockets)

The tests parse the header themselves through `common::assert_http_date`
(`eqproxy-net/tests/common/mod.rs:66`), an independent IMF-fixdate check:
weekday, two-digit day, month name, four-digit year, `hh:mm:ss` and `GMT`. A
test that called the dataplane formatter would prove it against itself.

| Suite / test | What it asserts | Command and result |
|---|---|---|
| `e2e_http1::server_header_is_the_product_banner_not_the_origin` | a proxied response keeps the origin's exact `Date`, with exactly one `Date` line, while `Server` is replaced | `cargo test -p eqproxy-net --test e2e_http1 -- unsupported_expectation_is_rejected_with_417 server_header_is` → **2 passed**, covering this row and the next |
| `e2e_http1::unsupported_expectation_is_rejected_with_417` | an early local error (`write_error`) carries exactly one valid `Date` | same command → **2 passed, 0 failed** |
| `e2e_http2::h2_dead_backend_answers_502_and_a_redirect_answers_locally` | the h2 local `502` carries a valid `Date` | `cargo test -p eqproxy-net --test e2e_http2 -- h2_dead_backend_answers_502` → **1 passed** |
| `e2e_http3::http3_connect_is_refused_and_a_dead_backend_is_an_explicit_502` | the h3 local `502` and the refused `CONNECT` (`501`) carry a valid `Date` | `cargo test -p eqproxy-net --test e2e_http3 -- http3_connect_is_refused` → **1 passed** |
| `e2e_grpc::grpc_dead_backend_is_an_explicit_502` | the gRPC local `502` (`local_answer`) carries a valid `Date` | `cargo test -p eqproxy-net --test e2e_grpc -- grpc_dead_backend_is_an_explicit_502` → **1 passed** |
| `a_local_connect_failure_serves_the_configured_page`, `intercept_replaces_an_origin_error_and_drops_its_connection`, `errorloc_answers_a_local_failure_with_a_bodyless_redirect`, `no_page_keeps_the_builtin_minimal_error` | the local connect-failure page, an intercepted origin error page, an `errorloc` redirect and the built-in error each carry exactly one valid `Date` | `cargo test -p eqproxy-net --test e2e_error_pages` → **9 passed** |
| `e2e_rules::redirect_action_answers_locally_with_rendered_location` and `direct_response_answers_locally_and_keeps_keep_alive` | each local rule response carries one valid `Date`; two responses on one keep-alive connection carry two, one each | `cargo test -p eqproxy-net --test e2e_rules -- redirect_action_answers_locally_with_rendered_location direct_response_answers_locally` → **2 passed** |
| `e2e_ws_h2::ws_unknown_protocol_is_refused_with_501` and `ws_classic_connect_keeps_its_501` | both `ws.rs` refusals carry a valid `Date` | `cargo test -p eqproxy-net --test e2e_ws_h2` → **8 passed, 0 failed** |

These are integration tests with the real listener, the real client and, where
a backend is involved, a real origin socket; the local-answer paths never touch
the backend, which the tests also assert through the connection counters.

### Real binary

The release build was also driven by hand, because the suite is not
independent evidence of the packaged binary. A listener on
`127.0.0.1:56382` pointed at a closed port (`127.0.0.1:56383`); a single
request got the built-in local error:

```console
$ ./target/release/eqproxy run --config /tmp/eqproxy-date2.toml &
$ curl -sS -D - -o /dev/null http://127.0.0.1:56382/
HTTP/1.1 502 bad gateway
Server: eqproxy
date: Sun, 01 Nov 2026 06:17:09 GMT
connection: close
content-length: 0
```

Body length 0, curl exit 0, and the log names the refused connection to the
dead backend. This is the `write_error` path of `http1.rs` in the real
artifact (rebuilt after the last code change), not a unit double.

## Gates on this tree

Platform: **Darwin arm64**, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, branch
`main`, HEAD `04859af`, 135 modified/untracked paths (the 0.9.1 tree is not
committed). All commands ran on 2026-11-01.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit **0** |
| `cargo test --workspace` | exit **0** — **36 targets, 711 passed, 0 failed, 2 ignored** |
| `cargo build --workspace --release` | exit **0**; `target/release/eqproxy --version` → `eqproxy 0.9.1` |

The suite grew from 698 to 711 because the date module, its five unit tests and
the shared test helpers (`mod common`, two socket helpers) are compiled into
the suites that now assert the header; no test was removed, ignored or
weakened.

## Audits

These ran on the final working tree on 2026-11-01; they still have to be
repeated on the cut commit before the tag, as
[release-0.9.1.md](../release-0.9.1.md) §2.2 requires.

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit **0**: rustfmt, check, clippy, test, nextest, llvm-cov, audit, deny, machete, veyra-e2e, anti-theater OK. `geiger` INCONCLUSIVE (coverage warnings); dylint and semver-checks skipped with their reasons | `.veyra/evidence/20261101T055442Z/report.md` |
| Rust Audit | `python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit **1** by rule: **0 critical / 7 high / 417 medium / 111 low**; the seven high are the previously triaged cache/fuzz set, identical to the scan before this delta, and `date.rs` has no finding | `report/rust-audit-20261101-072304.md` (local-only) |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091-date2.json --batch-size 16`; `agent impact "what breaks if current_http_date …"`; `agent context "where are locally generated HTTP responses …"` | index exit **0**: 85 files, 81,432 lines, 8,924 nodes, 15,744 edges, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms 79 558`. The impact query found the 9 direct dependents listed above, checked in source; its suggestion to validate the return value is not a defect (the formatter emits fixed ASCII by construction). The context query omitted `http3.rs` from its first list; the source has the call at `http3.rs:953` | `/tmp/cg-v091-date2.json` |
| Hindsight | `hindsight-note --title "v0.9.1 Date header on generated responses" --project EQProxy --tags http,date,v091,evidence` | stored (`status: completed`), 8 ids; first `99d880e9-53d7-47c8-903f-4561322e46c2` | bank `eqproxy` |

## Limits

- No Docker image, `linux/amd64` build, Docker Scout scan or CI run was made
  for this delta: **NOT EXECUTED**. The host here is arm64 macOS. The image
  gate remains as described in the release checklist.
- TLS/mTLS does not change the header path, but no separate TLS journey was
  repeated for this delta.
- External URLs, rendered HTML and `#L…` anchors are outside this page's scope;
  the repository link sweep is [v091-link-sweep](v091-link-sweep.md).
- The administration plane keeps its own responses unchanged, as it does for
  the banner (ADR-0017): it is a management surface, not a site response.
- The `100 Continue` interim response still carries no header fields; RFC 9110
  does not require `Date` on it, and this change does not alter that.
