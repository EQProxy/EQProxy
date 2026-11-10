# Registro de ejecución — auditoría v0.9.4

Fecha: 2026-11-09. Host: macOS (Darwin), workspace
`/Volumes/Lexar/Cursor/EQProxy`. Toolchain del repo: Rust 1.99.1.

## Comandos de esta fase

| # | Comando | Exit | Resultado |
|---|---|---|---|
| 1 | `git status --short` | 0 | limpio antes de crear los entregables |
| 2 | `git rev-parse v0.9.4^{commit}` | 0 | `24e6dc6b29728b8ce16c197642a58268a27d89bd` |
| 3 | `cargo test -p eqproxy-net --test audit_304 -- --nocapture` | 0 | 2 passed, 0 failed |
| 4 | `cargo check --manifest-path fuzz/Cargo.toml --bins` | 101 | `rules` no compila: `missing field maps` |
| 5 | `rg -n 'fuzz\|cargo-fuzz' .github/workflows/*.yml` | 1 | sin coincidencias: no hay job de fuzz |

Salidas crudas:

- `scripts/evidence/audit-2026-11-09/audit_304.out`
- `scripts/evidence/audit-2026-11-09/fuzz-check.out`

## Evidencia previa del corte v0.9.4 (no re-ejecutada)

Estos resultados pertenecen al cierre de release y se citan, no se presentan como
ejecutados en esta auditoría:

| Evidencia | Resultado |
|---|---|
| `cargo fmt --all -- --check` | verde en el corte |
| `cargo clippy --workspace --all-targets -- -D warnings` | verde en el corte |
| `cargo test --workspace` | 760 passed, 0 failed, 2 ignored |
| M-01 | 1 unit + 2 E2E pasando |
| E2E `netcup-bench` amd64 | HTTP 200 a través de nginx real; log del origen correcto |
| E2E `oracle-quasar` arm64 | HTTP 200; mismo resultado |
| Imagen GHCR/Docker Hub | `sha256:d139f07b…4ba8`; `:latest` apunta al mismo digest |
| Tag `v0.9.4` | objeto `da14ee23…d4a`, commit `24e6dc6…89bd` |

## Lecturas de código que sustentan los hallazgos

- `eqproxy-net/src/cache.rs:435-512` — `refresh`.
- `eqproxy-net/src/cache.rs:903-940` — `decide` (solo `no-store`/`no-cache`).
- `eqproxy-net/src/cache.rs:1105-1138` — `storable_vary`.
- `eqproxy-net/src/http1.rs:3549-3566` — llamada a `refresh` tras `304`.
- `eqproxy-net/src/http1.rs:2776-2917` — `serve_cached`.
- `eqproxy-net/src/http1.rs:1404-1432`, `1555-1556` — trailers en compresión.
- `eqproxy-net/src/http2.rs:643-666` — trailers h2.
- `fuzz/fuzz_targets/rules.rs:78` — inicializador sin `maps`.
- `eqproxy-core/src/rules.rs:63` — campo `maps`.
- `README.md:10`, `KNOWN_GAPS.md:5` — deriva documental.

## Estado de la memoria (Hindsight)

La consulta de apertura se hizo contra el banco `eqproxy` para las áreas
`audit`, `http1`, `http2`, `tls`, `config`, `admin`, `cache`, `balancer` y
`security`: el banco respondió sin memorias para esas áreas. Se reporta como
**sin notas aplicadas**, no como «sin riesgos». No se escribió nota de cierre en
esta fase porque la auditoría no es un incremento de producto; la nota
`task-summary` corresponde al cierre de v0.9.5.

## Lo que no se ejecutó

- `cargo test --workspace` completo en esta sesión (el código de producto no se
  tocó; se cita la suite del corte).
- `veyra audit . --profile standard` — **NOT EXECUTED** en esta fase.
- Rust Audit `scan` — **NOT EXECUTED** en esta fase.
- `codegraph-rust index .` / `agent impact` / `agent context` — **NOT EXECUTED**
  en esta fase.
- `cargo fuzz build` / campañas — **NOT EXECUTED**: el target `rules` no compila.
- Docker build, Scout, E2E de imagen — **NOT EXECUTED** en esta fase (evidencia
  previa citada arriba).
- Medición de memoria de AUD-03 — **NOT EXECUTED**.
