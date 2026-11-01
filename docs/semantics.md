# Semantics, limits and what EQProxy does NOT do

**Status:** public behaviour reference (v0.9.1, boxes H7 · H8). Every claim on
this page comes from [SPEC.md](../SPEC.md), an accepted ADR or a repository
test; when a figure is a design target rather than a measurement, the page says
so. It does not announce compatibility: the SPEC sentence still stands —
*compatibility not announced until demonstrated*.

The goal is for every feature to answer three questions explicitly: **what
semantics it preserves**, **what limits it applies** and **what it does not
do**. A known difference is documented here and in
[SPEC §13](../SPEC.md#13-semantic-difference-log-vs-the-reference-proxy); it is
not hidden behind the same word.

## 1. TCP (`mode = "tcp"`)

- **Semantics.** Bidirectional byte relay that does not interpret the payload.
  Each direction has its own `idle_timeout`; on expiry it closes with EOF
  towards its peer. A full `maxconn` is refused at accept time (immediate close
  plus counter), never queued without a bound. The schema accepts TLS on a TCP
  listener (ALPN is a hint for the tunnel, not an HTTP promise); the tunnel
  does not parse the payload (`tcp_listener_keeps_h2_alpn_for_byte_tunnels`,
  `validate.rs`).
- **Limits.** `maxconn` per listener; `idle_timeout` per direction; 16 KiB
  buffers per direction (SPEC §7.3); graceful shutdown lets live connections
  finish within `grace`.
- **What it does NOT do.** It does not speak HTTP, does not reuse backend
  connections and does not retry or apply HTTP rules. There is no generic UDP
  (§16). The per-connection memory budget is a design target; the Linux
  measurement is still pending (SPEC §7.3).

## 2. HTTP/1.1 (`mode = "http"`)

- **Semantics.** Strict in-house parser: head and body framed by
  `Content-Length` or `chunked`, keep-alive, `Expect: 100-continue` released
  with a real `100`, `Upgrade`/`CONNECT` tunnelled after a valid response.
  Ambiguous framing (CL+TE, duplicates, TE other than `chunked`,
  absolute-form, obs-fold) is **rejected**, not normalized (SPEC §13.4). A
  backend `101` opens the tunnel; it is not relayed as a normal response.
- **Product banner.** Every response the dataplane assembles carries exactly
  one `Server: eqproxy`. The origin's value is replaced, never appended, and
  the proxy does not advertise its version; this covers proxied HTTP/1.1, the
  h2/h3 paths that traverse the h1 engine, cached entries, local errors,
  redirects, auth denials and gRPC responses. The administration plane keeps
  its own responses unchanged. There is no `server_header_transformation`
  toggle, and a rule that sets `Server` cannot add a second line. Evidence:
  [v085-server-header](evidence/v085-server-header.md).
- **`Date`.** Every response the dataplane generates itself — local errors, the
  custom error pages and `errorloc`, redirects and direct responses, auth
  denials, the h2/h3 and WebSocket refusals, and gRPC local answers — carries
  one IMF-fixdate `Date` (RFC 9110 §6.6.1). A proxied response, cached or not,
  keeps the origin's value and is never given a second one; a rule that sets
  `Date` cannot forge it. The administration plane keeps its own responses
  unchanged, as it does for the banner. Evidence:
  [v091-date-header](evidence/v091-date-header.md).
- **Limits.** `limits.max_head_bytes` defaults to 16 KiB (1 KiB..1 MiB);
  `limits.max_headers` defaults to 100 (1..1000); `limits.max_body_bytes`
  defaults to 0 (unlimited), and when declared it is rejected with `413`
  before touching the backend, or when crossing the bound if the body is
  chunked. `client_header_timeout` and `server_header_timeout` bound each
  phase.
- **What it does NOT do.** It does not accept HTTP/0.9 or ambiguous messages
  "just in case". It does not invent a `Content-Length` or turn a chunked body
  into a length-delimited one. It does not compress responses to an HTTP/1.0
  client (which cannot read chunked framing; ADR-0010). The banner is fixed,
  not configurable.

## 3. HTTP/2

- **Semantics.** Terminates h2 via ALPN over TLS or via the h2c preface in
  clear text. Each stream is served by the same rule engine, actions, timeouts
  and errors as HTTP/1.1; framing is translated explicitly (DATA, trailers,
  hop-by-hop headers removed). Towards the backend,
  `[backend.*.tls] alpn = ["h2"]` opens a real HTTP/2 session and requests are
  multiplexed over it (ADR-0006, ADR-0008).
- **Limits.** At most **128 concurrent streams** per connection; DATA is read
  in 16 KiB units with capacity reserved before each frame (SPEC §13.10). Each
  stream's internal pipe is bounded (64 KiB by default, configurable with
  `limits.stream_buffer_bytes`).
