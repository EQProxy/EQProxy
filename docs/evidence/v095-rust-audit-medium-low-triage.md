# Rust Audit — triage de 471 `medium` + 137 `low` (v0.9.5)

Fecha: 2026-11-14. Árbol: rama `release/v0.9.5`, HEAD `ae85cf9`, con el
worktree sucio existente. Este documento es solo triage: **no se modificó
código, `rust-audit-baseline.json`, el gate ni los seis `high` ya revisados**.

## Fuente y alcance

Informe exacto solicitado:

```text
report/rust-audit-20261114-103520.md
Generated: 2026-11-14 10:35:20 — Rust Audit 0.2.9
6 baseline findings unchanged; 608 new
```

| Severidad | Total |
|---|---:|
| medium | 471 |
| low | 137 |
| **Total** | **608** |

Los informes posteriores (`201304`, `190732`, `174836`, etc.) tienen 477 o
487 `medium` porque el árbol cambió después de las 10:35. **No se han mezclado
con este triage.** Tampoco se re-ejecutó el scan: el árbol actual ya no
reproduce el recuento de 471 y el informe de `103520` es la fuente exacta.

Los seis `high` de la baseline (R20 `pprof`, dos R21 de `build.rs` y tres R18
de `lints/eqproxy-lints`) quedan fuera de alcance. Su triage sigue en
`docs/evidence/v095-rust-audit-triage.md` y no se ha tocado.

## Resultado ejecutivo

- **267 de 608** findings son patrones de test o calidad de test (R02, R08,
  R17). No son defectos de producción.
- **17 de 608** son harness local, fuzz u oráculo test-only; tampoco son
  producto.
- **6 de 608** son `build.rs`; afectan al build, no al runtime.
- **318 de 608** están en código de producto, pero **286 son R10**, una regla
  textual de errores descartados. La revisión muestra que la mayoría son
  `writeln!` sobre `String`, defaults opcionales o cierres best-effort.
- **Los 137 `low` son todos de test.** No hay ningún `low` de producción.
- **No hay P0/P1 confirmados** en estos 471/137. Los únicos P2 son dos gaps
  de robustez/observabilidad en CLI y caché, no exploits ni corrupción.
- La prioridad real de producción sigue siendo los seis `high` ya revisados y
  los gates de release, no estos 608.

## Recuento por regla

Categorías: **Test** = archivos `*/tests/*` o módulos `#[cfg(test)]`/`#[test]`;
**No producto** = harness local, fuzz u oráculo test-only; **Build** =
`build.rs`; **Producto** = código que se compila en el binario o en las
librerías de producción.

| Regla | Título de la regla | Sev | Total | Test | No producto | Build | Producto |
|---|---|---|---:|---:|---:|---:|---:|
| R02 | Fake / meaningless test | low | 134 | 134 | 0 | 0 | 0 |
| R05 | Demo code | medium | 2 | 0 | 0 | 0 | 2 |
| R08 | Flaky / order-dependent test | medium | 130 | 130 | 0 | 0 | 0 |
| R09 | `unwrap`/`expect`/`panic` | medium | 18 | 0 | 12 | 6 | 0 |
| R10 | Swallowed error | medium | 290 | 0 | 4 | 0 | 286 |
| R13 | Missing input validation | medium | 1 | 0 | 1 | 0 | 0 |
| R14 | `clone()` abuse | medium | 23 | 0 | 0 | 0 | 23 |
| R16 | Async misuse (Tokio) | medium | 7 | 0 | 0 | 0 | 7 |
| R17 | `Arc<Mutex<>>` overuse | low | 3 | 3 | 0 | 0 | 0 |
| **Total** | | | **608** | **267** | **17** | **6** | **318** |

Comprobación de subtotales:

```text
medium: 2 + 130 + 18 + 290 + 1 + 23 + 7 = 471
low:   134 + 3                          = 137
total: 471 + 137                        = 608
test/no producto/build/producto: 267 + 17 + 6 + 318 = 608
```

### Desglose de los 17 “no producto”

| Regla | Ubicación | N | Motivo |
|---|---|---:|---|
| R09 | `scripts/evidence/audit-2026-11-09/audit_304.rs` | 11 | harness temporal de reproducción; su cabecera dice “not product code” |
| R09 | `eqproxy-config/src/import/security.rs` | 1 | oráculo test-only del importador; `field_present` no es ruta de producción |
| R10 | `scripts/evidence/audit-2026-11-09/audit_304.rs` | 2 | harness temporal |
| R10 | `fuzz/fuzz_targets/framing.rs`, `rules.rs` | 2 | harness de fuzzing |
| R13 | `eqproxy-config/src/import/security.rs` | 1 | oráculo test-only; el `expect` es de un TOML generado por el test |

