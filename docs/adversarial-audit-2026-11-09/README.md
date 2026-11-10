# Auditoría adversarial de EQProxy — 2026-11-09

**Estado:** auditoría parcial ejecutada, con 2 hallazgos confirmados y 1 hipótesis.
**Repositorio:** `https://github.com/EQProxy/EQProxy.git`
**Rama:** `release/v0.9.5`
**Commit:** `ecda55a46f91982842f8bfe87772bf44324df905`
**Árbol:** *dirty* (cambios previos sin commitear de AUD-01/P0-2 en caché; ver §9).
**Fecha:** 2026-11-09.

> Esta carpeta es nueva y deliberadamente distinta de `docs/audit-2026-11-09/`
> (auditoría previa). No se ha modificado ningún fichero de producto para
> “hacer pasar” pruebas. Todo lo nuevo es harness, prueba o documentación.

---

## 1. Resumen ejecutivo

Se auditó el árbol de trabajo real (no solo `HEAD`) con cuatro métodos:
revisión adversarial del código Rust, fuzzing real con `cargo-fuzz`, pruebas de
propiedades deterministas y un laboratorio E2E con el binario `eqproxy`
compilado y upstreams TCP reales.

**Resultado:** la superficie de *request smuggling* del cliente está bien
defendida y los arreglos de caché AUD-01/AUD-02 del árbol de trabajo resisten
las pruebas E2E. Sin embargo, aparecieron dos defectos reales:

| ID | Severidad | Estado | Título |
|---|---|---|---|
| [ADV-01](findings/ADV-01-bodyless-response-desync.md) | **Alta** | Confirmado | Desincronización de respuestas: un `204`/`304`/`1xx` con framing de cuerpo y cuerpo retrasado envenena una conexión del pool y entrega una respuesta forjada al siguiente cliente. |
| [ADV-02](findings/ADV-02-h2-connection-nominated-headers.md) | **Media** | Confirmado | La traducción h2↔h1 no elimina las cabeceras nominadas por `Connection` en ninguna dirección (fuga h2→cliente y cliente→upstream). |
| [HYP-01](findings/HYP-01-haproxy-hdr-mapping.md) | Informativa | Hipótesis | El importador HAProxy mapea un token `hdr`/`hdr_dom` suelto a `rule.host`; la sintaxis real usa `hdr(nombre)` y la rama parece inalcanzable. |

**No se ha demostrado** una vulnerabilidad crítica, RCE, fuga de secretos de
administración ni bypass de autenticación en el tiempo disponible.

### Lo que sí resistió (resultado negativo, no “seguro”)

- CL+TE, CL duplicado, TE no-`chunked`, espacio antes de `:` → `400` antes del
  upstream; ninguna petición smuggled llegó al origen (E2E S3).
- CR/LF desnudo en una línea y obs-fold → `400` (E2E S4).
- Normalización de path: `/public/../admin` → `/admin`; `/../etc` y
  `/%2e%2e/admin` → `400` (E2E S5).
- Host ausente o duplicado en HTTP/1.1 → `400` (E2E S6).
- `Vary` con aislamiento de variantes y `HEAD` servido desde una entrada `GET`
  (E2E S7/S8).
- **AUD-01**: un `304` con `Set-Cookie` revoca la entrada compartida y fuerza
  un fetch completo; no hay fuga de cookie ni cuerpo viejo (E2E S1).
- **AUD-02**: `Cache-Control: max-age=0` de un cliente no contamina a otro
  (E2E S2), y la propiedad P8 lo verifica a nivel de `Cache`.
- Fuzzing: 2.585.091 ejecuciones totales, 0 crashes, 0 artefactos.

---

## 2. Entorno

| Elemento | Valor |
|---|---|
| SO | macOS 26.5.1 (Darwin 25.5.0), arm64 (Apple T6020) |
| Rust estable | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| Nightly (fuzz) | `cargo 1.101.0-nightly (15be64aa0 2026-10-20)` |
| cargo-fuzz | `0.13.2` |
| Binario E2E | `target/release/eqproxy`, SHA-256 `d68ec1dc2731fb7b088756da094543fe5b8683fb12ca7d674b4ac3fda77e1a44` |
| Cliente h2c | `curl 8.7.1` con `nghttp2/1.68.1`, `--http2-prior-knowledge` |
| Upstream | Python 3.14.6, servidor TCP propio del laboratorio |

Detalle completo en [`run-log.md`](run-log.md) y
`report/adversarial-2026-11-09/environment.txt`.

