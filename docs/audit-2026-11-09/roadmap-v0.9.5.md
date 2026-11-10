# Roadmap v0.9.5 — correcciones de la auditoría adversarial de v0.9.4

Objetivo: cerrar los defectos confirmados en la auditoría de v0.9.4 con
comportamiento real, pruebas de regresión y evidencia reproducible, sin declarar
«seguro» lo que solo está «sin hallazgos en una campaña limitada».

Principios:

1. Primero los defectos que rompen una garantía de seguridad (AUD-01), después
   los de correctitud (AUD-02) y disponibilidad (AUD-03), después el aseguramiento
   (AUD-04) y la documentación (AUD-05).
2. Cada corrección entra con su test E2E real; no se acepta un test unitario
   aislado como prueba de integración.
3. No se amplía el alcance con features nuevas. v0.9.5 es un release de
   endurecimiento, no de funcionalidad.
4. Ningún gate se desactiva para obtener verde. Un gate que no se ejecuta se
   reporta **NOT EXECUTED**.

---

## Fase P0 — bloqueantes de release

### P0-1. Cerrar AUD-01: el `304` no puede contaminar la entrada compartida

**Cambios**

- `Cache::refresh` deja de fusionar a ciegas: recibe el `ResponseFacts` del `304`
  (o lo parsea) y aplica la misma política que `storable_vary`.
- Invalidar la entrada (no refrescar) cuando el `304` trae:
  - `Set-Cookie`;
  - `Cache-Control: private` o `no-store`;
  - `Vary: *` o `Vary` sobre `cookie`/`authorization`;
  - un `Vary` distinto del almacenado;
  - un `Cache-Control` que cambia la cacheabilidad a no almacenable.
- Si se invalida, no servir el cuerpo viejo bajo la política nueva: resolver la
  petición con un fetch completo y volver a decidir con `storable_vary`.
- Fusionar solo cabeceras end-to-end permitidas. Excluir siempre `set-cookie`,
  `vary`, `cache-control` y las cabeceras listadas en `Connection` del `304`.
- Mantener el contador `cache_revalidations` y añadir un contador de
  invalidaciones por `304` no almacenable.

**Ficheros previstos**: `eqproxy-net/src/cache.rs`, `eqproxy-net/src/http1.rs`,
`eqproxy-net/tests/e2e_cache.rs`, nuevo `eqproxy-net/tests/e2e_cache_304.rs`.

**Tests obligatorios**

- `304` con `Set-Cookie` → siguiente cliente anónimo no ve la cookie; la entrada
  no se sirve.
- `304` con `private`, `no-store`, `Vary: *`, `Vary` sobre cookie y `Vary` nuevo →
  entrada invalidada y fetch completo.
- `304` limpio (solo `ETag`/`Date`/`Cache-Control` equivalente) → refresco sin
  dial extra y `X-Cache: REVALIDATED`.
- Concurrencia: dos clientes y un waiter del cache-lock durante la invalidación;
  nadie recibe el cuerpo viejo con cabeceras nuevas.
- Persistencia: la entrada invalidada no resucita tras reinicio desde disco.

**Aceptación**: los tests nuevos pasan; `e2e_cache.rs` sigue verde; el invariante
«`Set-Cookie` nunca se sirve desde una entrada compartida» se cumple también en el
camino `304`.

### P0-2. Cerrar AUD-02: directivas del cliente

**Cambios**

- Parsear `max-age`, `min-fresh` y `max-stale` de `Cache-Control` en la petición.
- `max-age=0` y `min-fresh > 0` fuerzan revalidación (o fetch si no hay
  validador).
- `max-stale` puede quedar fuera si se documenta; `Pragma: no-cache` se decide
  explícitamente (hoy está fuera de alcance declarado).
- Tests unitarios del parser y E2E de `HIT` → `REVALIDATED`.

**Aceptación**: `max-age=0` no produce `HIT`; el origen recibe la condicional; una
directiva mayor que la edad sigue produciendo `HIT`.

### P0-3. Cerrar AUD-03: trailers acotados

**Cambios**

- Constantes `MAX_TRAILER_BYTES` y `MAX_TRAILER_FIELDS` en `http1.rs`, aplicadas
  en `copy_chunked_capped`, `copy_chunked_dechunked`, `pump_chunked_plain`,
  `relay_chunked` (h2) y el camino h3.
- Al superar: `502` y cierre del origen; en h2/h3, reset de stream con error de
  protocolo; métrica nueva.
- No acumular trailers en memoria cuando el cliente no los necesita: en
  `copy_chunked_dechunked` ya se descartan; en compresión, escribir en streaming
  al `plain_write` o acotar antes de acumular.

**Tests**

- Origen que envía 1, 128 y 100 000 trailers de 8 KiB; el proxy corta en el
  límite, devuelve `502` y el RSS se mantiene por debajo del presupuesto.
- Caso normal con 0–2 trailers intacto.
- h2: trailers válidos llegan; exceso resetea el stream.

