# Run log — auditoría adversarial EQProxy 2026-11-09

Registro de ejecución de la campaña. Todos los comandos se ejecutaron desde la
raíz del repositorio, salvo donde se indique. Los logs crudos están en
`report/adversarial-2026-11-09/` y no se han editado después de su ejecución.

---

## 1. Identidad del árbol auditado

| Campo | Valor inicial | Valor final |
|---|---|---|
| Repositorio | `https://github.com/EQProxy/EQProxy.git` | igual |
| Rama | `release/v0.9.5` | `release/v0.9.5` |
| `HEAD` | `ecda55a46f91982842f8bfe87772bf44324df905` | `ecda55a46f91982842f8bfe87772bf44324df905` |
| Árbol | *dirty* (cambios previos de AUD-01/P0-2) | *dirty* + añadidos de auditoría |
| Commits / tags / push | ninguno | ninguno |

El árbol inicial ya contenía modificaciones sin commitear de otra tarea
(AUD-01/P0-2). **No se revirtieron, descartaron ni sobrescribieron.** La
auditoría se hizo sobre ese estado real, porque es el que se va a publicar.

### `git status --short` inicial (relevante)

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

### Cambios introducidos por la auditoría

```
 M fuzz/fuzz_targets/rules.rs                      # solo harness: campo `maps`
?? eqproxy-net/tests/adversarial_properties.rs     # 11 propiedades
?? report/adversarial-2026-11-09/                  # logs y laboratorio
?? docs/adversarial-audit-2026-11-09/              # este informe
```

No se modificó ningún fichero de producto (`eqproxy-*/src/**`) ni configuración
de build. `report/` está fuera del control de versiones del repo; se conserva
como evidencia local.

---

## 2. Entorno de ejecución

| Elemento | Valor |
|---|---|
| SO | macOS 26.5.1, Darwin 25.5.0, arm64 (Apple T6020) |
| Rust estable | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| Toolchain nightly | `cargo 1.101.0-nightly (15be64aa0 2026-10-20)` |
| cargo-fuzz | `0.13.2` |
| curl | `8.7.1` con `nghttp2/1.68.1` |
| Python | `3.14.6` |
| Sanitizer de fuzzing | ASAN de libFuzzer (por defecto en `cargo-fuzz`) |
| CPU/afinidad | sin fijar; puertos efímeros en `127.0.0.1` |

Volcado completo: `report/adversarial-2026-11-09/environment.txt`.

No se dispuso de Linux amd64/arm64 nativo, contenedor equivalente ni máquina
remota. Esta es la limitación principal de confirmación dual.

---

## 3. Secuencia de ejecución

Orden real según las marcas de tiempo de los logs:

| # | Hora local | Paso | Exit | Log |
|---|---|---|---|---|
| 1 | 22:21:16 | `cargo +nightly fuzz check` | **101** (fallo inicial) | `fuzz-check.log` |
| 2 | 22:22:10 | `cargo test --workspace --no-fail-fast` | 0 | `workspace-test.log` |
| 3 | 22:25:54 | fuzz `framing` (120 s) | 0 | `fuzz-framing.log` |
| 4 | 22:26:03 | `cargo build --release -p eqproxy-cli` | 0 | `build-release.log` |
| 5 | 22:28:14 | fuzz `config_import` (120 s) | 0 | `fuzz-config-import.log` |
| 6 | 22:29:07 | `adversarial_properties` | 0 | `property-tests.log` |
| 7 | 22:30:18 | fuzz `rules` (120 s) | 0 | `fuzz-rules.log` |
| 8 | 22:35:46 | laboratorio E2E real | **1** (3 fallos esperados) | `lab-run.log` |

---

## 4. Comandos exactos y resultados

### 4.1 Comprobación inicial de targets de fuzz

```bash
cargo +nightly fuzz check
```

Resultado: **exit 101**. El target `rules` no compilaba:

```
error[E0063]: missing field `maps` in initializer of `RequestFacts<'_>`
  --> fuzz_targets/rules.rs:78:25
```

Esto es deuda de CI/harness, no un fallo de producto: `RequestFacts` ganó el
campo `maps` y el target no se actualizó. Se corrigió **solo el harness**
añadiendo `maps: eqproxy_core::maps::MapRegistry::empty()` en
`fuzz/fuzz_targets/rules.rs:78`. No se tocó `eqproxy-core` ni `eqproxy-net`.

Comprobación posterior implícita en las tres campañas (compilaron y corrieron).

### 4.2 Suite base del workspace

```bash
cargo test --workspace --no-fail-fast
echo "EXIT:$?"
```

Resultado: **exit 0**. Todos los tests del workspace en verde, incluidos los
tests de caché del árbol *dirty*. Log: `workspace-test.log` (60 KB).

Esto incluye los tests preexistentes de `eqproxy-net`, `eqproxy-config`,
`eqproxy-core` y `eqproxy-cli`; no incluye `adversarial_properties` (se ejecuta
aparte, §4.5).

### 4.3 Fuzzing

Los tres targets se ejecutaron con el mismo presupuesto:

```bash
cargo +nightly fuzz run <target> fuzz/corpus/<target> -- \
  -max_total_time=120 -rss_limit_mb=2048 -print_final_stats=1
