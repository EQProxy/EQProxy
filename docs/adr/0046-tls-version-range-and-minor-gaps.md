# ADR-0046: TLS version bounds, `errorloc`, and the B-H16 closing table

Language: **English**.

- State: **Accepted** (2026-11-01, roadmap v0.9.1)
- Date: 2026-11-01
- Related: [ADR-0035](0035-hot-secrets-and-drift-runbook.md) (TLS scope),
  [ADR-0045](0045-custom-error-pages.md) (custom error pages),
  [SPEC §7.5](../../SPEC.md), B-I16, B-N16, B-H16

## Context

The 0.9.1 box `B-I16 · B-N16 · B-H16 (fin)` asks for the remainder of TLS
advanced features and the minor HAProxy gaps. Most of that box was already
delivered:

- mTLS (`client_ca`, A15), multiple certificates per SNI and hot renewal of
  both listener and backend trust (ADR-0035);
- per-server `maxconn` (B-H16 first step, v0.8.5);
- custom error pages (B-H15/B-E18/B-N14, ADR-0045).

What remained was a real gap rather than a missing rewrite: nginx
`ssl_protocols` and HAProxy `ssl-max-ver` express a **maximum** version, and
EQProxy only had `min_version`. HAProxy `errorloc` (a redirect for an error
code) also had no equivalent. The other rows of B-H16 are system facilities,
deliberate exclusions, or already expressible with rules and pool policy.

## Decision

### 1. `tls.max_version` closes the version range

`[listener.*.tls] max_version` accepts `"1.2"` or `"1.3"`. Absent keeps the
previous behaviour (TLS 1.3). It must not be lower than `min_version`; a
QUIC listener cannot cap at 1.2 because QUIC is TLS 1.3. The value is part of
the TLS material fingerprint, so a live QUIC endpoint rejects a change it
would freeze, exactly like ALPN or the client CA.

Conversions use it where the source really expresses it: nginx
`ssl_protocols` (1.2 only → max 1.2; 1.2 + 1.3 → no cap), HAProxy
`ssl-min-ver`/`ssl-max-ver` on a bind, and Envoy
`tls_minimum_protocol_version`/`tls_maximum_protocol_version`. The
ingress-nginx ConfigMap `ssl-protocols` is controller-wide and the generated
listener has no certificate file yet, so it stays a review item with the
field named in the report.

### 2. Ciphers, OCSP, DH parameters and session caches stay provider-managed

ADR-0035 already excluded operator-chosen cipher suites (rustls/AWS-LC
negotiates the modern policy; exposing legacy ciphers would contradict the
project's TLS 1.0/1.1 ban) and OCSP stapling (an unauthenticated background
fetch with its own failure semantics). This ADR confirms that decision for
B-N16/B-I16:

- `ssl_ciphers`, `ssl_prefer_server_ciphers`, HAProxy `ciphers`/`curves` and
  Envoy `cipher_suites` are reported, not silently applied;
- `ssl_dhparam`/`ssl_ecdh_curve`: EQProxy uses ECDHE through rustls; finite
  field DH is not exposed;
- `ssl_session_cache`, `ssl_session_timeout`, `ssl_session_tickets`: rustls
  owns a bounded in-memory resumption cache. There is no operator knob; the
  import report says so. This is a deliberate E2 difference (no measurable
  contract beyond latency), not an unclassified gap.

`default-ssl-certificate` maps to the listener's `cert`/`key` pair: mount the
default Secret and point the listener at it. `strict-ssl`/`ssl-redirect` map
to a plain HTTP listener whose rules answer a redirect; the converter already
reports that shape.

### 3. `errorloc` is a redirect error page

`[[listener.*.error_pages]]` gains `location`: a static path or absolute
`http://`/`https://` URL. It is mutually exclusive with `file` and `body`;
`format = "raw"` is refused; `status` defaults to 302 and must be a 3xx other
than 304; `intercept` is allowed. The response has `Location` and
`content-length: 0`, no body. Placeholders (`$host`, `$uri`) are rejected:
HAProxy's `errorloc` is static and an unresolved placeholder in a `Location`
header would reach the client.

HAProxy `errorloc`, `errorloc302` and `errorloc303` convert in `defaults` and
`frontend`/`listen`; a code declared twice is replaced (last wins), never
emitted twice. A `backend`-scoped declaration is reported for manual
transcription because EQProxy pages are per listener.

### 4. The rest of B-H16

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

The E3 rows are not silently ignored: the converter names them as gaps or
review items, and this table is the public scope decision.

## Consequences

- The TLS range is now explicit and testable at the handshake, not just in
  the config gate.
- A migration no longer loses `errorloc`; a redirect page is a first-class
  error page and can intercept an origin response.
- The remaining B-H16 rows are closed by decision instead of staying in an
  open-ended "resto" box.
- No dependency, cipher list or runtime knob was added.

## Evidence

[v091-tls-gaps](../evidence/v091-tls-gaps.md) records the release-binary
journey, the handshake cap, the redirect cases and the conversion checks.
