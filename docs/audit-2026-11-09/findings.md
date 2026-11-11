# Fichas de hallazgos — EQProxy v0.9.4

Severidades cualitativas: Alta = rompe una garantía de seguridad y es silencioso;
Media = comportamiento incorrecto explotable o pérdida de disponibilidad;
Baja = deriva documental o de proceso. No se asigna CVSS sin una medición de
explotabilidad completa.

---

## AUD-01 — Un `304` contamina la entrada de caché compartida

**Severidad:** Alta
**Componente:** caché de respuestas (`eqproxy-net/src/cache.rs`), revalidación
HTTP/1.1 (`eqproxy-net/src/http1.rs`)
**Estado:** confirmado con E2E real

### Qué pasa

`Cache::refresh` (`cache.rs:435-512`) fusiona las cabeceras del `304` en la entrada
almacenada. El filtro solo descarta hop-by-hop, `MANAGED_HEADERS`,
`content-length` y `connection` (`cache.rs:444-457`). No vuelve a ejecutar
`storable_vary` (`cache.rs:1105-1138`), que es la función que en el `200` rechaza
`Set-Cookie` y `Cache-Control: private`/`no-store`. Tampoco re-calcula la clave
`Vary`: conserva `old.vary` y `old.vary_values` (`cache.rs:474-475`) aunque el
`304` anuncie un `Vary` nuevo.

El camino real es `OriginHeadPhase::read` → `cache.refresh(...)`
(`http1.rs:3549-3566`), seguido de `lookup` y `serve_cached`. La entrada
contaminada se sirve después a cualquier cliente que haga `GET` sin cookie.

### Impacto

- Una cookie de sesión emitida en un `304` se replica a clientes anónimos. La
  garantía del módulo («a response with `Set-Cookie` is never stored») deja de ser
  cierta sin ningún aviso.
- `Cache-Control: private` o `no-store` en el `304` no expulsa la entrada: el
  origen revoca la cacheabilidad y el proxy sigue sirviendo `X-Cache: HIT` durante
  el TTL por defecto.
- Cabeceras arbitrarias del `304` (`access-control-allow-origin`, `x-*`, etc.) se
  copian a la respuesta compartida. Las hop-by-hop listadas en `Connection` no se
  filtran por esa lista, solo por nombre.
- Un `Vary` nuevo se anuncia en la respuesta pero no se usa en la clave: la misma
  representación se sirve a peticiones con distinta negociación.

No es explotable por un cliente arbitrario por sí solo: hace falta que el origen
responda un `304` con esas cabeceras (origen comprometido, framework con una
política cambiante o un bug de la aplicación). Por eso es Alta y no Crítica; el
daño es silencioso y cruza la frontera entre clientes.

### Reproducción

El harness real está en `scripts/evidence/audit-2026-11-09/audit_304.rs`
(local-only). Copiarlo a `eqproxy-net/tests/` y ejecutar:

```bash
cp scripts/evidence/audit-2026-11-09/audit_304.rs eqproxy-net/tests/audit_304.rs
cargo test -p eqproxy-net --test audit_304 -- --nocapture
rm eqproxy-net/tests/audit_304.rs
```

Resultado del 2026-11-09: `2 passed; 0 failed` (exit 0). El test
`audit_304_merge_pollutes_a_shared_cache_entry` hace:

1. `GET` anónimo → `200`, `X-Cache: MISS`, entra en caché.
2. `GET` con `Cache-Control: no-cache` → el proxy revalida y el origen responde
   `304` con `set-cookie: leak=1`, `cache-control: private`,
   `vary: accept-language` y `x-audit-304: yes`.
3. Otro `GET` anónimo → `200`, `X-Cache: HIT`, y la respuesta contiene
   `set-cookie: leak=1`, `cache-control: private` y `x-audit-304: yes`. El origen
   no recibe ninguna petición.
4. `GET` con `accept-language: fr` → `HIT` con el mismo cuerpo; el origen sigue sin
   recibir peticiones, aunque la respuesta anuncia `Vary: accept-language`.

