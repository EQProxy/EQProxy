# EQProxy parity matrix — v0.9.1 (reference scope)

- Status: **complete classification** of the reference scope, 2026-11-01.
- References: HAProxy **3.4.6 LTS** (primary) / 3.0.29, nginx **1.27.1**
  (the data plane shipped in the ingress-nginx controller image) and Envoy
  **API v3, v1.34.2** with a static bootstrap.
- Rows: **120** — HAProxy A1–A19, B1–B17, C1–C13, D1–D12 (61); nginx
  B-N1…B-N20 (20); ingress-nginx B-I1…B-I19 (19); Envoy B-E1…B-E20 (20).
- Result: **11 E1 · 88 E2 · 21 E3 · 0 unclassified**. The counts are the
  arithmetic of the tables below; every E3 names its ADR and every E1 names
  the differential case that proves it.

This document is a **scope classification**, not a compatibility announcement:
«compatibility not announced until demonstrated» (SPEC). The maintainer's
local HAProxy table keeps the per-directive test names, limitations and
phases; this public matrix adds the classification and the three other
references.

## Taxonomy

| Class | Meaning | Required evidence |
|---|---|---|
| **E1** | Same observable semantics as the reference | A case in the differential corpus that runs against the reference and EQProxy and passes |
| **E2** | Equivalent with a known, documented difference | A test of the behaviour plus the difference named in this row (and, where applicable, SPEC §13) |
| **E3** | Out of scope by decision | A public ADR plus an alternative |

E1 is never inferred from "it compiles" or from a unit test: only the cases in
[docs/differential-corpus.md](differential-corpus.md), executed by
`differential_shared_semantics_corpus`, count. A row whose shared semantics are
covered but whose full directive set is not keeps the E1 for the covered
semantics and names the uncovered directives in **Diferencia**. A row with no
differential case is E2 even when the implementation looks equivalent: that is
the honest ceiling until the corpus grows.

## Summary

| Reference | Rows | E1 | E2 | E3 |
|---|---:|---:|---:|---:|
| HAProxy 3.4.6 | 61 | 3 | 49 | 9 |
| nginx | 20 | 3 | 15 | 2 |
| ingress-nginx | 19 | 3 | 11 | 5 |
| Envoy API v3 | 20 | 2 | 13 | 5 |
| **Total** | **120** | **11** | **88** | **21** |

## 1. HAProxy 3.4.6 LTS

