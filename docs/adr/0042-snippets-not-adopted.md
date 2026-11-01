# ADR-0042: nginx snippets — not adopted (E3), mapped to rules and WASM

- State: **Accepted** (2026-10-31, roadmap v0.9.1, box B-I5 completo)
- Date: 2026-10-31
- Related: ingress-nginx.md B-I5 (local-only: `ROADMAP EQPROXY/ingress-nginx.md`),
  [ADR-0031](0031-wasm-plugin-host.md) (the plugin host),
  [ADR-0041](0041-lua-not-adopted.md) (the other scripting surface),
  [CVE-2021-25742](https://github.com/kubernetes/ingress-nginx/issues/7837)
  (why ingress-nginx disabled snippet annotations by default)

## Context

ingress-nginx exposes `configuration-snippet`, `server-snippet`, `http-snippet`
and `location-snippet`: raw nginx configuration injected into the generated
file. They are an escape hatch, and their history is the reason the project
distrusts them — CVE-2021-25742 was a snippet-injection vulnerability, and
snippet annotations are disabled by default since then.

EQProxy's schema is strict and validated; there is no raw-config escape hatch
and no way to run an nginx directive. The importer already classifies every
snippet annotation as **review**, with the message "snippet de nginx: no se
ejecuta ni se traduce (sustituto: plugin WASM o reglas)" and the same for
`allow-snippet-annotations`. Nothing is dropped silently.

## Decision

nginx snippets are **out of scope** as raw configuration (E3). EQProxy never
executes, embeds or emits nginx configuration.

The supported mapping for snippet uses:

| Snippet use | EQProxy equivalent |
|---|---|
| Add/remove request or response headers | `[[listener.*.rules]].set_headers` / response header rules |
| Redirect or return a fixed answer | `redirect` / `direct_response` rule actions |
| Deny by IP, method, path or header | rule conditions with a `direct_response` |
| Fix a URI or upstream host | `rewrite.path_prefix_rewrite` / `host_rewrite` |
| Arbitrary logic (HMAC, custom auth, body checks) | **WASM plugin** (ADR-0031) |
| Anything else | not supported; the migration report says so |

Reasons:

1. **A snippet is arbitrary code with the proxy's privileges.** Supporting it
   would reintroduce exactly the injection class the SPEC forbids.
2. **The native surface covers the common cases.** The table above is the
   contract; the converter's review note points at it.
3. **WASM is the bounded extension route**, with fuel, memory and capability
   limits that raw nginx directives do not have.

## Alternative (the "or equivalent" path)

- Rewrite the snippet with native rules; when that is not possible, implement
  it as a WASM plugin.
- Keep nginx in front for the specific location that needs raw configuration
  and proxy the rest.

## Consequences

- The matrix classifies B-I5 as **E3** for raw snippets, with the mapping table
  above as the "equivalent" path.
- The converter keeps snippets in the **review** bucket with the explicit
  message; it must never convert a snippet into a comment or drop it.
- No raw-config parser, directive evaluator or nginx compatibility layer is
  added.

## Revisit triggers

1. A migration corpus shows a recurring snippet pattern that native rules
   cannot express and a WASM plugin is not acceptable to the owner.
2. A safe, declarative subset is specified with validation and tests (that
   would be a new feature, not a return of raw snippets).
3. The converter starts converting snippets without review.
