# ADR-0044: no native Kubernetes/Consul discovery — DNS and the admin API are the equivalent (E3)

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-I3 · B-H13 · B-N18)
- Date: 2026-10-31
- Related: ingress-nginx.md B-I3 (local-only: `ROADMAP EQPROXY/ingress-nginx.md`),
  envoy.md B-E15 (local-only: `ROADMAP EQPROXY/envoy.md`) (EDS/DNS),
  haproxy.md B-H13 (local-only: `ROADMAP EQPROXY/haproxy.md`) (`server-template`),
  [ADR-0019](0019-hot-config-writes-via-admin-api.md),
  [ADR-0027](0027-xds-bridge-decision.md) (no xDS bridge),
  [v08-backup](../evidence/v08-backup.md) and the v0.6 DNS evidence

## Context

ingress-nginx watches Kubernetes `Service`/`Endpoints` objects and balances
over pods as they change; Envoy uses EDS or `STRICT_DNS`; HAProxy uses
`server-template` with DNS. EQProxy has:

- `HOSTNAME:PORT` servers resolved in the background, with
  `resolve_interval` re-resolution and an explicit 503 while a name does not
  resolve (v0.6; `ServerTarget::Name`, `schema.rs:268`); the DNS resolver is a
  stub resolver, not a control-plane client;
- hot backend writes through the admin API under `If-Match` (ADR-0019) and
  per-server `ready`/`drain`/`maintenance` (ADR-0018).

It has no Kubernetes API client and no Consul client (`grep -ci kube|consul
Cargo.lock` → 0). A native watch would mean an API client, RBAC, watch loops,
reconnect/backoff and a cache — a control plane inside the dataplane, which
ADR-0027 already refused for xDS.

## Decision

Natively watching Kubernetes `Service`/`Endpoints`/`EndpointSlice` (or Consul
catalog) is **out of scope** (E3). EQProxy keeps discovery at the DNS/hot-config
boundary.

Reasons:

1. **A controller is not a proxy.** The watch, the RBAC identity and the
   reconciliation loop belong to a controller (ingress-nginx itself is one).
   Embedding them would make the proxy depend on a cluster API and its failure
   modes.
2. **The supported path is real and tested.** Cluster DNS resolves Services;
   `resolve_interval` re-resolves them without a restart, and the admin API
   applies endpoint changes atomically with validation.
3. **It scales with the deployment, not with the binary.** A controller or a
   CI job can write the generated config or call the API; the proxy stays a
   single, auditable process.

## Alternative (the "or equivalent" path)

- Point servers at cluster DNS names and set `resolve_interval` to the
  acceptable staleness window (documented per backend).
- Let the deployment's controller render `eqproxy.toml` (or call
  `PUT /api/v1/backends/...`) when Endpoints change; validation and `If-Match`
  keep the swap safe.
- For static sets, list `IP:PORT` servers directly.

## Consequences

- The matrix classifies B-I3 (native watch) as **E3**; B-H13/B-N18/E15 (DNS
  with hot re-resolution) stay **E1/E2** evidence-backed and are not reopened.
- The converter reports ingress Services without resolvable endpoints as a
  gap with a cause, as it does today; it never invents a watch.
- No Kubernetes/Consul dependency enters the binary.

## Revisit triggers

1. The owner ships a separate controller component (outside the proxy binary)
   that watches a cluster and renders configuration or calls the API.
2. DNS-based discovery proves insufficient for a real migration corpus
   (staleness beyond the window) and a bounded watch design is specified.
3. The documentation starts implying native Kubernetes discovery.
