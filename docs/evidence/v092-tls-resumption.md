# v0.9.2 — TLS 1.3 session resumption (A16)

Date: 2026-11-03 (UTC)
Status: **implemented in the library and verified over a real TCP socket.
Not a benchmark scenario; the 18-scenario roadmap is now at 18/18 after
S-STR-01, S-FAIL-01, S-CFG-01, S-OVL-01 and S-PLG-01 (executed; only S-TLS-01
certified).**

## 1. Objective

Returning TLS clients used to pay for a full handshake on every connection:
rustls is stateless by default and sends no ticket unless the server installs
a ticketer. That is a direct client cost (extra round trips and CPU) and a
product limit, not a reason to change the 2 GiB / 2 CPU benchmark limits.
This slice adds TLS 1.3 session tickets without adding a dependency or an
operator secret.

## 2. Change

`eqproxy-net/src/tls.rs` installs rustls' provider ticketer after ALPN is set:

```rust
config.ticketer = rustls::crypto::aws_lc_rs::Ticketer::new()
    .map_err(|e| TlsError::Config(format!("TLS session ticket keys: {e}")))?;
```

The ticketer is the one already linked through `aws-lc-rs`. It uses the
RFC 5077 recommended construction, a **12 h ticket lifetime** and **6 h key
rotation**, and is not configurable per listener in this slice. The clone used
by a QUIC listener inherits the same server configuration.

**What this does not do:** tickets are encrypted with process-local random
keys. A restart invalidates them, and two EQProxy instances do not share them.
There is no operator-provided shared ticket key, no external session cache and
no 0-RTT. Those are deployment features, not silently implied by this change.

## 3. Verification

The test `tls13_session_ticket_resumes_on_a_second_connection` listens on a
real loopback TCP socket, accepts two TLS 1.3 connections through
`TlsAcceptor`, and uses one `TlsConnector` (and therefore one client ticket
store) for both. It writes and reads an application byte so the
post-handshake `NewSessionTicket` is processed, asserts the first handshake is
**not** `HandshakeKind::Resumed`, and asserts the second is **Resumed**.

```text
cargo test -p eqproxy-net tls13_session_ticket_resumes_on_a_second_connection -- --nocapture
test tls::tests::tls13_session_ticket_resumes_on_a_second_connection ... ok
test result: ok. 1 passed; 0 failed
```

`cargo fmt --all -- --check` and
`cargo clippy -p eqproxy-net --all-targets -- -D warnings` passed on the same
tree. `cargo test --workspace` then passed **35 suites, 673 tests, 0 failed,
2 ignored**, exit 0 (the full log was captured by the test process in the
maintainer tree; it is not a published artifact).

An additional `openssl s_client` attempt was **discarded**: LibreSSL 3.3.6
and OpenSSL 3.6.4 did not materialise a session file with `-sess_out` in this
environment, so no `-sess_in` resumption result was produced. That is a
client-test limitation, not evidence against the product; the rustls
real-socket test above is the evidence used.

## 4. Limits

- **Not a benchmark.** No throughput, latency or CPU comparison for resumed
  vs full handshakes was run. The expected client benefit is unmeasured here.
- **Process-local tickets.** No resumption across restarts or replicas; no
  shared ticket key configuration.
- **No 0-RTT.** The client still sends application data only after the
  handshake, and early-data replay protection is not implemented.
- **Debug binary/test.** The change was compiled and tested from the working
  tree; no release image was built or published in this slice.
- **No audit.** Veyra, Rust Audit and CodeGraph are reported separately in the
  delivery; this page does not substitute for them.
- **No commit, push or publication.**

## 5. Reproduce

```bash
cargo fmt --all -- --check
cargo test -p eqproxy-net tls13_session_ticket_resumes_on_a_second_connection
cargo clippy -p eqproxy-net --all-targets -- -D warnings
```
