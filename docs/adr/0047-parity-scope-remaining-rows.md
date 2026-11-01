# ADR-0047: Parity scope for the remaining reference rows

Language: **English**.

- State: **Accepted** (2026-11-01, roadmap v0.9.1)
- Date: 2026-11-01
- Related: [ADR-0027](0027-xds-bridge-decision.md) (xDS), [ADR-0032](0032-fastcgi-uwsgi-scgi-decision.md)
  (FastCGI/uwsgi/scgi), [ADR-0037](0037-udp-proxying-not-adopted.md) (UDP),
  [ADR-0038](0038-ajp-gateway-not-adopted.md) (AJP), [ADR-0039](0039-transparent-proxying-not-adopted.md)
  (TPROXY), [ADR-0040](0040-waf-not-adopted.md) (WAF), [ADR-0041](0041-lua-not-adopted.md)
  (Lua), [ADR-0042](0042-snippets-not-adopted.md) (snippets), [ADR-0044](0044-no-native-service-discovery.md)
  (service discovery), [ADR-0046](0046-tls-version-range-and-minor-gaps.md) (B-H16)

## Context

The v0.9.1 parity matrix classifies every row of HAProxy 3.4.6 LTS, nginx,
ingress-nginx and Envoy API v3 as **E1** (same observable semantics, proven by
a differential test), **E2** (equivalent with a known, documented difference)
or **E3** (out of scope by ADR, with an alternative).

Most rows already had an implementation, a test and a documented difference.
A handful were still `Designed (scope)` in the maintainer's local HAProxy
table or in the four inventories: they had neither code nor an explicit decision, so
the matrix could not finish without either implementing them or excluding them.
This ADR records the exclusion decisions. It adds no dependency and changes no
runtime behaviour.

Two rows often mistaken for gaps are **not** E3 and are recorded here for
clarity:

- **B9 cookie persistence** is **E2**: `[backend.*.affinity]` with
  `mode = "cookie"` issues and reads its own sticky cookie
  (`affinity_cookie_and_source_pin_clients_to_a_server`). It does not
  implement HAProxy's `cookie NAME insert|rewrite|prefix` modes or
  `maxidle`/`maxlife`; the difference is documented in the matrix.
- **A6 IPv6** is **E2**: a listener and a backend can both be IPv6 literals
  (`an_ipv6_listener_proxies_to_an_ipv6_backend`). The reference's
  `bind … v4v6` wildcard keyword has no direct equivalent; EQProxy binds the
  literal family and lets the OS decide dual-stack.

## Decision

### 1. HAProxy rows excluded (E3)

| Row | Capability | Why it stays out | Alternative |
|---|---|---|---|
| A5 | HTTP/3 to backends | HAProxy itself marks it experimental; the committed direction is h3 to clients, and no backend h3 client is needed for parity of shared semantics | h2 backend or a TCP tunnel to an h3 origin |
| B2 | `balance static-rr` | The balancer deliberately has two algorithms (`roundrobin`, `leastconn`) plus a hash policy; `static-rr` is a fixed rotation whose observable result is covered by weighted RR when weights do not change at runtime | `balance = "roundrobin"` |
| B6 | `balance random` | No deterministic distribution to preserve; random selection defeats reproducible tests and the project's explicit tie-breaking policy | `roundrobin`, `leastconn` or `hash_policy` |
| B10 | `stick-table` (general shared tables) | Per-rule fixed-window rate limiting and cookie/source affinity cover the operational uses; a general table is unbounded shared state with its own expiry/typing semantics | per-rule `rate_limit`, `affinity`, or an external store |
| C2 | Converters (chained transformations) | The rule engine has explicit path/host/header mutations, not an expression converter language; a converter engine would be a second, unreviewed rule language | `rewrite_prefix`, header actions, or a WASM plugin |
| C7 | `set-var` / `var()` namespaces | There is no variable namespace in the rule engine; lifetimes and scopes would need a new model. Maps and the WASM plugin cover the common state-carrying cases | `maps`, rule conditions, WASM plugin |
| C8 | `peers` state sync | EQProxy declares no cross-instance state protocol; each instance keeps its own view (SPEC §9 and ADR-0046) | external shared store; per-instance affinity |
| D10 | Multi-instance management | No distributed atomicity is declared without a protocol (SPEC §9); the admin API applies one instance at a time | run independent instances behind a load balancer |
| D12 | Master-worker binary upgrade with FD handover | The delivered mechanism is hot config application on the same process (ADR-0019); an FD-handover upgrade is a different lifecycle and is not promised | graceful rolling restart behind a load balancer |

### 2. nginx non-proxy server features (E3)

`B-N20` mixes several non-proxy nginx features. The following are **E3**:

- static file serving (`root`, `index`, `try_files`, `alias`, `autoindex`,
  `sendfile`, `open_file_cache`, …): EQProxy is a proxy, not a web server;
  keep the static tier in an origin;
- the variable/expression language (`geo`, `split_clients`, `set`,
  `perl_set`, `js_set`, `if`): no expression evaluator by design; `map` has
  an equivalent in `[maps]` (row C10), and WASM covers programmable filters;
- `ssl_preread` (SNI routing in the stream module): terminate TLS and route by
  host, or keep a TCP passthrough;
- `zone`/shared upstream state: one process, no shared worker memory.

The remaining `B-N20` entries are **E2** (documented differences): `include`
is not expanded by the converter, `slow_start` and per-server `max_conns` have
the EQProxy equivalents, `proxy_next_upstream` maps to safe retries, and
WebSockets are tunnelled in HTTP mode.

### 3. What remains E2

The matrix's E2 rows keep their implementation and test, with the difference
named in the row. This ADR does not reclassify them: a row is E2 when the
behaviour exists and the difference is documented, not when the directive
spelling differs.

## Consequences

- The public matrix has no `Designed (scope)` row left: every row is E1, E2 or
  E3 with an ADR, a differential case or a named test.
- Nine HAProxy rows and four nginx non-proxy families are explicitly out of
  scope; none is silently ignored by the converters (they report the gap or a
  review item).
- E1 remains limited to the cases the differential corpus actually executes;
  the rest is E2, never an inferred E1.
- No runtime, dependency or configuration change is introduced.

## Evidence

The matrix and its counts: [docs/parity-matrix.md](../parity-matrix.md).
The differential run and its raw report: [v091-parity-matrix](../evidence/v091-parity-matrix.md).
