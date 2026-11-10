# Reproducción — auditoría v0.9.4

Todo lo que sigue se ejecutó el **2026-11-09** en el árbol
`/Volumes/Lexar/Cursor/EQProxy`, HEAD `ecda55a46f91982842f8bfe87772bf44324df905`
(código de producto idéntico al tag `v0.9.4`, commit
`24e6dc6b29728b8ce16c197642a58268a27d89bd`). Toolchain fijado por
`rust-toolchain.toml` (`1.99.1`).

## 1. AUD-01 y AUD-02 — caché y revalidación

El harness es un test de integración Rust con sockets reales: un origen TCP
programable y el proxy real (`bind` + `serve_http`). No hay mocks ni respuestas
hardcodeadas en el producto; el origen solo programa el comportamiento del
servidor de prueba.

Fichero: `scripts/evidence/audit-2026-11-09/audit_304.rs` (local-only, fuera de
`docs/` según la regla del repositorio). Para ejecutarlo:

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cp scripts/evidence/audit-2026-11-09/audit_304.rs eqproxy-net/tests/audit_304.rs
cargo test -p eqproxy-net --test audit_304 -- --nocapture
rm eqproxy-net/tests/audit_304.rs
```

Resultado guardado en `scripts/evidence/audit-2026-11-09/audit_304.out`:

```text
running 2 tests
test audit_client_max_age_zero_does_not_revalidate ... ok
test audit_304_merge_pollutes_a_shared_cache_entry ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

audit_test_exit=0
```

Los tests **afirman el comportamiento defectuoso** para que sigan verdes mientras
el defecto exista; son evidencia, no un contrato. Cuando se aplique la corrección
deben fallar y reescribirse como regresión del comportamiento correcto.

### Secuencia exacta de AUD-01

1. `GET /reval` anónimo → `200`, `X-Cache: MISS`, cuerpo `body-v1`, origen 1
   petición.
2. `GET /reval` con `Cache-Control: no-cache` → el proxy envía
   `If-None-Match: "v1"`, el origen responde `304` con:
   `set-cookie: leak=1; Path=/`, `cache-control: private`,
   `vary: accept-language`, `x-audit-304: yes`.
3. `GET /reval` anónimo → `200`, `X-Cache: HIT`, origen sigue en 2 peticiones, y
   la respuesta contiene las cuatro cabeceras del `304`.
4. `GET /reval` con `accept-language: fr` → `200`, `X-Cache: HIT`, mismo cuerpo,
   origen sigue en 2 peticiones: la clave no usa el `Vary` anunciado.

### Secuencia exacta de AUD-02

1. `GET /reval` anónimo → `MISS`, origen 1 petición.
2. `GET /reval` con `Cache-Control: max-age=0` → `X-Cache: HIT`, origen 1
   petición. RFC 9111 §5.2.1.1 exige revalidar.

## 2. AUD-04 — fuzz

```bash
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Resultado (guardado en `scripts/evidence/audit-2026-11-09/fuzz-check.out`):

```text
error[E0063]: missing field `maps` in initializer of `RequestFacts<'_>`
  --> fuzz_targets/rules.rs:78:25
   |
78 |             let facts = RequestFacts {
   |                         ^^^^^^^^^^^^ missing `maps`

error: could not compile `eqproxy-core-fuzz` (bin "rules") due to 1 previous error
fuzz_check_exit=101
```

No se ejecutó ninguna campaña de fuzzing: el target no compila. `framing` y
`config_import` sí compilan, pero no se corrieron.

## 3. AUD-03 — trailers

No hay una reproducción E2E de consumo de memoria en esta fase. El mecanismo está
en el código citado:

- `pump_chunked_plain`: `trailers.extend_from_slice(&trailer)` sin contador
  (`eqproxy-net/src/http1.rs:1429`).
- `relay_chunked`: `trailers.append(name, value)` sin contador
  (`eqproxy-net/src/http2.rs:660`).
- La única cota es `MAX_LINE_BYTES = 8 * 1024` por línea
  (`eqproxy-net/src/http1.rs:59`), no por sección.

Reproducción propuesta (pendiente de ejecutar en v0.9.5):

```text
origen real -> chunked, cuerpo pequeño, N trailers de 8 KiB
listener con compresión -> cliente GET
medir RSS del proceso del proxy con ps -o rss= -p <pid> mientras N crece
repetir con h2 (trailers -> HEADERS con END_STREAM)
```

Sin esa medición no se afirma una cifra de memoria; solo se afirma el mecanismo.

## 4. Evidencia previa reutilizada

No se repitió el E2E de tráfico del corte v0.9.4. Se cita la evidencia ya
registrada en `docs/release-0.9.4.md`:

- `netcup-bench` (linux/amd64): HTTP 200 a través de nginx real, marcador del
  origen y log de nginx desde la IP del proxy.
- `oracle-quasar` (linux/arm64): HTTP 200, mismo resultado.
- Imagen: `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8`.
- Suite del tag: 760 passed, 0 failed, 2 ignored (no re-ejecutada aquí).

## 5. Límites de esta reproducción

- No se ejecutaron Veyra, Rust Audit, CodeGraph ni Docker en este incremento
  documental: **NOT EXECUTED**.
- No se midió memoria, CPU ni latencia.
- No se probó HTTP/3, WebSocket, TLS, admin ni importadores con tráfico real.
- El harness de AUD-01/02 se retiró de `eqproxy-net/tests/` para no dejar en el
  producto un test que afirma un defecto; vive en `scripts/evidence/` y se copia
  para ejecutarlo.
