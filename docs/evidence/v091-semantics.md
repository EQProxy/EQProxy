# v0.9.1-d — Semántica, límites y qué NO hace (H7 · H8)

**Objetivo.** Cerrar la casilla 13 del roadmap 0.9.1: la decisión sobre UDP
(H7) ya estaba en [ADR-0037](../adr/0037-udp-proxying-not-adopted.md), pero
faltaba la documentación por feature que H8 pide —«semántica exacta + límites +
qué NO hace»— y que el ADR-0037 exige publicar donde se describe la feature,
no solo en el ADR. Este incremento no cambia comportamiento: es la referencia
pública que faltaba.

## 1. Qué se entrega

[docs/semantics.md](../semantics.md): 17 secciones que cubren TCP, HTTP/1.1,
HTTP/2, HTTP/3/QUIC, TLS/SNI/mTLS, reglas y filtros nativos, balanceo/salud/
colas, pool/timeouts/recursos, caché, compresión, WebSocket/CONNECT, páginas de
error, plano admin/recarga, plugins WASM, importación de configuraciones y
UDP/no-objetivos. Cada sección tiene tres bloques: **Semántica**, **Límites** y
**Qué NO hace**.

La exclusión UDP queda en dos sitios: la sección 16 de la página y el
[ADR-0037](../adr/0037-udp-proxying-not-adopted.md). La alternativa operativa
(HAProxy/balanceador dedicado; syslog por TCP con `mode = "tcp"`) está escrita
en ambos.

## 2. De dónde sale cada afirmación

No se inventaron cifras ni límites. Las fuentes son:

| Fuente | Qué aporta |
|---|---|
| [SPEC §4](../../SPEC.md#4-non-goals-explicit) | no-objetivos (WAF, sintaxis completa, ediciones comerciales, atomicidad multi-instancia) |
| [SPEC §7](../../SPEC.md#7-architecture) | snapshots, presupuestos de recursos, TLS/balanceo/salud, pool keep-alive, error pages |
| [SPEC §13](../../SPEC.md#13-semantic-difference-log-vs-the-reference-proxy) | diferencias semánticas ya aceptadas (framing, h2, TLS, balanceo, PROXY…) |
| [ADR-0010](../adr/0010-response-compression.md) | compresión: negociación, elegibilidad y límites |
| [ADR-0015](../adr/0015-backend-idle-pool-sizing.md) | tope de idle del pool (64..4096, 60 s) |
| [ADR-0025](../adr/0025-certificate-renewal-without-restart.md) | rotación de certificado y lo que exige reinicio en QUIC |
| [ADR-0031](../adr/0031-wasm-plugin-host.md) | ABI, imports, fuel/deadline/memoria y fail-closed |
| [ADR-0034](../adr/0034-http-response-cache.md) | claves, bypass, TTL, memoria, disco y fuera-de-v1 |
| [ADR-0035](../adr/0035-hot-secrets-and-drift-runbook.md), [ADR-0046](../adr/0046-tls-version-range-and-minor-gaps.md) | cifrados/OCSP/DH/sesión provider-managed y rango de versiones |
| [ADR-0037](../adr/0037-udp-proxying-not-adopted.md) | UDP E3 y alternativa |
| [ADR-0040](../adr/0040-waf-not-adopted.md), [ADR-0041](../adr/0041-lua-not-adopted.md), [ADR-0042](../adr/0042-snippets-not-adopted.md), [ADR-0032](../adr/0032-fastcgi-uwsgi-scgi-decision.md), [ADR-0038](../adr/0038-ajp-gateway-not-adopted.md), [ADR-0039](../adr/0039-transparent-proxying-not-adopted.md), [ADR-0044](../adr/0044-no-native-service-discovery.md) | no-objetivos y alternativas |
| [ADR-0043](../adr/0043-unix-sockets-and-source-bind.md), [ADR-0045](../adr/0045-custom-error-pages.md) | sockets Unix, source bind, páginas de error y `errorloc` |
| [docs/equivalences.md](../equivalences.md) | CORS, ext_authz, rate limit y filtros fuera |
| Tests citados en la página | `tcp_listener_keeps_h2_alpn_for_byte_tunnels`, `parse`/`validate` de reglas, E2E de framing, h2/h3, TLS/SNI, pool, caché, compresión, WS/CONNECT, error pages y plugins |

Las cifras que el SPEC marca como **objetivo de diseño** (por ejemplo, la
memoria por conexión TCP en Linux) se presentan como tales, no como medidas.
Los límites de configuración se copiaron de los tipos y del gate
(`eqproxy-config/src/schema.rs`, `validate.rs`) y de los ADR que los fijan.

## 3. Verificación

- **Enlaces relativos del documento nuevo:** 0 destinos inexistentes
  (comprobación con un script sobre `docs/semantics.md` y sobre esta página).
- **Citas de tests:** `python3 .github/scripts/check-evidence-test-citations.py`
  → exit **0** (`96 documents, 218 cited identifiers, 690 #[test] functions`).
  Los identificadores de test citados en la página existen en el árbol.
- **Alcance:** verificación de documentación. No se ejecutó una E2E nueva
  porque este incremento no toca código; presentarlo como una prueba de
  comportamiento sería teatro. La evidencia de cada comportamiento sigue
  siendo la de su casilla (enlaces en la tabla anterior).

## 4. Límites de esta entrega

- La página es una **referencia de superficie**, no el catálogo exhaustivo de
  cada campo del schema: los valores exactos viven en `eqproxy.toml.example`,
  el schema JSON y el gate. Cuando un límite no está aquí, no se promete.
- No cubre benchmarks ni clasificación E1/E2/E3: eso son las casillas 14–17.
- Las auditorías Veyra/Rust Audit/CodeGraph de este mismo árbol de Rust son las
  de [v091-tls-certification](v091-tls-certification.md) §7.4; este delta es
  solo un `.md` y no las invalida. No se repitieron para no presentar una
  pasada idéntica como evidencia nueva.
