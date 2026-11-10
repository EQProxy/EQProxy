# Matriz de cobertura — auditoría adversarial 2026-11-09

Leyenda de resultado: **CONF** = hallazgo confirmado; **NEG** = sin hallazgos
en esta campaña (no implica “seguro”); **HYP** = hipótesis; **PARCIAL** =
revisión/campaña incompleta; **NO** = no ejecutado.

| Superficie | Archivos / rutas | Método | Herramientas / casos | Resultado | Límites / pendiente |
|---|---|---|---|---|---|
| Request line, headers, obs-fold, CR/LF | `eqproxy-net/src/http1.rs:242-299,836-968` | Revisión + fuzz + propiedades + E2E | libFuzzer `framing` (1.042.071 ejec., 121 s); `adversarial_properties` P1/P2 (20.000 mutaciones c/u); E2E S3/S4/S6 | **NEG** | Fuzz de 120 s; sin Miri |
| Framing de petición CL/TE, duplicados, Host | `http1.rs:306-414` | Revisión + E2E | E2E S3 (CL+TE, CL dup, TE no-chunked, espacio antes de `:`) y S6 | **NEG** (todos `400`, 0 smuggled) | Sin variantes HTTP/1.0 exhaustivas |
| Normalización de target/host, traversal | `http1.rs:456-638` | Revisión + propiedades + E2E | P2 (idempotencia, 20.000); E2E S5 | **NEG** | Sin IDN/Unicode exhaustivo |
| Framing de respuesta, 1xx/204/304/HEAD | `http1.rs:973-1068` | Revisión + E2E | E2E S9 | **CONF ADV-01** | Variantes CL en 1xx/204 no explotadas por separado |
| Pool / keep-alive / `peer_gone` | `http1.rs:2052-2058,4911-4929` | Revisión + E2E | E2E S9 | **CONF ADV-01** | Carga/concurrencia no medida |
| Chunking y trailers | `http1.rs:1150-1300` | Revisión | — | **PARCIAL** | Sin campaña dedicada de trailers; AUD-03 previo |
| `Expect: 100-continue`, 1xx | `http1.rs:909-925,3587-3650` | Revisión | — | **NEG parcial** | Sin E2E de 100-continue |
| `HEAD` servido desde caché | `http1.rs:2772+, cache.rs` | E2E | S8 | **NEG** | Sin HEAD con `If-None-Match` E2E |
| `CONNECT` y upgrade | `http1.rs:865-953,4965+` | Revisión | — | **PARCIAL** | Sin túnel WebSocket E2E |
| Caché: lookup, TTL, Vary | `cache.rs:382-461,1180-1402` | Revisión + propiedades + E2E | P3/P4/P5/P7/P10; E2E S1/S2/S7 | **NEG** (AUD-01/02 resisten) | Sin disco E2E ni LRU a presión |
| Caché: refresh 304 | `cache.rs:510-642`; `http1.rs:3549-3583` | Revisión + propiedades + E2E | P9; E2E S1 | **NEG** | Sin 304 con `Vary: *` E2E (sí propiedad) |
| Caché: lock/Vary concurrente | `cache.rs:440-505` | Revisión + propiedades | P8 | **NEG parcial** | Sin prueba de carrera real multi-cliente |
| Traducción h2↔h1 | `eqproxy-net/src/http2.rs` | Revisión + E2E | curl h2c S10/S11/S12 | **CONF ADV-02** | Sin h2 con TLS/ALPN ni h3 |
| Traducción h3 | `eqproxy-net/src/http3.rs` | Revisión | — | **NO** | Pendiente |
| PROXY protocol v1/v2 | `eqproxy-net/src/proxy_proto.rs` | Revisión | — | **NEG parcial** | Sin E2E propio en esta campaña |
| Auth básica / admin / panel | `auth.rs`, `admin.rs`, `panel.rs` | Revisión | — | **PARCIAL** | Sin campaña de autorización |
| TLS/ALPN/SNI/mTLS | `tls.rs`, `proxy.rs` | Revisión | — | **NO** | Sin laboratorio TLS |
| Importador HAProxy | `eqproxy-config/src/import/haproxy.rs` | Revisión + fuzz | `config_import` 1.080.684 ejec. | **HYP-01** | Sin diferencial contra HAProxy real |
| Importador NGINX | `eqproxy-config/src/import/nginx.rs` | Revisión + fuzz | idem | **NEG parcial** | Sin diferencial contra NGINX real |
| Importadores Ingress/Envoy | `import/{ingress,envoy}.rs` | Fuzz | idem | **NEG parcial** | Sin corpus semántico |
| Reglas de routing | `eqproxy-core/src/rules.rs` | Fuzz + revisión | `rules` 463.336 ejec. (target reparado) | **NEG** | Fuzz de 120 s |
| Límites y recursos | `http1.rs` limits, `proxy.rs` | Revisión | — | **PARCIAL** | Sin slowloris/estrés propio |
| Compresión | `compression.rs` | Revisión | — | **NEG parcial** | Sin E2E de bombas/ratio |
| WebSocket / gRPC | `ws.rs`, `grpc.rs` | Revisión | — | **NO** | Pendiente |
| WASM plugin | `plugin.rs` | Revisión | — | **NO** | Pendiente |
| Dependencias/`unsafe` | workspace | Revisión de configuración | `#![forbid(unsafe_code)]` en `eqproxy-net` | **NEG parcial** | Sin `cargo-audit`/`cargo-deny` nuevos |
| Docs vs código | `THREAT_MODEL.md`, README, ADR-0034 | Revisión | — | **NEG** (deriva menor) | T01 aún dice “fuzzing pending” |

## Campañas y presupuesto

| Campaña | Duración | Casos | Crashes | Log |
|---|---|---|---|---|
| `framing` | 121 s | 1.042.071 | 0 | `fuzz-framing.log` |
| `config_import` | 121 s | 1.080.684 | 0 | `fuzz-config-import.log` |
| `rules` | 121 s | 463.336 | 0 | `fuzz-rules.log` |
| Propiedades | 0,14 s | 11 tests; 20.000 mutaciones en P1/P2 | 0 | `property-tests.log` |
| E2E | ~20 s | 13 escenarios | 3 fallos = 2 causas | `lab-run.log` |

## Lo que esta matriz **no** dice

- Un **NEG** significa “no se encontró nada en este método y presupuesto”, no
  que la superficie sea segura.
- No se cubrieron Linux nativo, TLS, h3, WASM, gRPC/WebSocket ni carga.
- El fuzzing no sustituye una revisión semántica ni un diferencial contra los
  productos originales de los importadores.