- **What it does NOT do.** There is no server push. A classic `CONNECT` over
  h2 answers `501`; WebSocket over h2 uses extended CONNECT (§11). There is no
  h2 over cleartext towards the backend and no h3 towards the backend (§4,
  §16).

## 4. HTTP/3 and QUIC

- **Semantics.** Terminates QUIC/h3 on the configured TLS listener; requests
  go through the same HTTP engine as h1/h2. Certificate material is consulted
  on every handshake, so a rotation is adopted on the same UDP socket
  (ADR-0025).
- **Limits.** QUIC is TLS 1.3 by definition; the listener's `maxconn` bounds
  concurrent QUIC connections. h3 `stream_buffer_bytes` defaults to 16 KiB.
- **What it does NOT do.** There is no HTTP/3 towards backends: it is an
  experimental extension with no parity commitment (PARITY_MATRIX A5,
  `Designed (scope)`). Changing ALPN, the mTLS `client_ca` or the version range
  of a live QUIC endpoint requires a restart; only the certificate rotates
  hot.

## 5. TLS, SNI and mTLS

- **Semantics.** TLS 1.2 and 1.3; TLS 1.0/1.1 are never accepted. A default
  certificate plus SNI certificates: exact name, then the longest `*.`
  wildcard, then the default certificate; an unknown name does **not** fail the
  handshake. mTLS verifies the chain against `client_ca`. Towards the backend,
  certificate **and hostname** are verified by default (Mozilla roots +
  `ca_file`); `verify = false` is an explicit opt-out documented as dangerous.
- **Limits.** `min_version`/`max_version` bound the range; a QUIC listener
  cannot go below 1.3. Certificate renewal is atomic and does not drop
  established connections (ADR-0025). The handshake falls under
  `client_header_timeout`.
- **What it does NOT do.** Ciphers, OCSP stapling, DH parameters and the
  session cache are provider-managed: there is no operator knob
  (ADR-0035/ADR-0046). Certificate expiry and `not_after` are not checked.
  There is no mapping from mTLS client identity (subject/DN) to rules:
  accepting a certificate does not by itself authorize anything.

## 6. Rules, actions and native filters

- **Semantics.** Rules are evaluated in declaration order and **the first
  match decides** (`eqproxy-core/src/rules.rs`). Conditions: exact/regex host,
  path/prefix, method, IP and headers. Actions: backend, `redirect`,
  `direct_response`, request/response headers, prefix rewrite, CORS, auth, rate
  limit and mirror.
- **Limits.** `rate_limit` is an in-memory token bucket **per client IP and per
  rule**; `burst` defaults to `rate`. `auth` is fail-closed: `2xx` allows,
  `401`/`403` deny with the service's response, any other status is `500`, and
  a down service/timeout is `502`; the subrequest body is bounded to 8 KiB.
  CORS accepts one exact origin and rejects `*` with credentials.
- **What it does NOT do.** The rate limit is not anti-DDoS and is not shared
  between instances (each process has its own bucket). There is no Lua,
  snippets, WAF or per-request Envoy filter code: the path is WASM (§14) or an
  external service (`docs/equivalences.md`).

## 7. Balancing, health and queues

- **Semantics.** `roundrobin` (smooth weighted) and `leastconn` (effective
  weight, smoothed ties). `backup = true` forms a second tier consulted only
  when no primary is eligible; `priority` orders failover within the backups
  and `all_backups = true` balances the whole tier (no priority between
  backups). `slow_start` ramps the effective weight after each transition to
  operational. Health checks are TCP or HTTP with `expect status|rstring`;
  `fall`/`rise` decide the state change. A server address may be a local Unix
  socket (`unix:/absolute/path`) and a TCP server may bind its outbound socket
  to a literal `source`; the health checker dials from the same source.
- **Limits.** `weight` 0..=256 (0 = drained, never selected); `slow_start`
  0..=1 h; `maxqueue` bounds the per-backend queue and, when full, answers 503
  or exhausts the timeout, never grows without a bound; `retries` covers only
  the connection phase and non-idempotent requests require
  `retry_non_idempotent = true` (off by default). With no eligible servers:
  HTTP 503, TCP close, both counting `no_available_server`. A Unix target
  cannot use TLS, PROXY or `source`; those combinations are configuration
  errors, not ignored keys.
- **What it does NOT do.** There is no global `fullconn` and no UDP balancing.
  A check OK does not prove the application works (SPEC §13.7); the health
  check does not reinterpret the body.

## 8. Pool, timeouts and resource budget

- **Semantics.** Keep-alive pool per backend, shared by its listeners. A
  connection returns to the pool **only after a complete, clean exchange**; a
  framing error, truncation, timeout or server close discards it. The
  per-server idle cap follows the peak of in-flight exchanges (floor 64,
  ceiling 4096, ADR-0015) with a 60 s expiry.