**No se pudo validar en Linux amd64/arm64 nativo.** Los hallazgos son de lógica
de parsing/pooling, independientes de la arquitectura, pero la confirmación
dual queda pendiente (§7).

---

## 3. Alcance real revisado

Crates: `eqproxy-core`, `eqproxy-config`, `eqproxy-net`, `eqproxy-cli`.

Superficies priorizadas y efectivamente examinadas:

- `eqproxy-net/src/http1.rs` (5.723 líneas): framing de petición/respuesta,
  normalización de target/host, keep-alive, pool, caché, `Expect`, 1xx,
  `HEAD`, `CONNECT`, upgrade, chunking/trailers, reintentos.
- `eqproxy-net/src/http2.rs`: traducción h2↔h1, pseudoheaders, hop-by-hop,
  bodyless, `HEAD`/`204`/`304`.
- `eqproxy-net/src/cache.rs` (2.202 líneas) y el parche sin commitear:
  `lookup`/`lookup_with_policy`, `storable_vary`, `effective_ttl`,
  `end_to_end`, `refresh`/`refresh_storable`, Vary, 304, disco, locks.
- `eqproxy-net/src/proxy_proto.rs`, `auth.rs`, `admin.rs`, `tls.rs`,
  `proxy.rs`, `pool.rs`, `compression.rs`, `ws.rs`, `grpc.rs`, `http3.rs`
  (revisión de código, sin campaña dedicada).
- `eqproxy-config/src/import/{haproxy,nginx,ingress,envoy}.rs`: parsers,
  precedencia, directivas no soportadas, generación de TOML.
- Reglas y routing: `eqproxy-core/src/rules.rs` (fuzz + propiedades).
- Configuración, límites, timeouts, métricas y documentación de garantías.

La matriz completa, con método y resultado por superficie, está en
[`coverage.md`](coverage.md).

---

## 4. Hallazgos confirmados

### ADV-01 — Desincronización de respuestas con estados sin cuerpo (Alta)

Un upstream hostil o defectuoso responde `204 No Content` con
`Transfer-Encoding: chunked` (o `Content-Length`) y envía el cuerpo **después**
de que EQProxy haya devuelto la conexión al pool. EQProxy clasifica la
respuesta como *bodyless* (`BodyFraming::None`), no lee ningún cuerpo y
reutiliza la conexión. El cuerpo tardío se interpreta como la respuesta de la
**siguiente** petición de otro cliente.

Evidencia E2E (S9): el segundo cliente pidió `/normal` y recibió
`HTTP/1.1 200` con cuerpo `SMUGGLED-RESPONSE` (17 bytes), no `NORMAL2`.

- Causa: `eqproxy-net/src/http1.rs:1047-1062` + pooling en `:4911-4929`.
- Precondición: upstream capaz de emitir framing ilegal en un estado sin
  cuerpo (F2 del `THREAT_MODEL.md`: “pueden fallar, devolver basura o atacar”).
- Impacto: inyección de respuesta en la cola de otra petición, corrupción de
  integridad y disponibilidad; con un backend compartido entre listeners,
  cruce de fronteras de host.
- Ficha: [`findings/ADV-01-bodyless-response-desync.md`](findings/ADV-01-bodyless-response-desync.md).

### ADV-02 — Cabeceras nominadas por `Connection` cruzan la traducción h2 (Media)

La traducción h2↔h1 solo elimina una lista fija de hop-by-hop, no las
cabeceras nombradas por `Connection`. Confirmado en ambas direcciones:

- **Respuesta (h1→h2):** el origen envía `Connection: X-Secret` +
  `X-Secret: leaked`; el cliente h2 recibe `x-secret: leaked`
  (`curl --http2-prior-knowledge`, E2E S10). Viola RFC 9113 §8.2.2.
- **Petición (h2→h1):** un cliente h2 envía `Connection: X-Evil` +
  `X-Evil: smuggled-h2`; el origen recibe `x-evil=smuggled-h2` (E2E S12).

- Causa: `eqproxy-net/src/http2.rs:68-78,136-138,357-418,739-763`.
- Impacto: fuga de cabeceras que el origen marcó como hop-by-hop hacia
  clientes h2; y cabeceras que el cliente marcó hop-by-hop llegan al upstream,
  rompiendo la semántica de stripping entre saltos.
- Ficha: [`findings/ADV-02-h2-connection-nominated-headers.md`](findings/ADV-02-h2-connection-nominated-headers.md).