### Causa raíz

La política de almacenamiento se evalúa una sola vez, en el `200`, y el `304` se
trata como una simple actualización de cabeceras. La actualización no es
equivalente a un almacenamiento nuevo: puede cambiar la naturaleza de la
representación (privada, con cookie, dependiente de otra cabecera) sin pasar por
el gate.

### Corrección propuesta (v0.9.5)

1. Antes de refrescar, parsear el `304` con `parse_origin_head` y aplicar la misma
   política que un `200`:
   - si `has_set_cookie`, `private`, `no-store`, `Vary: *` o `Vary` sobre
     `cookie`/`authorization` → **invalidar** la entrada y no servirla como
     compartida;
   - si el `Vary` del `304` difiere del almacenado → invalidar (no se puede
     re-keyear in-place sin recalcular valores);
   - si el `304` cambia `Cache-Control` a una política no almacenable → invalidar.
2. Fusionar solo cabeceras end-to-end permitidas; nunca `set-cookie`, `vary`,
   `cache-control` ni cabeceras nombradas en `Connection`, sin una decisión
   explícita.
3. Si se invalida, la petición actual debe resolverse con un fetch completo (sin
   condicional) y volver a evaluar `storable_vary`; no reutilizar el cuerpo viejo
   bajo una política revocada.
4. Tests de regresión: `304` con cada cabecera peligrosa, `Vary` nuevo, entrada en
   disco tras reinicio, dos clientes concurrentes y un waiter del cache-lock.

### Criterio de aceptación

- Un `304` con `Set-Cookie`, `private`, `no-store` o `Vary` distinto nunca deja una
  entrada servible; el siguiente cliente no ve esas cabeceras.
- El `200` sigue almacenando y el `304` limpio sigue refrescando sin dial extra.
- Tests E2E nuevos verdes; los tests actuales de `e2e_cache.rs` siguen verdes.

---

## AUD-02 — `Cache-Control: max-age=0` del cliente no fuerza revalidación

**Severidad:** Media
**Componente:** decisión de caché (`eqproxy-net/src/cache.rs:903-940`)
**Estado:** confirmado con E2E real

### Qué pasa

`decide` solo reconoce `no-store` y `no-cache` en la petición. `max-age=0` se
ignora, aunque RFC 9111 §5.2.1.1 lo define como «el cliente no acepta una
respuesta con edad mayor que cero». El resultado es un `HIT` con `Age: N` para una
recarga que el cliente pidió fresca.

### Impacto

Correctitud y confianza del operador: una recarga de navegador o un cliente que
pide explícitamente contenido fresco recibe la copia vieja. No cruza sesiones
(las peticiones con cookie siguen bypassando), pero puede mostrar contenido
obsoleto después de un cambio en el origen.

### Reproducción

Mismo harness; test `audit_client_max_age_zero_does_not_revalidate`:

1. `GET` anónimo → `MISS`, origen 1 petición.
2. `GET` con `cache-control: max-age=0` → `X-Cache: HIT`, origen sigue en 1.

Resultado: `ok` (el test documenta el comportamiento actual).

### Corrección propuesta

- Parsear `max-age`, `min-fresh` y `max-stale` de la petición (y `Pragma:
  no-cache` si se decide mantener paridad con clientes antiguos).
- `max-age=0` y `min-fresh>0` fuerzan revalidación cuando hay validador; sin
  validador, fetch.
- `max-stale` se puede dejar fuera de alcance si se documenta, pero `max-age=0` es
  demasiado común para ignorarlo.
- Tests unitarios de parsing y E2E de `HIT` → `REVALIDATED`.

### Criterio de aceptación

`max-age=0` produce `REVALIDATED` (o `MISS` si no hay validador) y el origen ve la
petición condicional. `max-age` mayor que la edad sigue dando `HIT`.

---

## AUD-03 — Trailers sin límite de cantidad/bytes