Los 6 `build.rs` son tres `.expect` en `eqproxy-cli/build.rs` y tres en
`eqproxy-net/build.rs`. Ambos scripts vuelven antes de ejecutar nada cuando
`PROFILE=release`; los `expect` son sobre variables que Cargo inyecta
(`CARGO_MANIFEST_DIR`, `OUT_DIR`) y sobre `sh`. No son rutas de runtime ni un
riesgo de producción. **No se tocan**: los R21 `high` de esos mismos archivos
están en la baseline revisada.

## Patrones de test (267)

### R02 — 134 `low`: calidad de test, no defecto de producto

- 65 en archivos `*/tests/*`.
- 68 en módulos inline `#[cfg(test)]`/`#[test]`.
- 1 en el harness local `scripts/evidence/audit-2026-11-09/audit_304.rs`.

La regla agrupa “assertion roulette”, literales numéricos sin nombre y
aserciones duplicadas. Pueden ocultar una regresión, pero **no hay ninguna
ruta de producción afectada**. No se arreglan con `#[allow]` ni borrando
tests; si se aborda, es una limpieza de calidad de test en otro cambio.

### R08 — 130 `medium`: flakiness de test

- 114 en archivos `*/tests/*`.
- 16 en módulos inline de test (`eqproxy-net/src/pool.rs`, `admin.rs`,
  `balancer.rs`, `dns.rs`, `maps.rs`, `panel.rs` y `eqproxy-cli/src/main.rs`).

Son `std::thread::sleep`, `Instant::now` o esperas de reloj en tests. El
riesgo es inestabilidad de CI, no comportamiento del proxy. Prioridad P3;
si CI se vuelve inestable, se ataca por los helpers de espera (por ejemplo
`eventually`/`wait_tcp`) y no finding a finding.

### R17 — 3 `low`: estado compartido en tests

`eqproxy-cli/tests/cli_e2e.rs` (10 usos), `eqproxy-net/tests/e2e_backend_h2.rs`
(11) y `eqproxy-net/tests/e2e_cache.rs` (7). Son archivos de test; no hay
`Arc<Mutex<>>` de producción en estos hallazgos. P3.

**Conclusión de tests:** los 137 `low` son íntegramente test; R08 añade 130
`medium`. Los 267 no justifican un cambio de producción.

## Código de producto (318)

### R05 — 2 `medium`: falsos positivos de nombre

- `eqproxy-cli/src/profile.rs:91` `sample_loop`: muestreador real del profiler,
  detrás de `EQPROXY_PROFILE_SAMPLES=1`; no es una demo.
- `eqproxy-net/src/proxy.rs` `sample`: nombre legítimo de la operación de
  muestreo.

Veredicto: **no action**. Renombrar por satisfacer la regla sería ruido.

### R10 — 286 `medium`: la regla más ruidosa

Los 290 R10 se reparten así:

| Subgrupo | N | Lectura |
|---|---:|---|
| `eqproxy-config/src/import/report.rs` `let _ = writeln!` sobre `String` | 90 | **Falso positivo**: `fmt::Write` para `String` no falla; el emisor TOML no puede devolver error por ahí |
| `let _ =` restantes (red, cierre, limpieza, señales) | 107 | Mayoría best-effort deliberado; no se ha confirmado un fallo de corrección |
| `unwrap_or_default()` | 83 | Mayoría campos opcionales/ausentes de importadores y defaults documentados; no equivale a “error silencioso” |
| `.ok()` | 6 | Cuatro joins de `spawn_blocking` en caché, uno de `local_addr()` en HTTP/3 y uno de lectura de `current` en CLI |

Casos revisados que **no** son defecto:

- `import/report.rs` (`emit_toml`, `render`): 90 escrituras a `String`; el
  `let _` es innecesario pero infalible.
- `cache.rs` `refresh`/`insert`/`remove`/`persist_variant`: el `std::fs` está
  dentro de `spawn_blocking`; el `.ok()` del join es best-effort.
- `http1.rs` `write_error`/`write_page`: timeouts de escritura y cierre
  descartados a propósito tras una respuesta local.
- `admin.rs`: `body.unwrap_or_default()` ocurre después de validar
  `content-length`, y el cuerpo vacío termina en 400.
- `cache.rs` `base_key`: ignora `method` a propósito; `HEAD` comparte la
  entrada `GET`, tal como documenta la función.
- `proxy.rs`: `reply.send`/`set_nodelay`/cierres; el receptor puede haberse
  ido y no hay estado que reparar.
- `grpc.rs`/`http3.rs`: respuestas locales y `finish`/`reset` de stream; un
  cliente desconectado no permite otra acción útil.

### R14 — 23 `medium`: clones, sin defecto funcional

Distribución principal: `resolve_with` 30 clones, `ingress::convert` 25,
`nginx::location` 14, `haproxy::frontend` 15, `nginx::upstream` 12,
`nginx::server` 11, `haproxy::convert` 10, `schema::to_spec` 19,
`cache::refresh` 8, `rules::compile` 8, `main::run` 9, `load_error_pages` 7,
`apply_candidate` 6, `http1::serve_http` 6, `proxy::spawn_background` 4,
`proxy::open_grpc_stream` 6, etc.