- **Limits.** Per-phase timeouts (`connect`, `client`, `server`, `tunnel`,
  `queue`); a full queue is rejected explicitly; `maxconn` per listener and per
  server. Each proxied connection uses ~2 FDs plus listeners and admin
  (SPEC §7.3).
- **What it does NOT do.** There is no `client-fin`/`server-fin` split. TCP
  does not reuse connections. FD/memory/queue exhaustion produces an explicit
  rejection (error + metric + log), never simulated success; memory budgets are
  design targets until a Linux measurement validates them.

## 9. Response cache

- **Semantics.** Opt-in per HTTP listener (`[listener.<name>.cache]`, absent =
  disabled). Only `GET` is stored; `HEAD` serves headers from the `GET` entry.
  It is neither consulted nor stored if the request carries `Cookie`,
  `Authorization`, `Range` or `Cache-Control: no-store`, or if the response
  carries `Set-Cookie`; `206`/`302` bypass the cache. The key is method +
  scheme + lower-case host + target + values of the headers listed in `Vary`;
  `Vary: *` or `Vary` over `Cookie`/`Authorization` makes the response
  unstorable. A backend with cookie affinity avoids the cache.
- **Limits.** `default_ttl` 1 s..1 day (60 s by default; `s-maxage`/`max-age`
  shorten it, never extend it); `max_memory_bytes` 1 MiB..1 GiB (64 MiB by
  default) with LRU; `max_entry_bytes` 64 KiB..64 MiB (1 MiB by default) and
  ≤ memory; optional `disk_path`. Only bodies with `Content-Length` under the
  bound are assembled; chunked/close-delimited bodies bypass the cache. There
  is a cache-lock per key and a bounded `lock_wait`.
- **What it does NOT do.** It does not implement `stale-while-revalidate`,
  `stale-if-error`, `only-if-cached`, `Pragma`, client `If-Modified-Since`
  evaluation or `Vary` folding beyond the exact match (ADR-0034). The cache is
  per process; it is not shared between instances.

## 10. Response compression

- **Semantics.** Opt-in per HTTP listener; gzip and Brotli are negotiated from
  `Accept-Encoding` (RFC 9110 subset: `q=0` refuses, `*` covers, malformed q
  refuses; a tie goes to Brotli). Compression is **streaming**: the whole body
  is never buffered; the head is rewritten (old `Content-Length`/`TE`/
  `Content-Encoding` removed, `Content-Encoding` and `Transfer-Encoding:
  chunked` added, `Vary: Accept-Encoding` once).
- **Limits.** gzip `level` 1..9 (6 by default), `brotli_quality` 0..11 (5 by
  default), `min_size` 1024 by default (only applies with a
  `Content-Length`), `types` with `type/subtype`, `type/*` or `*/*`. It does
  not compress `HEAD`, 1xx/204/206/304, responses with `Content-Encoding`,
  `Content-Range` or `Cache-Control: no-transform`.
- **What it does NOT do.** It does not decompress backend responses, does not
  support zstd or precompression, and does not compress to HTTP/1.0 clients.
  Every request pays the CPU cost; there is no compression cache (ADR-0010).

## 11. WebSocket and CONNECT

- **Semantics.** WebSocket over HTTP/1.1 is a validated `Upgrade` and a
  bidirectional tunnel; over HTTP/2 it uses extended CONNECT. Rules choose the
  backend and can deny before touching the origin. A classic `CONNECT` over h1
  opens a raw tunnel after a `2xx`.
- **Limits.** The tunnel is subject to `idle_timeout` per direction; a silent
  tunnel is cut and counted. The `CONNECT` authority is routed by the rule; it
  does not connect to an arbitrary destination.
- **What it does NOT do.** WebSocket over HTTP/3 is not supported. A classic
  `CONNECT` over h2 answers `501` (extended CONNECT is used for WS). The tunnel
  payload is not inspected.

## 12. Error pages

- **Semantics.** Per listener, `[[listener.*.error_pages]]` maps codes to a
  `file`, `body` or a static 3xx `location`; `status` can change the code;
  `format = "raw"` serves a file that is already a complete HTTP response;
  `intercept = true` also replaces the origin's response and **closes** that
  connection instead of returning it to the pool.
- **Limits.** `file`/`body`/`location` are mutually exclusive; `raw` does not
  accept `body`; `location` requires a 3xx other than 304, no placeholders
  (`$host`, `$uri`) and `content-length: 0`; pages for 204 and 304 are not
  allowed; the body is resolved once at load time and served from memory; the
  page is the last response on that client connection, which the proxy then
  closes — a custom page does not leave the client keep-alive open.