**Severidad:** Media (disponibilidad)
**Componente:** `pump_chunked_plain` (`http1.rs:1404-1432`), relay h2
(`http2.rs:643-666`)
**Estado:** cerrado y commiteado (2026-11-11); medición real ejecutada sobre el binario y límites impuestos en h1/h2/h3. Evidencia: [v095-p0-5](../evidence/v095-p0-5.md)

### Qué pasa

Cada línea de trailer está limitada a `MAX_LINE_BYTES` (8 KiB), pero el bucle no
limita cuántas líneas acepta:

- `pump_chunked_plain` acumula todas en `trailers: Vec<u8>` y las escribe después
  del miembro gzip (`http1.rs:1429`, `1555-1556`). La ruta se usa al comprimir una
  respuesta `chunked`.
- `relay_chunked` de h2 acumula en un `HeaderMap` sin tope y lo entrega a
  `send_trailers` (`http2.rs:643-666`).
- `copy_chunked_capped` y `copy_chunked_dechunked` leen y reenvían/descartan en
  streaming; no crecen en memoria, pero consumen CPU y tiempo sin límite.

Un upstream hostil o defectuoso puede enviar trailers indefinidamente. El proxy
no corta por cantidad ni por bytes totales; solo corta si el upstream deja de
enviar (`idle_timeout`). Con un flujo continuo, el `Vec`/`HeaderMap` crece hasta
agotar la memoria del proceso.

### Impacto

Agotamiento de memoria (OOM) o consumo de CPU sostenido en la ruta de respuesta.
Requiere control del upstream o un bug grave del upstream; no lo provoca un
cliente final por sí solo. Aun así, un proxy inverso no debe confiar en que el
upstream termine.

### Reproducción y evidencia (2026-11-11)

La medición se ejecutó con el binario real y un origen real, no con una
estimación: listener h1 con compresión gzip, origen `chunked` con cuerpo
pequeño y **100 000 trailers** de ~7,9 KiB. `ps -o rss= -p <pid>` midió el
proceso vivo: **14 048 KiB baseline → 18 704 KiB peak**, crecimiento de
**4 656 KiB** frente a un presupuesto declarado de 64 MiB. La respuesta se cortó
tras el head comprometido y el contador privado `trailer_limit_exceeded` quedó
en 1. La prueba es `eqproxy-cli/tests/trailer_budget_e2e.rs`; la evidencia
completa, comandos y límites están en [v095-p0-5](../evidence/v095-p0-5.md).

La ruta h2 se cubre con `h2_resets_a_chunked_response_over_the_trailer_budget`
y la h3 con `http3_resets_a_chunked_response_over_the_trailer_budget`; ambas
usan clientes reales (h2/QUIC), no mocks. La suite h3 se repitió 20/20 para
comprobar que el corte no depende de una carrera del fixture.

### Corrección aplicada

- Constantes `MAX_TRAILER_BYTES = 64 * 1024` y `MAX_TRAILER_FIELDS = 128` en
  `eqproxy-net/src/http1.rs`, con un `TrailerBudget` compartido aplicado en
  `copy_chunked_capped`, `copy_chunked_dechunked`, `pump_chunked_plain` y los
  relays h2/h3.
- Petición h1 sobre presupuesto → `400`; respuesta h1 ya comprometida → cierre
  explícito (no se puede enviar un `502` después del `200` sin bufferizar el
  cuerpo); h2 → `RST_STREAM(ENHANCE_YOUR_CALM)`; h3 →
  `H3_GENERAL_PROTOCOL_ERROR`.
- Métrica privada `trailer_limit_exceeded`, con accessor público, que no cambia
  el layout de `MetricsSnapshot`.
- Tests unitarios, E2E h1/h2/h3 y journey de RSS con 100 000 trailers.

### Criterio de aceptación

Un upstream que envía trailers sin fin recibe un corte explícito; el proceso no
supera un presupuesto de memoria declarado; el caso normal (0–2 trailers) sigue
funcionando.

