# ADR-0040: WAF / ModSecurity / OWASP CRS — not adopted (E3)

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-I13)
- Date: 2026-10-31
- Related: [SPEC.md §13](../../SPEC.md) (the stated non-goal),
  [THREAT_MODEL.md §7](../../THREAT_MODEL.md) ("EQProxy is not a WAF"),
  ingress-nginx.md B-I13 (local-only: `ROADMAP EQPROXY/ingress-nginx.md`),
  [ADR-0031](0031-wasm-plugin-host.md) (the only in-process extension surface)

## Context

ingress-nginx can enable ModSecurity with the OWASP Core Rule Set
(`enable-modsecurity`, `enable-owasp-core-rules`). EQProxy has no payload
inspection: it parses and validates HTTP framing, routes on host/path/method/
header/cookie/IP, applies header, auth, rate and CORS rules, and forwards the
body untouched. There is no ModSecurity dependency and no CRS loader.

This was already a stated non-goal in SPEC §13 and THREAT_MODEL §7; the
roadmap box asks for the ADR that makes it an E3 classification rather than an
open gap.

## Decision

A WAF (ModSecurity, OWASP CRS or an equivalent rules engine) is **out of
scope** (E3). EQProxy does not inspect request or response payloads for
application vulnerabilities.

Reasons:

1. **Different product with a different failure mode.** A WAF is a rules
   engine with a rule lifecycle (CRS updates, overrides, false positives) and
   a detection contract. Shipping "a WAF" without that lifecycle would give
   false confidence, which this project's verification contract explicitly
   rejects.
2. **Payload inspection conflicts with streaming and limits.** The dataplane
   relays bodies with bounded buffering; full inspection means buffering and
   reassembly with a new class of memory/CPU budgets.
3. **The correct places exist.** A dedicated WAF in front (ModSecurity/CRS,
   Coraza) is auditable on its own; a narrow policy (header allowlists,
   method restrictions, IP rules) is already expressible with native rules.

## Alternative (the "or equivalent" path)

- Put the WAF in front of EQProxy (or behind it, for responses) where its own
  ruleset, logging and update process are configured and reviewed.
- For narrow checks, a **WASM plugin** (ADR-0031) can inspect bounded headers
  and metadata; it is not CRS and must not be described as one.

## Consequences

- The matrix classifies B-I13 as **E3** with this ADR, and SPEC/THREAT_MODEL
  keep saying so.
- The converter keeps `enable-modsecurity` / `enable-owasp-core-rules` as gaps
  with a cause; it never pretends to translate rules.
- No dependency, rules corpus or inspection buffer enters the product.

## Revisit triggers

1. The owner accepts a WAF product surface with its rule lifecycle and a
   conformance corpus (which would be a new ADR, not an extension of this one).
2. A regulation requires in-proxy inspection regardless of deployment.
3. A WASM plugin starts being presented as a WAF in the documentation.