### A. Networking and protocols

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| A1 | TCP proxy (`mode tcp`) | E2 | `e2e_tcp.rs` (real-socket tunnel, half-close, maxconn, shutdown). Difference: no TLS inside TCP mode; HTTP is `mode = "http"`. |
| A2 | HTTP/1.1 (`mode http`) | E2 | `e2e_http1.rs` (framing, interim 1xx, `Expect`, oversend, CONNECT/Upgrade). Difference: strict rejection of ambiguous framing instead of normalization. |
| A3 | HTTP/2 (h2/h2c) | E2 | `e2e_http2.rs`, `e2e_backend_h2.rs`, `e2e_ws_h2.rs`. Difference: no server push; classic CONNECT over h2 answers `501`. |
| A4 | HTTP/3 to clients | E2 | `e2e_http3.rs` (real quinn/h3 client). Difference: experimental; ALPN, client CA and minimum TLS need a restart on a live QUIC endpoint; no h3 to backends. |
| A5 | HTTP/3 to backends | **E3** | ADR-0047. Alternative: h2 backend or a TCP tunnel to the h3 origin. |
| A6 | IPv4/IPv6 (dual bind) | E2 | `an_ipv6_listener_proxies_to_an_ipv6_backend` (v6 listener + v6 backend + byte-exact echo). Difference: no `v4v6` wildcard keyword; bind the literal family, the OS decides dual-stack. |
| A7 | WebSocket | E2 | `upgrade_handshake_opens_a_bidirectional_tunnel`, `e2e_ws_h2.rs` (RFC 8441, refusal, 501, idle cut). Difference: no WebSocket over h3. |
| A8 | CONNECT tunnels | E2 | `connect_opens_a_raw_tunnel_on_2xx`. Difference: h1 only; a classic CONNECT over h2 is `501`; destination policy is rule-based. |
| A9 | PROXY protocol v1 | E2 | `proxy_protocol_v1`, `proxy_receive_forwards_the_claimed_peer`, allowlist rejections. Difference: receive needs an origin allowlist; trusted-proxy chains are not followed. |
| A10 | PROXY protocol v2 | E2 | `proxy_protocol_v2`, `proxy_receive_accepts_the_v2_header`. Difference: TLVs are parsed and bounded but not exposed to rules. |
| A11 | TLS termination | E2 | `tls_termination_e2e`, `client_garbage_handshake_is_counted_as_tls_failure`. Difference: no OCSP stapling or operator session-ticket knobs; rustls owns resumption. |
| A12 | TLS to backends | E2 | `tls_backend_e2e`, `tls_backend_verification_failure_is_counted_and_rejected`. Difference: Mozilla roots + `ca_file`; `verify = false` is an explicit opt-out. |
| A13 | SNI certificates and routing | E2 | `e2e_sni.rs` (selection, default, hot swap, wildcard validation). Difference: routing by SNI uses the rule engine's host condition, not a `use-server` fetch. |
| A14 | ALPN | E2 | `alpn_negotiation`, `tls_alpn_h2_terminates_a_real_http2_connection`, h3 tests. Difference: no common protocol fails the handshake; `h3` is reserved to the QUIC endpoint. |
| A15 | mTLS | E2 | `mtls_requires_a_certificate_issued_by_the_client_ca` (issue/refusal both ways). Difference: no subject/DN identity mapping into rules; client CA needs a restart on live QUIC. |
| A16 | Connection reuse and pools | E2 | `e2e_pool.rs` (N exchanges = 1 connection; framing error never pooled), `e2e_backend_h2.rs` (shared h2 session). Difference: TCP never reuses; fixed pool policy (ADR-0015). |
| A17 | Per-phase timeouts | E2 | `http1_client_header_timeout_returns_408_and_counts`, `http1_server_header_timeout_returns_504`, `full_queue_rejects_explicitly_and_waiters_time_out`, `ws_silent_tunnel_is_cut_by_the_idle_timeout`. Difference: no `client-fin`/`server-fin` split. |
| A18 | Backpressure and maxconn | E2 | `per_server_maxconn_caps_concurrency_and_answers_503`, `maxconn_rejects_excess_connections_explicitly`, `http3_maxconn_refuses_a_second_quic_connection`. Difference: no global ceiling or `fullconn`; excess is rejected, never queued unbounded. |
| A19 | Streaming and cancellation | E2 | `http1_client_abort_cancels_upstream_promptly` (backend sees the close, no live task). Difference: no full-duplex pause API for HTTP bodies. |