- **What it does NOT do.** There are no per-route pages, only per listener; no
  dynamic placeholders in `location`; it does not apply to TCP listeners
  (ADR-0045). `format = "raw"` keeps only status, `Content-Type` and body; other
  header fields in the raw file are not preserved, and the generated `Date`
  replaces any written in the file.

## 13. Admin plane, reload and state

- **Semantics.** It listens on its own socket, loopback by default;
  `allow_remote = true` is the explicit opt-in and warns about the token in
  clear text. Separate read and write tokens; without a write token the plane
  is read-only. Hot writes (`PUT /api/v1/backends/{name}`,
  `PUT /api/v1/listeners/{name}`) validate and resolve the complete candidate
  before the swap, answer `200` only after the commit and require `If-Match`
  with the current revision. Certificates: `POST /api/v1/certificates/reload`
  and `SIGHUP` share the same single writer.
- **Limits.** Topology write bodies ≤64 KiB; server-state `PUT` ≤1 KiB with
  `application/json`; exchange head 8 KiB and 5 s; a panel session expires
  after 30 min of inactivity and at most 16 are kept. Unix socket `0600` or
  `0660` + gid, uid allowlist and POSIX ACL (Linux), verified by the kernel
  before accepting. Writes are runtime-only: the file remains the baseline and
  the next `SIGHUP` replaces them.
- **What it does NOT do.** There is no persistence/drift of API writes, no
  durable audit sink, no per-user identity (only read/write roles) and no
  multi-instance desired/applied revision. A candidate the dataplane rejects
  (bind, TLS frozen on QUIC) is `409`; neither parsing nor validation changes
  the live generation. Token or endpoint changes are not hot.

## 14. WASM plugins

- **Semantics.** Opt-in **request** filter per HTTP listener; one module per
  listener, executed inline in the dataplane worker for every request. It
  exports `memory`, `eqproxy_alloc` and `eqproxy_on_request`; it may import only
  `eqproxy.set_request_header` and `eqproxy.remove_request_header`. It can add,
  replace or remove request headers or reject with 400..599; framing/connection
  headers are reserved. `on_error = "closed"` answers 503 on trap/timeout;
  `"open"` forwards unchanged.
- **Limits.** 10 M fuel instructions by default, 50 ms deadline, 16 MiB memory
  cap and a per-call mutation budget; a fresh store per request (no state). No
  WASI, network or filesystem: any other import rejects the configuration at
  load time.
- **What it does NOT do.** There are no response filters, body access,
  background tasks or extensions that block the worker. WASM is not presented
  as perfect isolation (ADR-0031).

## 15. Configuration import

- **Semantics.** `eqproxy config import` converts a subset of HAProxy, nginx,
  ingress-nginx and Envoy to `eqproxy.toml` and emits a per-directive report:
  equivalent, review or gap with a cause. It never silently discards a
  directive; a clean conversion does not promise functional migration
  equivalence.
- **Limits.** The report classifies every directive in the corpus; those that
  cannot be expressed (for example UDP, WAF, Lua or snippets) remain an
  explicit gap. Subsequent validation and resolution are the same ones the
  binary uses.
- **What it does NOT do.** It does not implement the reference's full syntax or
  translate semantics EQProxy does not have; for those rows the answer is an
  exclusion ADR (§16) or a manual review.

## 16. UDP and explicit non-goals

- **Generic UDP: not supported (E3, ADR-0037).** EQProxy adds no UDP
  listeners, UDP balancing or UDP health checks. The only UDP surface is
  QUIC/HTTP3 (§4), which is a terminated protocol, not a datagram relay. An
  Envoy `udp_listener` or an ingress-nginx `udp-services` entry becomes an
  explicit gap. Alternative: keep the UDP hop in HAProxy or a dedicated load
  balancer and keep EQProxy for HTTP/TCP; for syslog, use TCP (RFC 6587) with
  `mode = "tcp"`.
- **WAF / ModSecurity / CRS:** out of scope (ADR-0040); the rate limit is not
  anti-DDoS. **Lua:** rejected by the 100 % safe-code policy (ADR-0041); the
  path is WASM. **Snippets:** E3 with a mapping to rules/WASM (ADR-0042).
  **FastCGI/AJP/uwsgi/scgi:** E3 (ADR-0032/ADR-0038). **TPROXY/
  `original_dst`:** E3 (ADR-0039). **Native k8s/Consul watch:** E3; the
  equivalent is DNS with re-resolution + the admin API (ADR-0044).
  **Commercial editions:** outside the parity scope. **Atomic multi-instance
  updates:** not promised without a protocol that guarantees them (SPEC §4).

## 17. How this page is maintained

Every new or changed feature updates three bullets (semantics, limits, what it
does NOT do) and links its ADR or test. If a figure has no measurement, it is
marked as a design target. If a difference is neither here nor in SPEC §13, it
is a documentation bug, not an accepted difference.
