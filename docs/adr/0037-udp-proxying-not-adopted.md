# ADR-0037: generic UDP proxying — not adopted (E3)

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box H7 · B-I18 · B-E13)
- Date: 2026-10-31
- Related: roadmap v0.9.1, §1 (local-only: `ROADMAP EQPROXY/ROADMAP/Roadmap 0.9.1.md`),
  haproxy.md H7 (local-only: `ROADMAP EQPROXY/informe-criticas-haproxy.md`) (the
  request), envoy.md B-E13 (local-only: `ROADMAP EQPROXY/envoy.md`) and
  ingress-nginx.md `udp-services` (local-only: `ROADMAP EQPROXY/ingress-nginx.md`),
  [ADR-0013](0013-http3-quic.md) (the only UDP surface EQProxy serves)

## Context

The three references can relay **arbitrary UDP datagrams**:

- HAProxy gained UDP listeners (`bind … proto udp`) with `balance` and health
  checks for syslog collectors and appliances that only speak UDP; the request
  is tracked as H7 in the complaints report.
- Envoy has `udp_listener` and a UDP proxy filter (B-E13).
- ingress-nginx exposes a `udp-services` ConfigMap that maps ports to
  Services.

EQProxy has no generic UDP path at all: `grep -rniE "udpsocket|udp"
eqproxy-net/src eqproxy-config/src/schema.rs` returns only the QUIC endpoint that
serves HTTP/3 (ADR-0013) — no UDP listener and no datagram relay. The importer
already refuses the feature explicitly: an Envoy `udp_listener` becomes a
gap ("EQProxy solo habla UDP para QUIC/HTTP3") and an ingress `udp-services`
port becomes a gap with the same cause.

## Decision

Generic UDP proxying is **out of scope** (E3). EQProxy does not add UDP
listeners, UDP balancing or UDP health checks in the 0.9.x line or in the
v0.9.1 certification scope.

Reasons:

1. **It is a different dataplane.** A datagram has no connection, no
   half-close, no request boundary and no framing; the exchange accounting,
   retry and drain model this proxy is built on does not map onto it. A UDP
   balancer is a separate product with its own failure semantics (reordering,
   duplication, MTU, amplification).
2. **The observable contract would be weaker than promised.** "Health check"
   for UDP is a probe heuristic (a reply, or none), not an application-level
   check. Presenting it as parity with `httpchk` would overstate what is
   verified.
3. **The migration demand is narrow and has a correct home.** Syslog and
   appliance traffic is served by HAProxy or a dedicated UDP load balancer in
   front of the same backends; EQProxy keeps the HTTP/TCP hop. Nothing in the
   four migration corpora this project maintains requires UDP.
4. **HTTP/3 already covers the one UDP surface that belongs here.** QUIC is
   terminated as a protocol, not exposed as a generic datagram relay, and that
   distinction is deliberate.

## Alternative (the "or equivalent" path)

- Run the UDP hop on HAProxy/a dedicated balancer and keep EQProxy for the
  HTTP/TCP listeners behind it.
- For syslog, the collector can also accept TCP (RFC 6587) and be proxied by
  EQProxy's `mode = "tcp"`, which preserves delivery and adds real connection
  accounting.

## Consequences

- The parity matrix classifies H7/B-I18(udp)/B-E13 as **E3**, citing this ADR;
  no roadmap box remains open for generic UDP.
- The converter keeps classifying UDP as an explicit gap with a cause (never a
  silent conversion); the existing messages are the contract.
- No UDP socket, dependency or listener mode is added; the QUIC endpoint is
  untouched.
- Documentation states the exclusion where the feature is described, not only
  in this ADR.

## Revisit triggers

1. A real migration corpus contains UDP traffic and the owner accepts a second
   dataplane with its own semantics and acceptance tests.
2. A UDP design with application-level health, bounded queues and observability
   equivalent to the TCP path is specified.
3. The converter starts reporting UDP as anything but a gap.
