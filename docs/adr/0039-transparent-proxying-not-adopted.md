# ADR-0039: transparent proxying (TPROXY / original_dst) — not adopted (E3)

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-E14 · B-H14)
- Date: 2026-10-31
- Related: envoy.md B-E14 (local-only: `ROADMAP EQPROXY/envoy.md`),
  haproxy.md B-H14 (local-only: `ROADMAP EQPROXY/haproxy.md`) (`usesrc clientip`,
  `transparent`), [ADR-0005](0005-container-image-runtime.md) and
  [ADR-0016](0016-wolfi-runtime-base.md) (the unprivileged image),
  [ADR-0043](0043-unix-sockets-and-source-bind.md) (the source-bind part that
  **is** adopted)

## Context

Transparent proxying makes the backend see the **original client address**,
or makes the proxy connect to the **original destination** of an intercepted
packet. HAProxy does it with `source … usesrc clientip`, `transparent` and
policy routing; Envoy with `original_dst_lb_policy`, `internal_listeners` and
TPROXY support.

Both techniques depend on the host network, not on the proxy:

- `IP_TRANSPARENT` sockets require `CAP_NET_ADMIN` (or root) and a routing
  rule that sends the client's address through the proxy;
- `original_dst` requires `SO_ORIGINAL_DST` from netfilter/mesh interception
  and the same routing setup;
- the rules live in iptables/nftables, outside the configuration file EQProxy
  validates, and a wrong rule silently blackholes traffic.

EQProxy's shipped image runs as **uid/gid 65532** (`docker/Dockerfile:79`);
the dataplane has no `IP_TRANSPARENT`, `SO_ORIGINAL_DST` or `original_dst`
code (`grep` over `eqproxy-*/src` finds only the importer's "TPROXY: sin
soporte" message). The configuration has no address-transparency knob.

## Decision

TPROXY and `original_dst` transparent proxying are **out of scope** (E3).
EQProxy keeps opening ordinary outbound sockets and connecting to the address
in the validated configuration.

Reasons:

1. **Privilege and portability.** The feature contradicts the default
   non-root container and the "no host network magic" deployment; it cannot be
   exercised by the same E2E suite that proves everything else without
   privileged fixtures.
2. **The contract would live outside the config.** A feature whose correctness
   depends on routing rules the proxy cannot validate or report is a
   promise this project cannot keep honestly.
3. **The portable equivalent exists and is shipped.** `proxy_protocol = "v1"`
   / `"v2"` to the backend preserves the client address at the application
   layer, is validated by the backend, and is covered by E2E
   (`e2e_phase2b.rs::proxy_protocol_v1`/`proxy_protocol_v2`). The backend
   trusts the proxy, not the routing table.
4. **Source binding is not the same thing.** Binding the outbound socket to a
   local address (`source 10.0.0.5`) is adopted in ADR-0043; it does not
   rewrite the client address and needs no privileges.

## Alternative (the "or equivalent" path)

- Use `proxy_protocol = "v1"`/`"v2"` so the backend learns the real client
  address, or `X-Forwarded-For` via the header rules.
- Run a transparent front end (HAProxy, Envoy, a mesh sidecar) where the
  network architecture requires it and keep EQProxy behind it.

## Consequences

- The matrix classifies B-E14 as **E3** with this ADR; B-H14's transparent
  half is closed, its Unix-socket and source-bind half is ADR-0043.
- The importer keeps `transparent` and `usesrc` as gaps with a cause; no
  conversion to a plain backend without disclosure.
- No privileged socket option, capability or host-network requirement enters
  the product.

## Revisit triggers

1. A deployment the owner controls requires address transparency and accepts
   privileged fixtures plus a documented routing contract.
2. A design that validates the routing setup at startup and reports it in
   `doctor`/status is specified and tested.
3. The container stops being non-root by default (which would be its own ADR).