Son coste de asignación y ruido de propiedad; la mayoría está en carga de
configuración/arranque, no en el bucle por petición. No hay evidencia de un
bug. **P3**: perfilar antes de refactorizar; no hacer un barrido de clones a
ciegas.

### R16 — 7 `medium`: falsos positivos de “blocking I/O”

| Ubicación | Realidad |
|---|---|
| `cache.rs` `refresh`, `insert`, `remove`, `persist_variant` (4) | El `std::fs` está dentro de `tokio::task::spawn_blocking`; no bloquea el executor |
| `maps.rs` `run` (1) | La lectura también está en `spawn_blocking`, con comentario explícito |
| `admin.rs` `bind_unix` (1) | Solo arranque; el `std::fs` prepara el socket antes de servir |
| `main.rs` `apply` (1) | CLI one-shot; no hay reactor de producción que bloquear |

Veredicto: **no action** para los siete. El escáner marca la función `async`
sin ver el `spawn_blocking` o el contexto de arranque/CLI.

## Prioridad para producción

No hay P0/P1 confirmados entre los 471/137. Los seis `high` revisados siguen
siendo el orden del día. Dentro de estos findings, solo estos candidatos
merecen una decisión explícita:

| Prioridad | Regla | Ubicación (informe) | Qué es | Acción propuesta |
|---|---|---|---|---|
| **P2** | R10 | `eqproxy-cli/src/main.rs:1127` (`apply`) | `std::fs::read(&current).ok()` trata un `current.toml` ilegible como si no existiera; se puede instalar el nuevo fichero sin backup y sin aviso | Revisar el camino de error; si el fichero existe pero no se puede leer, abortar o avisar antes de sobrescribir. Añadir test de “current ilegible” |
| **P2** | R10 | `eqproxy-net/src/cache.rs:851` (`persist_variant`) | `std::fs::write`/`rename` fallidos no se registran ni cuentan; la entrada sigue en memoria pero se pierde la persistencia en disco sin señal | Decidir si la caché en disco es best-effort (documentarlo y contar el fallo) o si debe reportar `WARN`/métrica. No es corrupción de datos |
| **P3** | R10 | `eqproxy-net/src/http1.rs:2981/2999` (`serve_cached`) | Error de escritura del body tras enviar cabeceras descartado; el cliente puede ver una respuesta truncada y el access log ya registró el tamaño | Añadir métrica/log de escritura fallida si se quiere observabilidad; no cambia el framing ni la seguridad |
| **P3** | R14 | 23 funciones listadas arriba | Clones de mantenimiento/rendimiento | Perfilar; no refactorizar en bloque |
| **P3** | R08/R02/R17 | Tests | Flakiness y calidad de test | Backlog de CI; no tocar producción |
| **No action** | R05 | 2 nombres | Falsos positivos | Ninguna |
| **No action** | R09/R13 | `build.rs`, `security.rs`, harness | Build/test-only | Ninguna; no tocar los R21 high |

### Nota sobre `audit_304.rs`

El harness `scripts/evidence/audit-2026-11-09/audit_304.rs` reproduce un
defecto histórico de `Cache::refresh` (un 304 con `Set-Cookie`, `private` o
`Vary` distinto). En el árbol actual `cache.rs` ya tiene
`refresh_storable`, que invalida esos casos, y el test unitario
`a_304_that_revokes_shared_storability_is_not_refreshable`. Por eso el
harness es **no producto y potencialmente obsoleto**: sus aserciones afirman
el comportamiento antiguo y no deben usarse como contrato actual. No se ha
ejecutado ni modificado.

## Qué no hacer

- No tocar `rust-audit-baseline.json`, el gate ni los seis `high`.
- No añadir `#[allow]` globales para R02/R08/R10/R14/R16.
- No borrar tests R02/R08/R17 para bajar el recuento.
- No “arreglar” los 90 `writeln!` de `report.rs` ni los `spawn_blocking` de
  R16: son falsos positivos.
- No convertir los P2 en un refactor amplio: son dos caminos de error
  acotados y necesitan decisión de producto/observabilidad.

## Verificación y límites

- Parseados los 608 encabezados del informe: **471 medium + 137 low**.
- Suma por regla y por categoría comprobada: **267 test + 17 no producto +
  6 build + 318 producto = 608**.
- Revisados los contextos de R02/R08, el oráculo `security.rs`, el harness
  `audit_304.rs`, los `build.rs`, los `spawn_blocking` de R16 y las rutas R10
  citadas.
- No se ejecutó `cargo test`, Clippy ni un nuevo Rust Audit: es un cambio
  documental y el árbol actual ya no reproduce el informe de 103520.
- Las líneas del informe pueden haber cambiado respecto al worktree actual;
  se citan junto al nombre de la función. Verificar la línea actual antes de
  editar código.
- No se hizo commit, tag, push ni publicación.
