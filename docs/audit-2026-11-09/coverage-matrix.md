# Matriz de cobertura — auditoría v0.9.4

Leyenda: **E2E** = binario/proxy real con sockets y origen real; **Código** =
lectura y trazado del camino real; **Unit** = tests existentes del crate;
**NOT EXECUTED** = no se ejecutó y no se cuenta como pasado.

| Superficie | Qué se miró | Método | Resultado | Riesgo residual |
|---|---|---|---|---|
| Framing HTTP/1.1 (CL, TE, chunked, 1.0) | `http1.rs:300-410`, `1021-1040`, M-01 y tests del tag | Código + Unit (suite del tag) | Sin hallazgo nuevo; M-01 cerrado en `24e6dc6` | Smuggling por combinaciones no cubiertas; el parser está fuzzeado, pero el target `framing` no se re-ejecutó aquí |
| Trailers chunked | `http1.rs:1254-1270`, `1306-1325`, `1411-1429`, `1555-1556` | Código | **AUD-03**: `Vec` sin límite en `pump_chunked_plain` | Consumo de memoria/CPU con upstream hostil |
| Caché: decisión de almacenar | `cache.rs:903-940`, `1105-1138` | Código + E2E | Correcta en `200`; **AUD-01** la salta en `304` | Cookie/private/Vary en revalidación |
| Caché: refresh `304` | `cache.rs:435-512`, `http1.rs:3549-3566` | E2E real | **AUD-01 confirmado**: `set-cookie`, `private`, `x-audit-304` y `Vary` llegan a otro cliente | Contaminación de entrada compartida |
| Caché: directivas del cliente | `cache.rs:903-940` | E2E real | **AUD-02 confirmado**: `max-age=0` sirve `HIT` | Recarga del navegador devuelve contenido viejo |
| Caché: lock, variantes, disco | `cache.rs:320-430`, `552-700`; `e2e_cache.rs` | Unit + E2E previos | Sin hallazgo nuevo | Carreras no reproducidas; no se hizo stress |
| Compresión al servir | `http1.rs:2827-2916`, `compression.rs` | Código + Unit | Sin hallazgo nuevo además de AUD-03 | BREACH fuera de alcance; no se midió CPU |
| HTTP/2: traducción de cabeceras | `http2.rs:360-430` | Código + Unit | Filtra hop-by-hop y reconstruye framing | `content-length` con END_STREAM corto no probado E2E |
| HTTP/2: trailers | `http2.rs:643-666` | Código | **AUD-03**: `HeaderMap` sin límite | Memoria/HPACK con upstream hostil |
| HTTP/3 | `http3.rs:1-60` | Código | Experimental, límites declarados | T19 no ejecutado; no se declara seguro |
| Routing/reglas | `eqproxy-core/src/rules.rs:1-70`, `300-430` | Código + Unit | Host normalizado, regex lineal, sin hallazgo nuevo | No se hizo corpus adversarial nuevo |
| TLS/SNI | `tls.rs:1-100` | Código | Verificación por defecto, opt-out explícito | No se auditó el resolver SNI ni la rotación en vivo |
| Admin/panel | `admin.rs:1-120`, `1949-1960`; `panel.rs:1-30`, `222-340` | Código | Token constante, `Origin` rechazado, CSRF y escape presentes | No se hizo pentest de RBAC/sesiones |
| Auth/plugin | `auth.rs:1-240` | Código | Sin hallazgo nuevo | Subrequest y allowlist de cabeceras no probados E2E |
| Importadores | `eqproxy-config/src/import/*` | Código | Sin hallazgo nuevo; el conversor no lee archivos | No se fuzzeó el corpus |
| Validación de configuración | `eqproxy-config/src/validate.rs` | Código | Límites y allowlists presentes | No se recorrió el espacio completo |
| Fuzz | `fuzz/Cargo.toml`, `fuzz_targets/*` | `cargo check` | **AUD-04**: `rules` no compila (exit 101) | Cobertura de fuzzing real inexistente |
| CI | `.github/workflows/*.yml` | Búsqueda | No hay job de fuzz | Regresiones de fuzz no detectadas |
| Documentación | `README.md:10`, `KNOWN_GAPS.md:5` | Lectura | **AUD-05**: conteo y versión desactualizados | Confianza documental |

## Cobertura no ejecutada

- **Carga y memoria sostenida**: sin prueba de trailers ilimitados con medición de
  RSS; AUD-03 queda como mecanismo confirmado, no como cifra.
- **HTTP/3 adversarial**: no ejecutado; sigue experimental.
- **WebSocket**: no se probó el túnel ni el handshake con un servidor real.
- **TLS**: no se probó SNI múltiple, rotación en caliente ni mTLS.
- **Admin**: no se probó escalado de rol, sesiones concurrentes ni el socket Unix
  con credenciales reales.
- **Importadores**: sin corpus malicioso; solo lectura.
- **Fuzzing**: no se ejecutó ninguna campaña; el target `rules` ni siquiera
  compila.

## Conclusión de cobertura

La auditoría cubre los caminos donde ya había una sospecha concreta y confirma
tres defectos de comportamiento. No es una auditoría exhaustiva: las áreas marcadas
como «sin hallazgo nuevo» significan «no se encontró uno con el método aplicado»,
no «están libres de defectos».