### B. Balancing

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| B1 | `roundrobin` | E2 | `balancer_roundrobin`, `pick_rotates_deterministically_with_equal_weights`. Difference: smooth weighted RR (deterministic); no queue or retry in the picker. |
| B2 | `static-rr` | **E3** | ADR-0047. Alternative: `balance = "roundrobin"` (fixed weights give the same fixed share). |
| B3 | `leastconn` | E2 | `leastconn_routes_new_exchanges_to_the_least_loaded_server`, weight test, unit tie-break tests. Difference: weighted `active / weight`; ties smoothed by SWRR, no random tie-break. |
| B4 | `source` | E2 | `affinity_cookie_and_source_pin_clients_to_a_server`. Difference: expressed as `[backend.*.hash_policy]` `source`, not `balance source`; rendezvous remaps only owned keys. |
| B5 | `uri` / `url_param` / `hdr` hash | E2 | `hash_policy_keeps_a_key_on_its_target_and_only_remaps_owned_keys` + key grammar unit tests. Difference: keys are `source`, `path`, `host`, `cookie:NAME`, `header:NAME`; no `url_param`, `len` or `depth`. |
| B6 | `random` | **E3** | ADR-0047. Alternative: `roundrobin`, `leastconn` or `hash_policy`. |
| B7 | Weights | E2 | `balancer_weights`, leastconn weight test, drained-server test. Difference: `weight` 0..=256 (0 = drained, still checked); backup-tier weights follow B8. |
| B8 | Backup, priority, `allbackups` | E2 | `e2e_phase2b.rs` backup tests, `v08-backup`, `v08-priority`, `v08-allbackups`. Difference: `priority` is an extension; `all_backups` balances the whole tier and is incompatible with a priority ladder. |
| B9 | Cookie persistence | E2 | `affinity_cookie_and_source_pin_clients_to_a_server` (sets the sticky cookie, pins, re-issues on a stale value). Difference: own cookie mode, not `cookie NAME insert/rewrite/prefix`; no `maxidle`/`maxlife`. |
| B10 | Stick-tables | **E3** | ADR-0047. Alternative: per-rule `rate_limit`, `affinity`, or an external store. |
| B11 | Active health checks | E2 | `healthchecks_e2e`, `http_expect_check_marks_5xx_server_down`, `http_check_interops_with_keep_alive_origins`. Difference: TCP and HTTP `expect`; no agent-check, external-check or client-cert check. |
| B12 | Passive detection | E2 | `passive_detection_marks_5xx_server_down` (real traffic feeds the same `fall`/`rise` machine). Difference: no per-error `on-error` matrix (`error-limit`, timeout classes). |
| B13 | UP/DOWN/DRAIN/MAINT | E2 | `drain_skips_new_requests_and_lets_the_in_flight_one_finish`, maintenance tests, `run_admin_plane_drains_a_server_with_the_write_token`. Difference: no operator-set `down`; states are runtime-only, the file is the baseline. |
| B14 | Slow start | E2 | `slow_start_ramps_the_recovered_server_up`. Difference: ramp in the effective weight; maximum 1h (schema gate). |
| B15 | Bounded queues | E2 | `full_queue_rejects_explicitly_and_waiters_time_out`. Difference: `maxqueue` + queue timeout per backend; no `minconn`/`fullconn`. |
| B16 | DNS discovery | E2 | `e2e_dns.rs` (`re_resolution_swaps_the_backend_address_in_hot`, `hung_dns_serves_explicit_503_never_blocks`). Difference: no `server-template` ranges or separate `resolvers` section. |
| B17 | Retries and redispatch | E2 | `retry_saves_idempotent_get_from_dead_server`, `non_idempotent_post_is_never_retried_without_opt_in`, `non_idempotent_opt_in_allows_post_retry`. Difference: connect-failure retry; no full `retry-on` matrix (timeouts, empty reply, 5xx). |

### C. Rules and routing

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| C1 | Typed ACLs and fetches | E2 | `rules_route_by_path_host_method_header_and_ip`, `first_matching_rule_wins_over_later_ones`, rule/condition unit tests. Difference: own typed rule model, not HAProxy `acl` syntax; fetch catalog is a subset. |
| C2 | Converters | **E3** | ADR-0047. Alternative: `rewrite_prefix`, header actions, or a WASM plugin. |
| C3 | Host/path/header/method routing | **E1** | Differential: `c1-prefix`, `c1-boundary`, `c2-host`, `c2-port`, `c2-near`, `c3-method`, `c3-near`, `c4-header`, `c4-near`, `c5-first`, `c5-second`, `c11-regex`, `c11-boundary`. Difference: no `use-server`, no query-string match. |
| C4 | URL and header rewriting | **E1** | Differential: `c8-rewrite`, `h5-set-header`, `h6-del-header`, `h7-add-header`, `h8-response-marker`. Difference: not the full `http-request` action catalog. |
| C5 | Redirects | **E1** | Differential: `c6-redirect` (status + Location). Difference: fixed status set and format, not full `redirect` expressions. |
| C6 | Rate limiting | E2 | `rate_limit_rejects_over_the_limit_explicitly` + config-gate grammar. Difference: per-rule fixed window with own counters, not `sc_*`/stick-table; no cluster-global limit. |
| C7 | Variables and counters | **E3** | ADR-0047. Alternative: `maps`, rule conditions, WASM plugin. |
| C8 | Peers sync | **E3** | ADR-0046/ADR-0047. Alternative: external store; each instance independent. |
| C9 | Error pages and local responses | E2 | `local_response_serves_the_configured_body`, `direct_response_answers_locally_and_keeps_keep_alive`, `e2e_error_pages.rs` (body/file/status/intercept/errorloc). Difference: pages are per listener, not per rule or `http-request return` location. |
| C10 | Maps | E2 | `map_condition_consults_hot_data_and_swaps_in_hot`, `run_hot_maps_swap_under_traffic_without_config_reload`. Difference: values used by `equals` matching only; file-driven updates, no API write. |
| C11 | Compression | E2 | `run_compresses_gzip_and_respects_every_refusal_case`, `run_compresses_brotli_and_prefers_it_over_gzip`, `run_serves_brotli_over_h2c_to_a_real_client`. Difference: gzip and brotli, no zstd; per-rule minimum size and content types. |
| C12 | In-memory HTTP cache | E2 | `e2e_cache.rs` (miss/hit, Vary, ETag, TTL, coalescing, HEAD, bypass, disk restart). Difference: opt-in per listener; no `stale-while-revalidate`, `stale-if-error`, `only-if-cached` or `If-Modified-Since`. |
| C13 | WASM request filter | E2 | `e2e_plugins.rs` (header, denial, fuel, deadline, memory, fail-open, reserved headers, h2). Difference: request head only; no response phase, body or cross-request state; no WASI. |

