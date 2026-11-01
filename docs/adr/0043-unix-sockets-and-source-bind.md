# ADR-0043: Unix-domain backend sockets and source bind — adopted

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-H14 · B-N15; to be
  implemented in 0.9.1-b)
- Date: 2026-10-31
- Related: nginx.md B-N15 (local-only: `ROADMAP EQPROXY/nginx.md`),
  haproxy.md B-H14 (local-only: `ROADMAP EQPROXY/haproxy.md`),
  [ADR-0021](0021-admin-unix-socket.md) (the admin plane already uses
  a Unix socket — this is about **backends**),
  [ADR-0039](0039-transparent-proxying-not-adopted.md) (TPROXY stays E3)

## Context

Both references can reach a local application without a TCP port:

- nginx: `proxy_pass http://unix:/run/app.sock:/path;`
- HAProxy: `server app /run/app.sock` or `server app unix@/run/app.sock`.

Both can also bind the **outbound** socket to a chosen local address
(HAProxy `source 10.0.0.5`), which is how multi-homed hosts pick an egress
interface. Transparent proxying (`usesrc clientip`) is a different feature and
is excluded by ADR-0039.

EQProxy models a backend server as `IP:PORT` or `HOSTNAME:PORT` only:
`ServerTarget` is `Ip(SocketAddr) | Name { host, port }`
(`eqproxy-config/src/schema.rs:268`). The only Unix socket in the product is
the administration plane's (ADR-0021). A Unix-socket backend is a real
migration blocker for local PHP/Python/gunicorn deployments, and source bind
is the portable half of B-H14.

## Decision

Adopt both, in the v0.9.1 implementation increment:

1. **Unix-domain backend addresses.** A server address may be
   `unix:/absolute/path` (nginx spelling). The `unix@/path` form recorded for
   HAProxy in the gap analysis is accepted and normalised to the same value.
   The path is validated at load: absolute,
   no NUL, bounded length. A Unix target is a local endpoint: no DNS, no
   `resolve_interval`, no TLS/SNI, no PROXY header.
2. **Per-server source address.** An optional `source = "IP"` on a server
   binds the outbound socket to that local address before connecting. Binding
   is best-effort at connect time; a failure is an explicit connect failure
   with its counter and error, never a silent fallback to the default address.

The implementation must reject, at validation time, the combinations that
cannot preserve their semantics: `proxy_protocol` with a Unix target, TLS with
a Unix target, and `source` on a Unix target. Those are configuration errors
with field, cause and example — not ignored keys.

## Out of scope

- TPROXY / `usesrc clientip` / `original_dst` (ADR-0039).
- Unix sockets as **dataplane listeners** (only backends); the admin socket is
  already covered by ADR-0021.
- Abstract Linux namespace sockets (`@name`) unless a concrete need appears;
  the first increment supports filesystem paths only.

## Acceptance criteria (for 0.9.1-b)

- A real `mode = "tcp"` and a real `mode = "http"` listener reach a backend
  over a Unix socket in E2E, with the origin on the other side of the socket.
- A missing/stale socket path fails with an explicit error (no hang, no
  retry storm), and the failure is counted.
- `source` binds and the backend observes the chosen address; an unusable
  source address produces an explicit connect failure.
- Validation rejects the forbidden combinations above with field, cause and
  example; the schema and `eqproxy.toml.example` document both knobs.
- The converter maps nginx `proxy_pass http://unix:…` and HAProxy
  `server … unix@…` to the new form instead of a gap, and its report says so.

## Consequences

- `ServerTarget` grows a variant and the connect path learns a second address
  family; both are covered by the existing pool/timeout code, which must be
  audited for the Unix case (no `peer_addr`, no keep-alive reuse assumptions
  about TCP).
- The importer's gap classification for Unix backends becomes a conversion;
  the corpus tests must change with it.
- This ADR does **not** claim TPROXY parity; B-H14's transparent half stays
  E3.

## Revisit triggers

1. The implementation shows that a Unix listener (not just a backend) is
   needed for a real migration.
2. Abstract namespace sockets or `SOCK_SEQPACKET` become a requirement.
3. Source bind needs per-backend rather than per-server scope.