---

## 5. Hipótesis (no confirmadas como impacto)

- **HYP-01** — `eqproxy-config/src/import/haproxy.rs:1815` asigna
  `"hdr" | "hdr_dom"` a `rule.host`. La sintaxis HAProxy real es
  `hdr(<nombre>)` como un único token, por lo que la rama parece inalcanzable;
  si se alcanzara con una entrada malformada, generaría una regla de host que
  compara el valor contra el Host completo en vez del header nombrado. No se
  ha demostrado explotación. Ficha:
  [`findings/HYP-01-haproxy-hdr-mapping.md`](findings/HYP-01-haproxy-hdr-mapping.md).

Otras observaciones menores, sin impacto demostrado:

- `cache::client_fresh` acepta un entity-tag sin comillas (`abc` casa con
  `"abc"`). Es laxitud, no un falso positivo entre representaciones distintas;
  queda documentado por la propiedad P6.
- `end_to_end` conserva `cache-control` y `etag` en la entrada almacenada; es
  intencional (ADR-0034) para revalidar y propagar frescura, no un fallo.
- `refresh_storable` no mira `no-cache` en el `304`, pero el TTL se recalcula
  con `effective_ttl` y queda a cero; no se observó reutilización incorrecta.
- `THREAT_MODEL.md` T01 sigue diciendo “parser fuzzing pending, Phase 2c”
  aunque los harness existen y en esta campaña se ejecutaron; además el target
  `rules` no compilaba (AUD-04). Es deriva documental y de CI, no un fallo de
  producto.

---

## 6. Campañas ejecutadas

| Campaña | Comando (resumen) | Resultado | Log |
|---|---|---|---|
| Suite base | `cargo test --workspace --no-fail-fast` | verde, exit 0 | `report/adversarial-2026-11-09/workspace-test.log` |
| Fuzz framing | `cargo +nightly fuzz run framing -- -max_total_time=120 …` | 1.042.071 ejec., 121 s, 0 crash, exit 0 | `…/fuzz-framing.log` |
| Fuzz config import | `cargo +nightly fuzz run config_import …` | 1.080.684 ejec., 121 s, 0 crash, exit 0 | `…/fuzz-config-import.log` |
| Fuzz rules | `cargo +nightly fuzz run rules …` | 463.336 ejec., 121 s, 0 crash, exit 0 | `…/fuzz-rules.log` |
| Propiedades | `cargo test -p eqproxy-net --test adversarial_properties` | 11/11, exit 0 | `…/property-tests.log` |
| E2E real | `python3 report/adversarial-2026-11-09/lab/run_lab.py` | 10/13; 3 fallos = 2 causas | `…/lab-run.log` |

Total fuzzing: **2.585.091 ejecuciones**, sin artefactos de crash. El corpus
útil quedó en `fuzz/corpus/` (sin secretos ni datos personales).

Los “3 fallos” del laboratorio son expectativas de regresión de los dos
hallazgos abiertos (S9, S10, S12); no hay tests del producto en rojo.

---

## 7. Limitaciones y bloqueos

1. **Solo macOS arm64.** No hay entorno Linux amd64/arm64 nativo disponible;
   la confirmación dual queda pendiente. Los hallazgos no dependen de
   endianness ni de `unsafe` (el crate tiene `#![forbid(unsafe_code)]`).
2. **Campañas de fuzzing de 120 s por objetivo.** Es un presupuesto inicial,
   no saturación; no prueba ausencia de fallos. Los objetivos no incluyen h2,
   h3, TLS, WASM ni el motor completo de exchange.
3. **E2E sin TLS, h3, gRPC, WebSocket ni WASM.** Se priorizó HTTP/1.1 y h2c
   por ser la superficie con más lógica propia y la que concentra los cambios
   sin commitear.
4. **Importadores:** revisión de código y fuzz de `convert`, sin diferencial
   contra binarios HAProxy/NGINX reales. HYP-01 queda como hipótesis.
5. **Carga/concurrencia:** no se ejecutaron ataques de carga ni pruebas de
   estrés; el desync se reprodujo con temporización controlada, no con carga.
6. **Sin Miri, ASAN fuera de fuzzing, ni `cargo-audit`/`cargo-deny` nuevos.**
   El fuzzing usa el sanitizer de libFuzzer (ASAN) en los tres targets.
7. **No se auditó** el plano administrativo/panel con campaña propia, ni la
   cadena de suministro más allá de lo ya documentado por el repo.