---

## AUD-04 — El target de fuzz `rules` no compila y el fuzz no está en CI

**Severidad:** Media (aseguramiento)
**Componente:** `fuzz/fuzz_targets/rules.rs`, `.github/workflows/`
**Estado:** confirmado

### Qué pasa

`RequestFacts` ganó el campo `maps` (`eqproxy-core/src/rules.rs:63`), pero el
target `rules` construye la struct sin él (`fuzz/fuzz_targets/rules.rs:78`).
`cargo check --manifest-path fuzz/Cargo.toml --bins` falla con `E0063: missing
field maps` (exit 101). Además, ningún workflow de `.github/workflows/` menciona
`fuzz` o `cargo-fuzz`, así que el fallo no se detecta en CI.

### Impacto

La cobertura de fuzzing que el repositorio declara para el evaluador de reglas no
se está ejecutando. Un cambio en `RequestFacts` o en el parser puede romper el
target sin que nadie lo note. No es un fallo de runtime, pero invalida una parte
de la evidencia de robustez.

### Reproducción

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
# exit 101, E0063 missing field `maps` at fuzz_targets/rules.rs:78
```

Salida guardada en `scripts/evidence/audit-2026-11-09/fuzz-check.out`.

### Corrección propuesta

- Añadir `maps: &MapRegistry::empty()` (o el registro vacío correspondiente) al
  target.
- Ejecutar `cargo fuzz build` en un job nightly/opt-in de CI, y una campaña corta
  por target con presupuesto de tiempo.
- Mantener el corpus y el replay comprometido (`rules_fuzz_corpus.rs`) como
  regresión.
- Repetir `cargo fuzz build` tras cualquier cambio de `RequestFacts`.

### Criterio de aceptación

`cargo check --manifest-path fuzz/Cargo.toml --bins` exit 0; `cargo fuzz build`
construye los tres targets; CI falla si vuelven a romperse.

---

## AUD-05 — Deriva documental de release

**Severidad:** Baja
**Componente:** `KNOWN_GAPS.md`, `README.md`
**Estado:** confirmado

### Qué pasa

- `KNOWN_GAPS.md:5` sigue diciendo «what v0.9.2 does not do» y más abajo afirma que
  «v0.9.2 is not declared production ready». El artefacto auditado es v0.9.4.
- `README.md:10` declara **738 tests**; la suite verificada en el tag es **760
  passed, 0 failed, 2 ignored** (evidencia previa del corte, no re-ejecutada en
  esta fase).

### Impacto

Un operador que lea la página de límites recibe una versión anterior del producto.
El conteo de tests no es un contrato, pero alimenta la confianza y ya no coincide.

### Corrección propuesta

- Actualizar `KNOWN_GAPS.md` a v0.9.4/v0.9.5 y revisar cada límite declarado.
- Sustituir el número fijo del README por el conteo real del corte, o por una
  referencia a la evidencia de release.

### Criterio de aceptación

`KNOWN_GAPS.md` nombra la versión publicada; el README no afirma un conteo que la
suite no produce.

---

## Hallazgos descartados o no confirmados

- **Path traversal en importadores**: no se encontró; el conversor trabaja sobre
  texto y no lee archivos. El `file` de `error_pages` lo lee el cargador de
  configuración, no el importador.
- **XSS en el panel**: `panel.rs` escapa nombres, rutas y revisiones
  (`panel.rs:456-595`, `826`) y usa token CSRF; no se encontró un sink sin
  escapar.
- **Comparación de tokens del admin**: `token_matches` es constante
  (`admin.rs:1949-1960`); el plano rechaza `Origin` y exige media type en writes.
- **Spoofing de PROXY protocol**: la recepción exige allowlist de orígenes
  (`proxy_proto.rs:131-140`, validación en `validate.rs:855`).
- **Smuggling CL/TE en h1**: duplicados y CL+TE se rechazan (`http1.rs:336-391`);
  M-01 está cerrado y probado.