**Aceptación**: el corte es explícito y medible; no hay crecimiento de memoria
proporcional al número de trailers.

---

## Fase P1 — aseguramiento y coherencia

### P1-1. Cerrar AUD-04: fuzz que compila y corre

- Añadir `maps` al `RequestFacts` de `fuzz/fuzz_targets/rules.rs`.
- `cargo check --manifest-path fuzz/Cargo.toml --bins` exit 0.
- `cargo fuzz build` para `framing`, `rules` y `config_import`.
- Job de CI (nightly o manual con presupuesto) que compile y ejecute una campaña
  corta por target; fallar si el build rompe.
- Mantener el corpus y el replay como regresión.

**Aceptación**: build verde y campaña corta ejecutada con semillas; el job falla
si un target deja de compilar.

### P1-2. Cerrar AUD-05: documentación

- `KNOWN_GAPS.md` nombra v0.9.4/v0.9.5 y revisa cada límite.
- `README.md` no declara un conteo de tests que la suite no produce; enlaza a la
  evidencia de release o usa el número del corte.
- `docs/parity-matrix.md` y `docs/semantics.md` revisados contra el
  comportamiento corregido.

### P1-3. Endurecer la verificación del caché

- Añadir un test de contrato que enumere las cabeceras que nunca pueden salir de
  una entrada compartida (`set-cookie`, `www-authenticate`, `proxy-authenticate`).
- Añadir un test de `Vary` con múltiples valores y comas.
- Documentar en `docs/semantics.md` la política exacta de revalidación.

---

## Fase P2 — revisión adicional (no bloqueante)

- HTTP/2: probar `content-length` con `END_STREAM` corto y decidir si se rechaza
  en la traducción (hoy se forwarda la longitud declarada).
- HTTP/3: ejecutar T19 del THREAT_MODEL o mantenerlo explícitamente experimental.
- WebSocket: E2E de handshake y túnel con un backend real.
- TLS: SNI múltiple, rotación en caliente y mTLS.
- Admin: pentest de RBAC, sesiones y socket Unix con credenciales reales.
- Importadores: corpus malicioso y límites de tamaño.
- Carga: memoria y CPU sostenidas con el caché lleno y trailers acotados.

---

## Orden de ejecución

1. P0-1 + tests (unas 2–3 sesiones de trabajo, incluyendo la revisión de
   concurrencia).
2. P0-2 + tests.
3. P0-3 + medición de memoria.
4. P1-1, P1-2, P1-3.
5. P2 según riesgo y tiempo.

No se empieza P1 hasta que P0-1 y P0-3 tengan tests verdes: son los que pueden
cambiar el diseño del caché y del relay.

## Gates de release v0.9.5

Cada gate se ejecuta sobre el árbol final, no sobre un commit intermedio:

| Gate | Comando / artefacto | Criterio |
|---|---|---|
| Formato | `cargo fmt --all -- --check` | exit 0 |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Suite | `cargo test --workspace` | 0 failed; contar y citar |
| Fuzz | `cargo check --manifest-path fuzz/Cargo.toml --bins` + `cargo fuzz build` | exit 0 |
| Veyra | `veyra audit . --profile standard` | verdict y evidence path citados |
| Rust Audit | `python3.13 …/rust_audit.py scan . --format md --out report --fail-on high` | sin critical/high reales sin revisar |
| CodeGraph | `codegraph-rust index . --stats-json …` + `agent impact/context` | índice del árbol final; hechos verificados en fuente |
| Docker amd64 | build + E2E real en `netcup-bench` | CLI, config, tráfico, caché |
| Docker arm64 | build + E2E real en `oracle-quasar` | idem |
| Scout | imagen publicada | 0 critical/high |
| E2E nuevo | AUD-01, AUD-02, AUD-03 | tests verdes y medición citada |
| Documentación | `docs/release-0.9.5.md` | tag = versión = changelog |

La publicación (push, tag, imagen, release) sigue requiriendo autorización
explícita del owner. Este roadmap no autoriza ninguna.

## Riesgos del roadmap

- **Cambiar `refresh` puede romper el cache-lock**: la invalidación debe publicar
  `Gone` a los waiters y no dejar entradas a medio refrescar. Los tests de
  concurrencia son obligatorios.
- **Acotar trailers puede romper clientes legítimos** que usan trailers grandes
  (por ejemplo firmas). El límite debe ser configurable o suficientemente amplio,
  y el error debe ser observable.
- **El test de memoria puede ser inestable en CI**: medir RSS en un proceso
  aislado con un presupuesto holgado; no convertirlo en un umbral ajustado.
- **La fase P2 puede crecer sin límite**: no se incluye en el release si no está
  cerrada con evidencia.

## Fuera de alcance de v0.9.5

- `stale-while-revalidate`, `stale-if-error`, `only-if-cached` (decisión de
  ADR-0034).
- FastCGI/uwsgi/scgi y service discovery nativo.
- Nuevas features de UI, admin o importadores.
- Cambios de formato de configuración o de API pública sin ADR.
