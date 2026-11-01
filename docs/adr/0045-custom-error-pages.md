# ADR-0045: Custom error pages — per-listener pages, local or intercepting

- State: **Accepted** (2026-11-01, roadmap v0.9.1, boxes B-H15 · B-E18 · B-N14)
- Date: 2026-11-01
- Related: [ADR-0043](0043-unix-sockets-and-source-bind.md),
  [ADR-0032](0032-fastcgi-uwsgi-scgi-decision.md);
  HAProxy `errorfile` (local-only: `ROADMAP EQPROXY/haproxy.md`),
  nginx `error_page` / `proxy_intercept_errors`
  (local-only: `ROADMAP EQPROXY/nginx.md`), Envoy `local_reply_config`
  (local-only: `ROADMAP EQPROXY/envoy.md`)

## Context

All three references let an operator replace the body of an error response:

- HAProxy: `errorfile <code> <file>`, where the file is a **complete HTTP
  response** (status line, headers, body), usable in `defaults`, a frontend or
  a backend.
- nginx: `error_page <codes…> [=status] <uri>`, server- or location-scoped;
  `proxy_intercept_errors on` decides whether an upstream response is replaced
  too. ingress-nginx exposes the same idea as `custom-http-errors`, with the
  body coming from the controller's default backend.
- Envoy: `local_reply_config.mappers[]` in the HTTP connection manager, with a
  `status_code` or a status filter, and a body (`inline_string`, `filename` or
  a `body_format`).

Before this decision, EQProxy only had fixed minimal errors (no body,
`content-length: 0`) plus the single `local_response` class for "no server
available". That is a real migration blocker: an operator moving a maintenance
page or a branded 503 has nowhere to put it.

The product has one listener per address and no per-rule error scope; nginx's
location scope and Envoy's per-route mappers would need a second routing
concept inside the error path. This ADR fixes the scope at the listener,
matching how the configuration, the metrics and the TLS/compression features
are already organised.

## Decision

1. **A listener owns its pages.** `[[listener.<name>.error_pages]]` declares
   `codes` (one or more, 400-599), exactly one of `file` (resolved against the
   configuration file's directory) or `body`, an optional `status`
   (200-599, never 204/304), an optional `content_type` and `intercept`.
   A code may appear in only one page per listener.
2. **Two classes, one flag.** A page without `intercept` replaces only statuses
   EQProxy generated itself (connect failure, timeout, no server, rejected
   request, rate limit). `intercept = true` additionally replaces a matching
   **upstream** response. An intercepted upstream connection is closed, never
   pooled: its body was not read.
3. **`format = "raw"` keeps HAProxy files working.** The file is parsed at load
   (status line, `Content-Type`, `Content-Length` consistency, no
   `Transfer-Encoding`); a page whose status line is not in `codes`, or whose
   length disagrees with its body, rejects the configuration. `status` and
   `intercept` are refused on a raw page because its status line is fixed.
4. **Resolve once, serve from memory.** Files are read, bounded
   (`MAX_ERROR_PAGE_BYTES`, 64 KiB) and parsed when the snapshot is resolved,
   like certificates. A missing or malformed file fails startup/reload with
   listener, field and path. The hot path only copies bytes.
5. **Observable.** `eqproxy_error_pages_served_total` counts every served page
   (local and intercepted); the periodic metrics line carries `error_pages=N`.
6. **Converters translate what is expressible, and say what is not:**
   - HAProxy `errorfile` (defaults/frontend) → `format = "raw"` pages;
     backend-scoped `errorfile` is a review pointing at the listener.
   - nginx server-level `error_page` (+`proxy_intercept_errors on`) → a page,
     with `=NNN` as `status` and the URI's leading `/` removed because the TOML
     path is configuration-relative (the report says so). Location-scoped
     `error_page` is a review: widening it to the listener would change which
     requests it covers.
   - Envoy `local_reply_config` mappers with `status_code` or an equality
     `status_code_filter`, and a body in `inline_string`, `filename` or a
     substitution-free `text_format` → a page. Ranges, other filters,
     `json_format`, substitutions and `headers_to_add` are reviews.
   - ingress-nginx `custom-http-errors` is a review that names the codes and
     the missing body: the content lives in the default backend, which the
     manifest does not carry.

## Out of scope

- Per-rule or per-location pages; that needs a scoped error model.
- Template substitution (`%RESPONSE_CODE%`, nginx variables) in page bodies:
  a page is literal bytes.
- Replacing the auth gate's 401/403: it carries `WWW-Authenticate` and other
  protocol headers, which a generic page would lose. Pages cover locally
  generated rejections and (with `intercept`) origin responses.
- Serving a page for statuses below 400 (redirects are configured with
  `redirect`/`direct_response`).
- HTTP/3-specific error handling beyond the shared HTTP path.

## Consequences

- The schema, validation, CLI loader, HTTP/1.1 path (shared with h2 streams),
  metrics, the four converters and `eqproxy.toml.example` learn the feature.
- Validation can only check structure; file existence and raw syntax are
  checked at snapshot resolution, exactly like TLS material.
- An intercepted response loses the origin's status-specific body; the status
  is kept unless `status` overrides it. The access log records the page's
  status for an intercepted exchange and the fired status for a local failure
  (a local page's `status` override changes the wire response only).
- `config import` may generate `error_pages` whose files do not exist on the
  machine that runs `config validate`; that is the same contract as imported
  certificate paths and is reported as such.

## Acceptance criteria (delivered in 0.9.1-b)

- Real HTTP/1.1 listener, real origin, real files: local page, status
  override, `HEAD` without body, intercept with a real 503 and with an explicit
  connection drop, `format = "raw"` from a CRLF response file, and the metrics
  counter. See [v091-error-pages](../evidence/v091-error-pages.md).
- Validation rejects TCP mode, duplicate codes, raw + body, raw + intercept,
  out-of-range codes and 204/304 pages; a missing file fails startup before the
  socket is served.
- HAProxy, nginx and Envoy sources convert to pages that pass
  `eqproxy config validate`.

## Revisit triggers

1. A real migration needs per-rule or per-location pages.
2. Template substitutions or content negotiation become a requirement.
3. The default backend of ingress-nginx must be reusable as the page body.