### D. Operations

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| D1 | Metrics | E2 | `metrics.rs` unit tests, real-traffic counters in `e2e_tcp.rs`, `e2e_admin.rs` fixed series. Difference: label-free counters always on; per-server/rule detail is opt-in under `max_series` (ADR-0026). |
| D2 | Configurable logs | E2 | `access_log_records_parsed_and_unparsed_requests`, h2 access log, `run_refuses_to_start_when_the_access_log_cannot_be_opened`. Difference: JSON/combined, bounded channel; no `log-format` expression language. |
| D3 | Statistics interface | E2 | `GET /api/v1/status` + `/metrics`, `run_admin_plane_serves_status_and_metrics_with_a_bearer_token`. Difference: own JSON shape, not HAProxy CSV. |
| D4 | Runtime API | E2 | `e2e_admin.rs` (15 tests), `run_admin_plane_applies_backend_config_under_traffic`. Difference: HTTP API with bearer/RBAC; no HAProxy Runtime API syntax; maps remain file-driven. |
| D5 | Prometheus exporter | E2 | `metrics_export_is_prometheus_with_a_fixed_series_count`. Difference: fixed label-free series; detail is opt-in. |
| D6 | Data Plane API | E2 | ADR-0019/0025/0029, admin and panel E2E. Difference: own API, no Data Plane API compatibility; optional panel is an API client. |
| D7 | Hot certificate management | E2 | `sni_set_swaps_in_hot_with_the_certificate_reload`, `http3_reload_applies_new_tls_on_the_same_socket`, `v08-certificate-renewal`. Difference: ALPN, client CA and minimum TLS still need a restart on live QUIC. |
| D8 | Hot map updates | E2 | `run_hot_maps_swap_under_traffic_without_config_reload` (100 000 entries, 1 reload, 0 config reloads). Difference: file-driven; no API write; bad update keeps the last good data and counts `map_reload_failures`. |
| D9 | Reload and graceful close | E2 | `run_admin_plane_applies_backend_config_under_traffic`, `e2e_reload.rs` (swap, add/remove/rebind, duplicate refusal), drain test. Difference: no new process, so no metric discontinuity; no master-worker `-x` FD handover. |
| D10 | Multi-instance | **E3** | ADR-0047. Alternative: independent instances behind a load balancer; admin applies one instance. |
| D11 | Reference → EQProxy converter | E2 | `import_corpus.rs` (four sources), `config_import_converts_and_names_what_it_refuses`, `v085-config-import` journey; fuzz target. Difference: static analysis only; no traffic A/B for every converted directive; source certificate paths are not copied. |
| D12 | Binary upgrade | **E3** | ADR-0047. Alternative: graceful rolling restart behind a load balancer; hot config apply covers configuration changes. |

