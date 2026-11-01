# ADR-0038: AJP gateway — not adopted (E3)

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-I12 · B-N17)
- Date: 2026-10-31
- Related: [ADR-0032](0032-fastcgi-uwsgi-scgi-decision.md) (FastCGI, uwsgi and
  scgi — the same decision, already recorded),
  ingress-nginx.md B-I12 (local-only: `ROADMAP EQPROXY/ingress-nginx.md`) (the gap),
  roadmap v0.9.1, §1 (local-only: `ROADMAP EQPROXY/ROADMAP/Roadmap 0.9.1.md`)

## Context

ingress-nginx's `backend-protocol` annotation can be `AJP`, which makes the
controller speak the AJP13 binary protocol to a Tomcat backend. nginx itself
has no AJP upstream module in the core distribution; the feature arrives
through ingress-nginx's own plumbing (and historically a third-party module).
EQProxy has no AJP code and no dependency for it; the importer treats any
`backend-protocol` value outside `HTTP`/`HTTPS`/`GRPC` as a **gap** with a
cause (`import::ingress`), so AJP is already refused, not dropped.

## Decision

AJP is **out of scope** (E3), for the same reasons ADR-0032 records for
FastCGI, uwsgi and scgi:

1. AJP is an application-server protocol, not HTTP: its own binary framing,
   request attributes, `secret` authentication on the Tomcat side and its own
   error model.
2. No corpus this project migrates uses it; ingress-nginx exposes it as an
   annotation, not as the default path.
3. The equivalent deployment is to keep Tomcat behind its HTTP connector (or
   any AJP front end) and proxy that HTTP endpoint, where routing, TLS, limits
   and observability are preserved.

ADR-0032 remains the ADR for `fastcgi_pass`/`uwsgi_pass`/`scgi_pass`; this ADR
extends the same decision to AJP so the roadmap box B-I12/B-N17 closes with a
single, citable policy.

## Alternative (the "or equivalent" path)

- Tomcat's HTTP connector (or a front end that speaks AJP) terminates AJP and
  speaks HTTP to EQProxy.
- No Tomcat-specific attribute mapping is promised; the HTTP contract is the
  supported contract.

## Consequences

- The matrix classifies AJP as **E3**, with ADR-0032 + this ADR.
- The converter keeps `backend-protocol: AJP` as a gap with a cause; it must
  never convert it to an HTTP backend silently.
- No protocol implementation, dependency or listener mode is added.

## Revisit triggers

1. A real migration corpus contains AJP and the owner accepts the
   application-gateway scope (which would also reopen ADR-0032).
2. A gateway design with conformance tests for the attribute set and `secret`
   handling is specified.
3. The converter starts reporting AJP as anything but a gap.
