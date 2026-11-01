# ADR-0041: Lua scripting — not adopted (E3), WASM is the extension path

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-E20)
- Date: 2026-10-31
- Related: [SPEC.md §10](../../SPEC.md) (safe-code policy, WASM as the only
  plugin route), [ADR-0031](0031-wasm-plugin-host.md) (the WASM request-filter
  host), [ADR-0042](0042-snippets-not-adopted.md) (nginx snippets),
  envoy.md B-E20 (local-only: `ROADMAP EQPROXY/envoy.md`)

## Context

Envoy's Lua filter runs user-supplied Lua scripts at request and response
hooks. nginx reaches the same space through OpenResty and, less directly,
through snippets (ADR-0042).

EQProxy forbids `unsafe` in its own code and discarded Lua in the SPEC: a Lua
runtime (`mlua` or equivalent) is a C FFI dependency with a large unsafe
surface, a VM/JIT that executes code outside Rust's guarantees, and its own
sandbox story. The shipped
extension mechanism is the WASM request filter (ADR-0031): fuel/epoch,
memory limits and deny-by-default capabilities, with the ABI documented in
that ADR. No Lua runtime is in the dependency graph.

## Decision

Lua scripting is **out of scope** (E3). EQProxy does not embed a Lua runtime,
does not accept Lua source in configuration and does not translate Envoy Lua
filters.

Reasons:

1. **The safe-code policy is a product constraint, not a preference.** A C FFI
   runtime with a JIT would be the largest unsafe surface in the product.
2. **A supported extension route already exists.** The WASM host covers the
   use cases (header/redirect/deny decisions) under explicit budgets and
   capabilities.
3. **Lua scripts are not portable as text anyway.** They call Envoy-specific
   objects (`request_handle`, `stream_info`, dynamic metadata); a mechanical
   translation would be a new runtime, not a conversion.

## Alternative (the "or equivalent" path)

- Port the logic to a WASM plugin against ABI v1 (ADR-0031); the plugin is
  sandboxed and its limits are enforced by the host.
- For simple cases, native rules already cover header manipulation, redirects,
  auth and local responses.

## Consequences

- The matrix classifies B-E20 as **E3** with this ADR.
- The converter keeps Lua filters and Lua-bearing snippets as explicit gaps
  ("WASM plugin" is the documented substitute), never as converted.
- No Lua dependency, interpreter or JIT enters the binary.

## Revisit triggers

1. The safe-code policy changes (a new ADR) and a sandboxed, audited Lua
   runtime meets the same guarantees as WASM.
2. A migration corpus shows Lua logic that cannot be expressed as a WASM
   plugin or native rules.
3. The documentation starts implying Lua compatibility.
