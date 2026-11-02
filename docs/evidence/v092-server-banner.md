# v0.9.2 — `Server: EQProxy` on every dataplane response

**Objective.** Every response the dataplane assembles carries exactly one
`Server: EQProxy`. The origin's value (often `nginx/1.24.0`, a version oracle)
never reaches the client, and the proxy advertises no build number. v0.9.1
shipped the same policy with the value `eqproxy` (tag `e7473ea`); this increment
changes only the capitalisation, so the v0.9.1 captures keep what the released
image did and this page records the new value.

## Change

One constant owns the value in the HTTP/1.1 engine; the builders outside that
engine set the same value because they never enter it.

| Path | Where |
|---|---|
| constant | `eqproxy-net/src/http1.rs` (`SERVER_HEADER`) |
| head normalizer | `eqproxy-net/src/http1.rs` (`with_server_header`) |
| proxied h1, and every h2/h3 listener that traverses the h1 engine | `eqproxy-net/src/http1.rs` (response assembly) |
| `101`/`CONNECT` switch head | `eqproxy-net/src/http1.rs` |
| cached response and its `304` | `eqproxy-net/src/http1.rs` |
| local error, custom error page, `errorloc` | `eqproxy-net/src/http1.rs` (`write_error`, `write_page`) |
| local `redirect`/`direct_response` | `eqproxy-net/src/http1.rs` (`write_rule_response`) |
| auth deny | `eqproxy-net/src/http1.rs` (`write_deny`) |
| custom local reply (no server available) | `eqproxy-net/src/http1.rs` (`write_no_server`) |
| h2 stream refused before the engine | `eqproxy-net/src/http2.rs` (`refuse`) |
| h3 stream refused before the engine | `eqproxy-net/src/http3.rs` (`refuse`) |
| extended/classic CONNECT refused over h2 | `eqproxy-net/src/ws.rs` (`refuse`) |
| gRPC pass-through | `eqproxy-net/src/grpc.rs` (`serve`) |
| gRPC local answer and deny | `eqproxy-net/src/grpc.rs` (`local_answer`) |

## Decisions

1. **Replace, never append.** `Server` is a singleton; appending would leave the
   origin's version in the response the client parses.
2. **No version.** `EQProxy`, not `EQProxy/0.9.2`. The value is stable across
   upgrades, so it cannot be used to pick a known-vulnerable build.
3. **Mixed case.** The value follows the written product name that the rest of
   the documentation uses; the wire spelling and the brand do not diverge.
4. **The administration plane is untouched.** Its responses are a separate
   surface (`eqproxy-net/src/admin.rs`, absent from this diff).
5. **Five literals, one owner each.** The h1 engine reads one constant; the four
   builders outside it (`http2`, `http3`, `ws`, `grpc`) repeat the value because
   they answer a stream the engine never sees. This page names all five so a
   future rename cannot miss one.

## Unit and E2E tests

Run on 2026-11-02, Darwin arm64, `rustc 1.99.1`, against the working tree that
becomes this increment (binary SHA-256 below).

| Command | Result |
|---|---|
| `cargo test -p eqproxy-net --lib server_header` | 3 passed, 0 failed |
| `cargo test -p eqproxy-net --test e2e_http1 server_header_is_the_product_banner_not_the_origin` | 1 passed |
| `cargo test -p eqproxy-net --test e2e_cache miss_then_hit_serves_identical_bytes` | 1 passed |
| `cargo test -p eqproxy-net --test e2e_resilience local_response_serves_the_configured_body` | 1 passed |
| `cargo test -p eqproxy-net --test e2e_grpc grpc_forwarded_headers_cross_the_pass_through` | 1 passed |
| `cargo test -p eqproxy-net --test e2e_ws_h2 ws_unknown_protocol_is_refused_with_501` | 1 passed |
| `cargo test -p eqproxy-net --test e2e_pool close_request_is_never_pooled` | 1 passed |

The unit test `server_header_replaces_the_origin_value_without_a_version`
covers the replacement, a mixed-case `server:` header, a head without one, and
a second pass that must not duplicate it. The e2e tests drive real sockets: an
origin that answers `Server: nginx/1.24.0`, a refused extended CONNECT, a gRPC
pass-through whose origin answers `server: grpc-origin/1.0`, a local reply, and
a `Connection: close` exchange.

## Real-binary journey

Script (maintainer tree, local-only):
`scripts/evidence/v092-server-banner/journey.sh`. Raw output:
[raw/](v092-server-banner/raw/). Artifact: `target/release/eqproxy`, SHA-256
`b4e5873a3922a20c8ccebf95b231f853fceeecdc94fa02c1ea9bf788744845ab`
(environment in [00-environment.txt](v092-server-banner/raw/00-environment.txt)).

| # | What it proves | Observed | Raw |
|---|---|---|---|
| 1 | control: the origin itself advertises a version | `Server: nginx/1.24.0` | [01](v092-server-banner/raw/01-control-direct.headers) |
| 2 | proxied h1: the origin's banner is replaced, one value | `Server: EQProxy`, `x-cache: MISS`, no `nginx` substring | [02](v092-server-banner/raw/02-h1-proxied.headers) |
| 3 | cached h1: the stored entry is stamped at serve time | `Server: EQProxy`, `x-cache: HIT`, same 15 bytes as the MISS | [03](v092-server-banner/raw/03-h1-cached.headers) |
| 4 | h2c: the h2 path through the h1 engine carries the same banner | `HTTP/2 200`, `server: EQProxy`, `x-cache: HIT` | [04](v092-server-banner/raw/04-h2c.headers) |
| 5 | local 502: a response the proxy builds itself carries the banner and a `date` | `HTTP/1.1 502 bad gateway`, `Server: EQProxy` | [05](v092-server-banner/raw/05-local-502.headers) |

The origin log records **two** requests (the control and the first proxied
request): the cache hit and the h2c request never reached it
([origin-requests.txt](v092-server-banner/raw/origin-requests.txt)). The config under test is
[config.toml](v092-server-banner/raw/config.toml).

## Limits

- The h3 refusal builder sets the same value (`http3.rs::refuse`), but this
  journey did not open a QUIC connection: no h3 capture is claimed.
- The h2 capture is h2c with prior knowledge on loopback; TLS h2 uses the same
  engine and is not re-captured here.
- No performance or compatibility claim: the change is one header value.
- The v0.9.1 evidence pages keep their captured `Server: eqproxy` outputs; they
  describe the image that tag published.
