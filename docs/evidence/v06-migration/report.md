# Migration report — real source configs vs the EQProxy translation

- date (UTC epoch): 1794586357
- eqproxy binary: /Volumes/Lexar/Cursor/EQProxy/target/debug/eqproxy (sha256 364127c7f130fa03fc22c0a709c0bdfc64291667198d7913eb0a7fab574b8d24)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/haproxy.cfg (sha256 0057980c97e4098a7d9495af74d41c9375b7df8c922345fe6ada476ab6f3f37d)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/nginx.conf (sha256 fcf1c5141b84911e5c5b537d777fb8e877d9faa1c2050d7a0d24474d0dcf08b0)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/envoy.yaml (sha256 1c9390d74a25bd2de7641ccc116b1c9a022b345e75b85174b43d63c00e16e17b)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/ingress.yaml (sha256 41a7cefe3cfe41692ccb47179e61d1d621428fb648b2e3c04cc660eedd6b38f4)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/ingress-nginx-generated.conf (sha256 dc6ede0c1fdae99f39b6a60a83137e1868f56db3d58277d92f0ff0f80615959e)
- /Volumes/Lexar/Cursor/EQProxy/eqproxy-cli/../docs/migration/eqproxy.toml (sha256 08d5eb39119ea75ff4f08e942454fa23b0d4e8ebe1939c072396cd1cf6e830bc)
- haproxy: haproxy:3.4.6 (sha256:e273915691a8eab3f105bf3e39fd4c29a4ecf334e33421903b5589d25e14db26)
- nginx: nginx:alpine (sha256:785835ea65db4bba6f021acc0f3be91b0fef76a53fe41f8588c6698d2aed0606)
- ingress-nginx data plane: registry.k8s.io/ingress-nginx/controller:v1.13.0 (sha256:dc75a7baec7a3b827a5d7ab0acd10ab507904c7dad692365b3e3b596eca1afd2)
- envoy: envoyproxy/envoy:v1.34.2 (sha256:daca6a3f353ba289cc786d2162d13d4ec2b16d921c6c3f2fc57ce6f7900ab3d9)
- backends: nginx:alpine (sha256:785835ea65db4bba6f021acc0f3be91b0fef76a53fe41f8588c6698d2aed0606)
- tls origin (case m14): nginx:alpine (sha256:785835ea65db4bba6f021acc0f3be91b0fef76a53fe41f8588c6698d2aed0606) on 172.24.0.16:8443, self-signed
- substitutions on the EQProxy translation (declared): free local listen
  address, and the five pools plus the TLS origin reached through their
  published host ports instead of the fixed container addresses.

| case | semantics | eqproxy | haproxy | nginx | ingress-nginx | envoy |
|---|---|---|---|---|---|---|
| m1-root | storefront vhost reaches the web pool | OK 200 "web /" | OK 200 "web /" | OK 200 "web /" | OK 200 "web /" | OK 200 "web /" |
| m2-canary-cookie | canary-by-cookie marks the storefront canary | OK 200 "canary /api/x" | OK 200 "canary /api/x" | OK 200 "canary /api/x" | OK 200 "canary /api/x" | OK 200 "canary /api/x" |
| m3-canary-header | canary-by-header marks the storefront canary | OK 200 "canary /" | OK 200 "canary /" | OK 200 "canary /" | OK 200 "canary /" | OK 200 "canary /" |
| m4-api-post | API vhost reaches the api pool | OK 200 "api /api/x" | OK 200 "api /api/x" | OK 200 "api /api/x" | OK 200 "api /api/x" | OK 200 "api /api/x" |
| m5-admin-post | method-scoped routing sends POST /admin to the admin pool | OK 200 "admin /admin/x" | OK 200 "admin /admin/x" | OK 200 "admin /admin/x" | OK 200 "admin /admin/x" | OK 200 "admin /admin/x" |
| m6-admin-get | a non-POST /admin request stays on the storefront pool | OK 200 "web /admin/x" | OK 200 "web /admin/x" | OK 200 "web /admin/x" | OK 200 "web /admin/x" | OK 200 "web /admin/x" |
| m7-reports | the /reports prefix reaches the api pool on the shop vhost | OK 200 "api /reports/x" | OK 200 "api /reports/x" | OK 200 "api /reports/x" | OK 200 "api /reports/x" | OK 200 "api /reports/x" |
| m8-rewrite | rewrite-target /api/v1/<rest> -> /api/<rest> | OK 200 "api /api/users" | OK 200 "api /api/users" | OK 200 "api /api/users" | OK 200 "api /api/users" | OK 200 "api /api/users" |
| m9-json | regex routing of *.json under /api/ | OK 200 "api /api/a.json" | OK 200 "api /api/a.json" | OK 200 "api /api/a.json" | OK 200 "api /api/a.json" | OK 200 "api /api/a.json" |
| m10-redirect | the legacy path redirects to /blog | OK 301 -> /blog "" | OK 301 -> /blog "" | OK 301 -> /blog "<html>\r\n<head><title>301 Moved Permanently</title></head>\r\n<body>\r\n<center><h1>301 Moved Permanently</h1></center>\r\n<hr><center>nginx/1.31.8</center>\r\n</body>\r\n</html>\r\n" | OK 301 -> /blog "<html>\r\n<head><title>301 Moved Permanently</title></head>\r\n<body>\r\n<center><h1>301 Moved Permanently</h1></center>\r\n<hr><center>nginx/1.27.1</center>\r\n</body>\r\n</html>\r\n" | OK 301 -> /blog "" |
| m11-healthz | the health answer is local (no backend contacted) | OK 200 "ok" | OK 200 "ok" | OK 200 "ok" | OK 200 "ok" | OK 200 "ok" |
| m12-default-host | an unknown host reaches the default backend | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" |
| m14-tls-backend | an HTTPS-only origin is reached over TLS, not in cleartext | OK 200 "tls-origin /tls/x" | OK 200 "tls-origin /tls/x" | OK 200 "tls-origin /tls/x" | OK 200 "tls-origin /tls/x" | OK 200 "tls-origin /tls/x" |
| m13-api-fallback | the api vhost falls back for unmatched paths | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" | OK 200 "fallback /x" |

verdict: 14/14 cases route identically across the five implementations (70 checks).