**Edition exclusions.** HAProxy Enterprise capabilities (WAF, bot
management, global/cluster rate limiting, and the rest of the E section of
the maintainer's HAProxy table) are **E3 by the 2026-10-12 user
decision**: parity covers HAProxy OSS only. They are not counted as rows here
because they are not part of the reference edition.

### HAProxy inventory crosswalk (B-H1…B-H16)

The local inventory's 16 rows are the same capabilities as the rows above;
this is the mapping used to classify them without duplicating evidence.

| Inventory | Class | Matrix rows |
|---|---|---|
| B-H1 h2 | E2 | A3 |
| B-H2 h3 | E2 | A4 |
| B-H3 ACLs/headers | E1/E2 | C4 (E1 headers), C1 (E2 fetch subset) |
| B-H4 `use_backend` | E1 | C3 |
| B-H5 balance | E2 | B1, B3, B4, B5 |
| B-H6 health checks | E2 | B11 |
| B-H7 retries/queues | E2 | B15, B17 |
| B-H8 affinity | E2 | B4, B9 |
| B-H9 PROXY | E2 | A9, A10 |
| B-H10 TLS | E2 | A11–A15 |
| B-H11 timeouts | E2 | A17 |
| B-H12 logs/observability | E2 | D1, D2 |
| B-H13 DNS | E2 | B16 |
| B-H14 Unix/source bind | E2 | ADR-0043 (Unix backend, source bind); A6 is IPv6, not Unix |
| B-H15 errors/WebSocket | E1/E2 | C5 (E1 redirect), C9 and A7 (E2) |
| B-H16 minor gaps | E2/E3 | ADR-0046 table (per directive) |

## 2. nginx

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| B-N1 | HTTP/2 and HTTP/3 | E2 | `e2e_http2.rs`, `e2e_http3.rs`, ALPN tests. Difference: h2/h3 to clients; no nginx `http2 on` syntax; h3 is experimental. |
| B-N2 | Virtual hosts (`server_name`) | **E1** | Differential: `c2-host`, `c2-port`, `c2-near`. Difference: exact host in the corpus; wildcard/regex hosts use the rule engine's `host_prefix`/`host_regex`. |
| B-N3 | `location` routing | **E1** | Differential: `c1-prefix`, `c1-boundary`, `c11-regex`, `c11-boundary`. Difference: no `try_files`/`alias` (static serving is E3). |
| B-N4 | Headers and X-Forwarded-For | **E1** | Differential: `h1-xff-append`, `h2-xff-xri-replace`, `h4-xfp`, `h5-set-header`, `h6-del-header`, `h7-add-header`, `h8-response-marker`. Difference: no full nginx variable language. |
| B-N5 | `rewrite`, `return`, `if`, `try_files` | E2 | `rewrite_action_rewrites_target_and_host_for_the_backend`, `redirect_action_answers_locally_with_rendered_location`, `direct_response_answers_locally_and_keeps_keep_alive`. Difference: no `if` blocks or `try_files`; rewrite/return are rule actions. |
| B-N6 | Compression | E2 | `run_compresses_gzip_and_respects_every_refusal_case`, `run_compresses_brotli_and_prefers_it_over_gzip`, `run_serves_brotli_over_h2c_to_a_real_client`. Difference: gzip and brotli, no zstd; per-rule types and minimum size. |
| B-N7 | `proxy_cache` | E2 | `e2e_cache.rs` (13 E2E). Difference: opt-in per listener, semantic parity not directive-compatible; no stale directives. |
| B-N8 | Rate limiting and limits | E2 | `rate_limit_rejects_over_the_limit_explicitly`, `e2e_limits.rs` (headers, body cap, chunked, h2). Difference: fixed-window per rule, no token bucket or `limit_conn`. |
| B-N9 | Authentication | E2 | `auth_basic_rejects_missing_or_wrong_credentials_explicitly`, `auth_subrequest_allows_on_2xx_and_fails_closed`, mTLS test. Difference: no `satisfy`/`auth_request` directive spelling; subrequest is the documented equivalent. |
| B-N10 | Passive vs active health | E2 | Active TCP/HTTP checks; passive `fall`/`rise`. Difference: no `max_fails`/`fail_timeout` per upstream. |
| B-N11 | HTTP health checks (Plus) | E2 | `http_expect_check_marks_5xx_server_down`, keep-alive interop. Difference: no Plus `match` blocks; status/rstring expectations. |
| B-N12 | Balance algorithms | E2 | `balancer_roundrobin`, `leastconn_*`, `hash_policy_*`, slow start. Difference: no `random`; `ip_hash` is the source hash policy. |
| B-N13 | Affinity | E2 | `affinity_cookie_and_source_pin_clients_to_a_server`. Difference: cookie/source modes; no Plus `sticky` directive options. |
| B-N14 | Logs and `error_page` | E2 | Access log tests, `e2e_error_pages.rs` (server-level page, intercept). Difference: JSON process log; error page is listener-scoped, not per `location`. |
| B-N15 | PROXY and Unix sockets | E2 | `e2e_unix.rs`, `proxy_receive_*`, `proxy_protocol_v1/v2`. Difference: Unix backend yes; no Unix listener; PROXY receive needs an allowlist. |
| B-N16 | Advanced TLS | E2 | ADR-0046, `e2e_sni.rs`, mTLS, `min_version`/`max_version`. Difference: no `ssl_ciphers`, `ssl_stapling`, DH params or session-cache knobs. |
| B-N17 | FastCGI/uwsgi/scgi/gRPC | **E3** | ADR-0032/ADR-0038. Alternative: gRPC is proxied as h2; FastCGI/uwsgi/scgi are not gateways. |
| B-N18 | `resolver` | E2 | `e2e_dns.rs` (hot re-resolution, hung DNS 503). Difference: a hostname in `server`; no nginx `resolver`/variable syntax. |
| B-N19 | Buffer and body limits | E2 | `e2e_limits.rs` (`max_headers`, `max_body_bytes`, declared/chunked/h2 caps, 413). Difference: no `proxy_buffering` strategy; limits are listener settings. |
| B-N20 | Minor gaps (static, variables, stream) | **E3** | ADR-0047: static file serving, `geo`/`split_clients`/`set`/`perl_set`/`js_set` variables, `ssl_preread` and `zone` are out of scope. E2 exceptions: `map` is covered by `[maps]` (C10), `include` is manual, `slow_start`/`max_conns` have equivalents, safe retries replace `proxy_next_upstream`, WebSockets work in HTTP mode. |

## 3. ingress-nginx

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| B-I1 | HTTP/2, gRPC and HTTP/3 | E2 | `e2e_http2.rs`, `e2e_grpc.rs`, `e2e_http3.rs`. Difference: gRPC over h2; h3 client-side; no `enable-http2` ConfigMap spelling. |
| B-I2 | Host/path routing | **E1** | Differential: `c1-prefix`, `c1-boundary`, `c2-host`, `c2-port`, `c2-near`, `c3-method`, `c4-header`, `c11-regex`. Difference: `pathType` Prefix/Exact and regex via rules; the default backend is a fallback rule. |
| B-I3 | Service discovery | **E3** | ADR-0044 (no native watch); hot DNS is E2 (`e2e_dns.rs`). Alternative: resolve the Service/Endpoints to addresses and let DNS re-resolve. |
| B-I4 | PROXY and X-Forwarded-* | **E1** | Differential: `h1-xff-append`, `h2-xff-xri-replace`, `h4-xfp`, `h5`–`h8`. Difference: PROXY receive is E2 (allowlist); there is no `use-forwarded-headers` switch — the safe append/replace modes are explicit. |
| B-I5 | Snippets | **E3** | ADR-0042. Alternative: native rules/headers, or a WASM plugin with declared capabilities. |
| B-I6 | Redirects and rewrites | **E1** | Differential: `c6-redirect`, `c8-rewrite` (prefix). Difference: prefix rewrite only; no regex captures or `app-root`. |
| B-I7 | Rate limiting | E2 | `rate_limit_rejects_over_the_limit_explicitly`. Difference: per-rule fixed window; no `global-rate-limit`/memcached. |
| B-I8 | Affinity and sessions | E2 | `affinity_cookie_and_source_pin_clients_to_a_server`. Difference: cookie/source with fixed options; no `session-cookie-*` matrix. |
| B-I9 | Authentication | E2 | `auth_basic_*`, `auth_subrequest_allows_on_2xx_and_fails_closed`, mTLS. Difference: basic, external subrequest and client certs; no `auth-url` annotation spelling. |
| B-I10 | CORS and HSTS | E2 | `cors_preflight_is_local_and_responses_carry_headers`; response header actions. Difference: `allow_origin` only; the full `cors-*` matrix and HSTS helper are not exposed (HSTS is a response header). |
| B-I11 | Canary and weighted traffic | E2 | `split_traffic_shares_requests_by_weight_across_backends`, `canary_by_cookie_and_header_overrides_route_to_the_canary`. Difference: weight/header/cookie; no `ewma` or `chash`. |
| B-I12 | Protocol gateways (GRPC/FCGI/AJP) | **E3** | ADR-0032/ADR-0038 for FCGI/AJP; gRPC is E2 via h2 (`e2e_grpc.rs`). Alternative: gRPC is proxied, not translated. |
| B-I13 | WAF (ModSecurity/CRS) | **E3** | ADR-0040. Alternative: deploy a WAF in front/behind, or a WASM filter for narrow cases. |
| B-I14 | Logs | E2 | Access log tests. Difference: structured JSON per process; no per-Ingress `log-format-upstream` or separate error-log path. |
| B-I15 | Observability | E2 | `metrics_export_is_prometheus_with_a_fixed_series_count`, status endpoint. Difference: no OpenTelemetry/OpenTracing/InfluxDB exporters; bounded cardinality. |
| B-I16 | Advanced TLS | E2 | ADR-0046, `e2e_sni.rs`, mTLS, min/max version. Difference: no `ssl-ciphers`, `ssl-dh-param` or session knobs; `default-ssl-certificate` is the listener cert/key. |
| B-I17 | Buffers, limits and retries | E2 | `e2e_limits.rs`, retry tests. Difference: configurable header/body caps; safe retries; no `proxy-next-upstream` condition matrix. |
| B-I18 | UDP, WebSockets, compression | **E3** | ADR-0037 for UDP. E2: WebSockets (`e2e_http1.rs`, `e2e_ws_h2.rs`) and gzip/brotli; no zstd. |
| B-I19 | Health checks | E2 | Active TCP/HTTP checks (`healthchecks_e2e`, `http_expect_check_marks_5xx_server_down`). Difference: EQProxy checks actively; Kubernetes readiness is not consumed. |

## 4. Envoy API v3 (static bootstrap)

| ID | Capability | Class | Evidence / difference |
|---|---|---|---|
| B-E1 | xDS dynamic configuration | **E3** | ADR-0027 (no xDS bridge); own admin snapshots and file apply. Alternative: regenerate the config and `config apply`, or run a control plane that writes EQProxy snapshots. |
| B-E2 | h2, gRPC, h3 | E2 | `e2e_http2.rs`, `e2e_grpc.rs`, `e2e_http3.rs`. Difference: no h3 to backends; h2 is terminated and spoken to h2 backends. |
| B-E3 | Advanced routing | **E1** | Differential: `c1`–`c5`, `c11`; `split_traffic_*`, `request_mirror_*`, `hash_policy_*`, rewrites. Difference: no `typed_per_filter_config`; mirror and hash are rule features. |
| B-E4 | HTTP/network filters | E2 | Native CORS, rate limit, auth subrequest/basic, cache, compressor, WASM (`e2e_plugins.rs`). Difference: no full filter chain; tap, transcoding, JWT/OAuth2 and other exotic filters are not exposed — use an external service or a WASM plugin. |
| B-E5 | Advanced health checks | E2 | `http_expect_check_marks_5xx_server_down`, passive detection, TCP checks. Difference: no gRPC health check, panic threshold or outlier-ejection matrix; passive 5xx detection is the equivalent. |
| B-E6 | Balance algorithms | E2 | RR/leastconn/hash/slow start. Difference: no RING_HASH/MAGLEV/RANDOM/ORIGINAL_DST or `lb_subset_config`; rendezvous hash covers consistent hashing. |
| B-E7 | Retries and circuit breaking | E2 | Safe retry tests, bounded queue, per-server maxconn. Difference: no retry budget or full `circuit_breakers` matrix. |
| B-E8 | Observability | E2 | Access log, Prometheus, status/admin. Difference: no StatsD/tracing/tap; JSON logs and fixed series. |
| B-E9 | Advanced TLS | E2 | ADR-0025/0035/0046, hot certs, mTLS, SNI. Difference: no SDS or OCSP/SAN validation; secrets rotate through the admin API/file. |
| B-E10 | PROXY protocol | E2 | `proxy_protocol_v1/v2`, `proxy_receive_*`. Difference: receive requires an allowlist; no listener-filter spelling. |
| B-E11 | WebSockets and CONNECT | E2 | `e2e_http1.rs` upgrade/CONNECT, `e2e_ws_h2.rs`. Difference: h1/h2; no h3; classic CONNECT over h2 is 501. |
| B-E12 | Granular timeouts | E2 | `client_header_timeout`, `server_header_timeout`, `idle_timeout`, `gRPC max_stream_duration`. Difference: no per-route timeout or `max_stream_duration` for HTTP/1.1. |
| B-E13 | UDP and QUIC | **E3** | ADR-0037 for UDP; h3 to clients is E2 (`e2e_http3.rs`). Alternative: dedicated UDP load balancer; QUIC only for the proxy's own listeners. |
| B-E14 | Transparent proxying | **E3** | ADR-0039. Alternative: source bind (ADR-0043) or network-level redirection. |
| B-E15 | Dynamic clusters (EDS/DNS) | **E3** | ADR-0044 for native EDS; DNS TTL re-resolution is E2 (`e2e_dns.rs`). Alternative: DNS names as servers. |
| B-E16 | Global rate limiting | E2 | `rate_limit_rejects_over_the_limit_explicitly`. Difference: local fixed-window counters, no external rate-limit service; no cluster-global budget. |
| B-E17 | Header manipulation | **E1** | Differential: `h1-xff-append`, `h2-xff-xri-replace`, `h4-xfp`, `h5-set-header`, `h6-del-header`, `h7-add-header`, `h8-response-marker`. Difference: no `server_header_transformation` toggle; the dataplane stamps `Server: EQProxy` on every response and replaces the origin's value. |
| B-E18 | Custom errors (`local_reply_config`) | E2 | `e2e_error_pages.rs` (body/file, status, intercept, no page). Difference: per-listener pages, literal bodies; no `%RESPONSE_CODE%` templates or mapper ranges. |
| B-E19 | Tuning and overload | E2 | `e2e_limits.rs` (`max_headers`, `max_body_bytes`), maxconn rejection, queue timeout. Difference: no `overload_manager` actions or per-filter buffer limits; fixed 16 KiB read buffers. |
| B-E20 | Lua/WASM extensibility | **E3** | ADR-0041 for Lua; WASM is E2 (`e2e_plugins.rs`, request head only, fuel/epoch/memory bounds). Alternative: native rules or a WASM plugin. |

## 5. What E1 covers, exactly

E1 is claimed only for the cases the differential corpus executes against
**HAProxy 3.4.6**, the **ingress-nginx 1.27.1 data plane** (static config, the
same shape the controller generates) and **Envoy v1.34.2** (static bootstrap).
The corpus is declared in [differential-corpus.md](differential-corpus.md) and
executed by `differential_shared_semantics_corpus`:

```
cargo test -p eqproxy-cli --test differential -- --ignored --nocapture
```

It compares status, `Location` (path + query), declared bodies and declared
response headers after the documented normalization (reason phrase, absolute
Location, comma separator, calibrated client address). It does not compare
timings, connection reuse, TLS parameters or error-page bytes.

## 6. Evidence and limits

- Differential report: [v091-parity-matrix](evidence/v091-parity-matrix.md)
  (command, exit code, binary hash, image digests, case table, report path).
- Unit/E2E evidence: the tests named in each row; the full suite and the
  binary journeys are recorded in `docs/evidence/`.
- ADRs: 0027, 0032, 0037–0047; SPEC §13 for documented differences.
- **E1 is not a claim that every directive of the row is implemented.** The
  **Diferencia** column names what is not; the classification is for the
  shared semantics the corpus proves.
- **No external review** is implied by this matrix. The production verdict
  (box 19, moved to 0.9.2) still depends on the open boxes 12, 14, 16 and 17,
  and on the external review (box 18, 0.9.3).
- The matrix is source-level evidence: it does not benchmark, and it does not
  replace the Docker/multi-architecture journeys.
