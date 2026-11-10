# Auditoría adversarial EQProxy v0.9.4 — informe ejecutivo

Fecha: 2026-11-09
Artefacto auditado: tag anotado `v0.9.4` → commit `24e6dc6b29728b8ce16c197642a58268a27d89bd`
Árbol de trabajo: `ecda55a46f91982842f8bfe87772bf44324df905` (solo documentación de release por encima del tag; código de producto idéntico)
Imagen ya verificada en el corte: `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8`

## Veredicto

v0.9.4 **funciona** en los recorridos ya verificados (tráfico HTTP real a través de
nginx en `linux/amd64` y `linux/arm64`, suite completa verde en el tag), pero **no
está lista para declararse segura ni para cerrar la fase de endurecimiento**. Esta
auditoría encontró:

- **1 hallazgo Alto**: un `304` puede contaminar una entrada de caché compartida
  con `Set-Cookie`, `Cache-Control: private`/`no-store`, cabeceras arbitrarias y un
  `Vary` nuevo que la clave ignora. Rompe de forma silenciosa la garantía central
  del caché («una respuesta con `Set-Cookie` nunca se almacena») y permite servir
  la misma cookie a otro cliente.
- **2 hallazgos Medios**: el cliente `Cache-Control: max-age=0` no fuerza
  revalidación (recarga del navegador sirve una entrada vieja), y los trailers de
  una respuesta `chunked` se acumulan sin límite en memoria en la ruta de
  compresión y en la traducción a HTTP/2.
- **1 hallazgo Medio de aseguramiento**: el target de fuzz `rules` no compila
  (`missing field maps`) y el workspace de fuzz no se ejecuta en CI; la cobertura
  de fuzzing que declara el repositorio no está corriendo.
- **1 hallazgo Bajo**: documentación desactualizada (`KNOWN_GAPS.md` sigue
  describiendo v0.9.2; `README.md` declara 738 tests frente a los 760 verificados).

Ningún hallazgo se ha corregido en esta fase: el encargo era auditar y preparar el
Roadmap v0.9.5. El producto no se ha tocado.

## Por qué importa

El caché es compartido por todos los clientes de un listener. La decisión de
almacenar se toma una vez, con la respuesta `200`, y el `304` posterior no vuelve a
pasar por `storable_vary`. Eso convierte una respuesta que el origen considera
privada —o una respuesta con cookie de sesión— en bytes públicos para el siguiente
cliente anónimo. No hace falta un atacante remoto controlando el proxy: basta un
origen comprometido, un framework que emite `Set-Cookie` en una revalidación o un
cambio de política del origen. El proxy no avisa; sigue sirviendo `X-Cache: HIT`.

Los trailers son el otro extremo: cada línea está limitada a 8 KiB, pero no hay
límite de número ni de bytes totales. Un upstream hostil o defectuoso puede enviar
trailers indefinidamente y hacer crecer el `Vec`/`HeaderMap` del proxy hasta agotar
la memoria. La ruta afectada es la compresión de respuestas `chunked` y el relay
h2; no es un fallo del parser de framing, es un límite que falta.

## Hallazgos

| ID | Severidad | Título | Evidencia |
|---|---|---|---|
| AUD-01 | Alta | Un `304` contamina la entrada compartida: `Set-Cookie`, `private`, cabeceras arbitrarias y `Vary` ignorado | E2E real, 2 asserts de fuga pasando |
| AUD-02 | Media | `Cache-Control: max-age=0` del cliente no fuerza revalidación | E2E real, `X-Cache: HIT` sin dial |
| AUD-03 | Media | Trailers sin límite de cantidad/bytes en compresión y h2 | Revisión de código; medición de memoria **NOT EXECUTED** |
| AUD-04 | Media (aseguramiento) | El target `rules` del fuzz no compila; fuzz fuera de CI | `cargo check` exit 101 |
| AUD-05 | Baja | Deriva documental: `KNOWN_GAPS.md` en v0.9.2, README con 738 tests | Lectura del árbol |

Las fichas completas, con reproducción y criterio de aceptación, están en
[`findings.md`](findings.md). El plan de corrección está en
[`roadmap-v0.9.5.md`](roadmap-v0.9.5.md).

## Alcance y método

Se revisaron, con lectura de código y pruebas reales cuando fue posible:

- parser y framing HTTP/1.1, trailers, dechunk, M-01;
- caché en memoria y en disco, revalidación `304`, `Vary`, compresión al servir;
- traducción HTTP/2 ↔ HTTP/1.1 y trailers h2;
- HTTP/3 (solo lectura; sigue experimental y sin E2E adversarial);
- routing/reglas, TLS/SNI, admin/panel, auth/plugin, importadores y validación de
  configuración;
- fuzz targets y su integración en CI;
- coherencia de la documentación de release.

No se repitió el E2E de tráfico ya ejecutado contra la imagen `0.9.4` en
`netcup-bench` (amd64) y `oracle-quasar` (arm64); se cita como evidencia previa.
No se ejecutaron Veyra, Rust Audit, CodeGraph ni Docker en esta fase documental:
son **NOT EXECUTED** en este incremento y vuelven a ser obligatorios en el
incremento de corrección v0.9.5.

## Qué está verificado y qué no

Verificado en esta auditoría:

- el `304` contamina la entrada y se sirve a otro cliente anónimo (test E2E con
  sockets y origen real, 2/2 passing);
- `max-age=0` no revalida (mismo harness);
- el target `rules` del fuzz no compila (exit 101).

No verificado (no se presenta como pasado):

- consumo real de memoria con trailers ilimitados (mecanismo confirmado por
  código; falta medición con RSS y un upstream real);
- HTTP/3 adversarial (T19 del THREAT_MODEL sigue sin ejecutar, como ya declara
  `KNOWN_GAPS.md`);
- WebSocket, TLS a fondo, DNS, RBAC del admin más allá de la lectura, importadores
  con corpus malicioso;
- comportamiento bajo carga y memoria a largo plazo.

## Decisión recomendada

No publicar v0.9.5 como «segura» hasta cerrar AUD-01, AUD-02 y AUD-03 con sus
pruebas de regresión y repetir los gates de release. AUD-04 y AUD-05 no bloquean
el binario, pero sí bloquean la afirmación de que el proceso de verificación está
completo.