---

## 8. Pruebas que fallan

- No hay ningún test existente del repositorio en rojo tras el cambio de
  `fuzz/fuzz_targets/rules.rs`; la suite completa pasa.
- Falla **por diseño** la expectativa E2E de los hallazgos abiertos:
  - `S9_backend_204_with_transfer_encoding_does_not_desync` → el siguiente
    cliente recibe `SMUGGLED-RESPONSE`.
  - `S10_h2_connection_nominated_header_stripped` → `x-secret: leaked`.
  - `S12_h2_request_connection_nominated_header` → `x-evil` llega al origen.
- Estado inicial conocido: el target `rules` no compilaba
  (`missing field 'maps'` en `RequestFacts`); se corrigió **solo el harness**
  añadiendo `maps: MapRegistry::empty()` y la campaña ya ejecuta.

---

## 9. Estado de Git y archivos

**Cambios preexistentes (no tocados):**

```
 M docs/adr/0034-http-response-cache.md
 M eqproxy-net/src/cache.rs
 M eqproxy-net/src/http1.rs
 M eqproxy-net/tests/e2e_cache.rs
?? docs/audit-2026-11-09/
?? docs/evidence/v095-aud-01.md
?? docs/evidence/v095-codegraph-index.json
?? docs/evidence/v095-codegraph.md
?? docs/evidence/v095-p0-2.md
?? docs/evidence/v095-rust-audit-triage.md
?? docs/release-0.9.4-runbook.md
```

**Añadido por esta auditoría (solo harness/pruebas/docs):**

```
 M fuzz/fuzz_targets/rules.rs                      # campo `maps` que faltaba
?? eqproxy-net/tests/adversarial_properties.rs     # 11 propiedades deterministas
?? report/adversarial-2026-11-09/                  # logs, corpus, laboratorio
?? docs/adversarial-audit-2026-11-09/              # este informe
```

No se hizo `git add`, commit, push, tag ni release. No se modificó código de
producto.

---

## 10. Plan de remediación (resumen)

Orden recomendado, con detalle y verificación en
[`remediation.md`](remediation.md):

1. **ADV-01 (Alta):** rechazar `Transfer-Encoding` en cualquier respuesta
   *bodyless* (`1xx/204/304/HEAD`) con `502`; rechazar `Content-Length` en
   `1xx/204`; para `304`/`HEAD` con `Content-Length`, no reutilizar la conexión
   (`wants_close`) salvo prueba de longitud. Añadir regresión E2E con el
   upstream del laboratorio.
2. **ADV-02 (Media):** en `http2.rs`, recoger los tokens de `Connection` y
   filtrar también las cabeceras nominadas, en petición y respuesta; tests
   unitarios con `X-Secret`/`X-Evil`.
3. **HYP-01:** eliminar la rama `hdr`/`hdr_dom` o parsear `hdr(<nombre>)` y
   mapear solo `host`; rechazar el resto con revisión explícita.
4. **CI:** incorporar los tres targets de fuzz y la suite de propiedades; el
   target `rules` ya no compila roto.
5. **Docs:** actualizar `THREAT_MODEL.md` T01 y la matriz de paridad con el
   estado real del fuzzing.

---

## 11. Índice de entregables

- [`coverage.md`](coverage.md) — matriz por superficie, método y resultado.
- [`run-log.md`](run-log.md) — comandos, tiempos, exit codes, hashes.
- [`remediation.md`](remediation.md) — plan priorizado y verificación.
- `findings/ADV-01-bodyless-response-desync.md`
- `findings/ADV-02-h2-connection-nominated-headers.md`
- `findings/HYP-01-haproxy-hdr-mapping.md`
- [`reproduction/README.md`](reproduction/README.md) — cómo repetir el E2E y
  las propiedades, con limpieza.
- Evidencia cruda: `report/adversarial-2026-11-09/` (logs, `lab/results.json`,
  `lab/run_lab.py`, `lab/eqproxy.toml` generado).
- Prueba de propiedades: `eqproxy-net/tests/adversarial_properties.rs`.

---

## 12. Siguiente paso recomendado

Corregir **ADV-01** primero (es el único con impacto de integridad
demostrado) y añadir la regresión E2E; después **ADV-02** con sus dos tests
unitarios. Antes de dar por cerrada la auditoría, repetir el laboratorio en
Linux amd64 y arm64 nativos y ampliar el fuzzing a h2/h3 y al importador con
diferencial contra HAProxy/NGINX fijados.