```

La invocación efectiva que libFuzzer registra en cada log es:

```
fuzz/target/aarch64-apple-darwin/release/<target> \
  -artifact_prefix=.../fuzz/artifacts/<target>/ \
  -max_total_time=120 -rss_limit_mb=2048 -print_final_stats=1 \
  .../fuzz/corpus/<target>
```

| Target | Semilla | Ejecuciones | exec/s | RSS pico | Nuevos corpus | Crashes | Exit |
|---|---|---|---|---|---|---|---|
| `framing` | `2485874594` | 1.042.071 | 8.612 | 531 MB | 1.463 | 0 | 0 |
| `config_import` | `2625655165` | 1.080.684 | 8.931 | 546 MB | 9.919 | 0 | 0 |
| `rules` | `2749962761` | 463.336 | 3.829 | 592 MB | 5.258 | 0 | 0 |

Total: **2.585.091 ejecuciones**, **0 crashes**, `fuzz/artifacts/` vacío.

Corpus resultante (local, no versionado):

| Corpus | Ficheros | Tamaño |
|---|---|---|
| `fuzz/corpus/framing` | 1.341 | 5,2 MB |
| `fuzz/corpus/config_import` | 4.376 | 17 MB |
| `fuzz/corpus/rules` | 3.866 | 15 MB |

Los corpus iniciales de `framing` (621 ficheros) y `config_import` (873) ya
existían; `rules` arrancó de corpus vacío. No se añadieron datos personales ni
secretos: las entradas son bytes sintéticos.

### 4.4 Build de release para E2E

```bash
cargo build --release -p eqproxy-cli
shasum -a 256 target/release/eqproxy
```

Resultado: **exit 0**, compilado en 26,27 s.

```
d68ec1dc2731fb7b088756da094543fe5b8683fb12ca7d674b4ac3fda77e1a44  target/release/eqproxy
```

El laboratorio usa **este** binario, no `cargo run` ni un mock.

### 4.5 Pruebas de propiedades

```bash
cargo test -p eqproxy-net --test adversarial_properties
```

Resultado: **exit 0**, **11/11** en 0,14 s.

| Test | Qué comprueba |
|---|---|
| `p1_parsers_never_panic_and_reject_explicitly` | 20.000 mutaciones deterministas contra `parse_request_head`/`parse_response_head`; sin pánico. |
| `p2_already_normalized_targets_are_fixed_points` | 20.000 rutas: normalizar dos veces es idempotente. |
| `p3_effective_ttl_never_exceeds_default` | TTL efectivo acotado por el máximo. |
| `p4_storable_vary_refuses_cross_client_keys` | `Vary`/claves no colisionan entre clientes. |
| `p5_end_to_end_removes_connection_nominated_headers` | `cache::end_to_end` elimina hop-by-hop nominadas. |
| `p6_client_fresh_weak_comparison` | Comparación débil de ETag documentada. |
| `p7_vary_variants_never_cross` | Variantes `Vary` aisladas en `Cache`. |
| `p8_client_max_age_zero_does_not_poison_other_clients` | AUD-02 a nivel de caché. |
| `p9_revoking_304_invalidates_clean_304_refreshes` | AUD-01 a nivel de caché. |
| `p10_rendered_head_round_trips` | El head renderizado vuelve a parsearse igual. |
| `generator_sanity` | El PRNG xorshift produce variedad y no degenera. |

El generador es xorshift determinista; no se añadió `proptest` ni dependencias
nuevas. Hay un warning de import no usado (`CacheKey`) que no afecta al
resultado; se deja tal cual para no alterar la evidencia.

### 4.6 Laboratorio E2E real

```bash
python3 report/adversarial-2026-11-09/lab/run_lab.py
echo "EXIT:$?"
```

Resultado: **exit 1**, **10/13** escenarios PASS. Los 3 fallos corresponden a
los 2 hallazgos abiertos (S9, S10, S12), no a tests del producto.

El script:

1. reserva 4 puertos efímeros en `127.0.0.1` (2 listeners, 2 upstreams);
2. arranca dos servidores TCP Python reales (uno normal, uno con el backend
   `desync` hostil);
3. escribe `lab/eqproxy.toml` y arranca `target/release/eqproxy run`;
4. habla HTTP/1.1 byte a byte y h2c con `curl --http2-prior-knowledge`;
5. escribe `lab/results.json`, imprime PASS/FAIL y devuelve 1 si algo falla.

| Escenario | Resultado | Evidencia clave |
|---|---|---|
| `startup` | PASS | listeners y orígenes enlazados |
| `S1_aud01_revoking_304_invalidates` | PASS | `STALE-BODY-v1` → `FRESH-BODY-v2`; sin `Set-Cookie` ni `X-Injected` |
| `S2_client_max_age_zero_scoped` | PASS | `x-cache=['MISS','HIT','MISS','HIT']`; 2 llamadas al origen |
| `S3_request_smuggling_rejected` | PASS | CL+TE, CL dup, TE no chunked y espacio antes de `:` → `400`; 0 smuggled |
| `S4_header_injection_and_obs_fold_rejected` | PASS | CR/LF desnudo y obs-fold → `400` |
| `S5_path_normalization_and_traversal` | PASS | `/public/../admin` → `/admin`; `/../etc` y `/%2e%2e/admin` → `400` |
| `S6_host_header_enforcement` | PASS | Host ausente/duplicado → `400` |
| `S7_vary_variants_isolated` | PASS | gzip e identity no se cruzan |
| `S8_head_served_from_get_entry` | PASS | `HEAD` 200, cuerpo vacío, `CL=11`, `HIT` |
| **`S9_backend_204_with_transfer_encoding_does_not_desync`** | **FAIL** | segundo cliente recibe `SMUGGLED-RESPONSE` |
| **`S10_h2_connection_nominated_header_stripped`** | **FAIL** | `x-secret: leaked` en la respuesta h2 |
| `S11_h2c_basic_translation` | PASS | h2c básico devuelve `NORMAL-OK` |
| **`S12_h2_request_connection_nominated_header`** | **FAIL** | el origen ve `x-evil=smuggled-h2` |

Log: `lab-run.log`. Datos crudos: `lab/results.json`. Config generada:
`lab/eqproxy.toml`. Log del proceso: `lab/eqproxy.log`.

S9/S10/S12 son **regresiones por diseño**: deben pasar cuando se corrijan
ADV-01/ADV-02. Mientras los hallazgos sigan abiertos, el laboratorio devuelve 1.
Esto es intencional para que sirva de puerta de CI.

---

## 5. Hashes de integridad de la evidencia

```text
d68ec1dc2731fb7b088756da094543fe5b8683fb12ca7d674b4ac3fda77e1a44  target/release/eqproxy
bdfa1849c8a0b34d29c18dbad73e87e6bcbf0a45cd55c44461fc9e4573ede1a6  report/adversarial-2026-11-09/lab/run_lab.py
78f713fb6abc3f45d14961831fe5c50d671d8a05ebd05d989b973d62ff1ed0d7  report/adversarial-2026-11-09/lab/results.json
6c9885fc7677ebca9a9875fe78156d2d3c208a1b64ef180795b0ebca445c8784  report/adversarial-2026-11-09/lab-run.log
e6a787b1977ab8b691ec9981308dade0d66aa6a0e01f82602b5ae025ed69807e  eqproxy-net/tests/adversarial_properties.rs
af0ec58d76af447773bc54ece3d9bb23c1fff17c6bb6442abba037f21662775c  fuzz/fuzz_targets/rules.rs
```

Los hashes permiten comprobar que la copia de `run_lab.py` en
`docs/.../reproduction/` y la evidencia local son la misma versión.

---

## 6. Distinción unidad vs E2E

Es importante no confundir qué prueba qué:

| Nivel | Qué demuestra | Qué **no** demuestra |
|---|---|---|
| Fuzzing | Ausencia de pánicos/crashes en el parser bajo 120 s de mutación | Corrección semántica, pooling, ni ausencia de bugs |
| Propiedades | Invariantes de funciones puras y de `Cache` | Comportamiento de red real ni temporización |
| E2E con binario real | Comportamiento observable en socket con upstream real | Carga, concurrencia alta, TLS, h3, WASM |
| Revisión de código | Causa raíz y líneas exactas | Explotabilidad fuera de las precondiciones |

Las pruebas `AUD-01`/`AUD-02` del árbol *dirty* se validaron a nivel de unidad
(suite del workspace) **y** E2E (S1/S2). Los hallazgos ADV-01/ADV-02 se
validaron E2E; su causa raíz está en código leído, no inferida del test.

---

## 7. Limpieza y reproducibilidad

- El laboratorio termina el proceso `eqproxy` en `finally` y cierra el log.
- Los puertos son efímeros; no queda nada escuchando al terminar.
- Los orígenes son hilos daemon del propio script; mueren con él.
- `lab/eqproxy.toml`, `lab/results.json` y `lab/eqproxy.log` se regeneran en
  cada ejecución.
- Para repetir desde cero: ver [`reproduction/README.md`](reproduction/README.md).

No se ejecutaron comandos destructivos, `git clean`, `git reset`, `git add`,
`git commit`, `git push`, ni cambios de rama.

---

## 8. Desviaciones y notas

1. **Presupuesto de fuzzing:** 120 s por target es un primer corte, no
   saturación. No se debe leer como “sin fallos”.
2. **Target `rules` reparado:** se editó el harness antes de poder fuzzear. Se
   documenta como cambio de infraestructura de pruebas, con hash.
3. **Sin Linux nativo:** no se pudo repetir el laboratorio en otra arquitectura.
4. **Un warning de compilación** en la suite de propiedades (`CacheKey` no
   usado). No se corrigió para mantener el log fiel a la ejecución.
5. **El laboratorio devuelve 1** mientras S9/S10/S12 fallen. Es el
   comportamiento esperado, no un error del arnés.
