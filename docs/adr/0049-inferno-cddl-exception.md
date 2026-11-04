# ADR-0049 — Excepción CDDL-1.0 para `inferno` y render directo del flamegraph

- **Estado:** aceptada
- **Fecha:** 2026-11-04
- **Contexto:** Veyra rechazó el árbol por `RUSTSEC-2026-0194`,
  `RUSTSEC-2026-0195` y la licencia CDDL-1.0 de `inferno 0.11.21`, todas en la
  cadena opcional del perfilador.

## Contexto

La feature no predeterminada `profile` de `eqproxy-cli` usa `pprof 0.15` para
muestrear con `SIGPROF` en hosts donde `perf_event_paranoid=4` impide
`perf_event_open`. La feature `flamegraph` de `pprof` fija `inferno ^0.11`;
`inferno 0.11.21` fija `quick-xml ^0.26`, afectado por dos avisos de severidad
7.5 (solución ≥0.41.0), y declara CDDL-1.0, que no está en la allowlist de
`deny.toml`.

`pprof 0.15.0` es la última versión publicada y su rama master sigue
declarando `inferno ^0.11`, así que no hay release upstream que corrija la
cadena. `inferno 0.12.8` ya usa `quick-xml ^0.41` y mantiene CDDL-1.0.
`pprof::Report::data` y `pprof::Frames` son API pública, y el método
`flamegraph` de `pprof` es un formateo de ~20 líneas sobre
`inferno::flamegraph::from_lines`.

El perfilador no se compila en release (`docker/Dockerfile` usa
`ARG PROFILE=0`; `release.yml` no activa la feature), pero `deny.toml` audita
`all-features = true` y la política de licencias se aplica al lockfile
completo.

## Decisión

1. Se permite **CDDL-1.0 únicamente para `inferno`** mediante
   `[[licenses.exceptions]]` en `deny.toml`. No se añade CDDL-1.0 a la
   allowlist general.
2. `eqproxy-cli` deja de activar `pprof/flamegraph` y depende directamente de
   `inferno 0.12.8` (`default-features = false`, `nameattr`), solo bajo la
   feature `profile`.
3. `eqproxy-cli/src/profile.rs` construye las líneas plegadas desde
   `pprof::Report::data` y renderiza el SVG con
   `inferno::flamegraph::from_lines`, conservando el formato y los marcadores
   `PPROF_SVG_BEGIN`/`PPROF_SVG_END`.

## Alternativas descartadas

- **Permitir CDDL-1.0 globalmente:** amplía la política a cualquier
  dependencia futura sin revisión. La excepción por crate es más estrecha.
- **Eliminar `inferno` y el SVG en proceso:** evita la licencia, pero rompe la
  salida que consumen los arneses de evidencia y obliga a renderizar fuera del
  proxy.
- **Fork o vendor de `pprof`, o `[patch]` de `quick-xml`:** mantiene una
  cadena propia y frágil; `quick-xml 0.41` no satisface el requisito `^0.26`
  de `inferno 0.11`, y `unknown-git = deny` prohíbe una dependencia git.
- **Ignorar los avisos en `cargo-deny`:** sería una supresión para obtener un
  PASS, prohibida por el contrato de verificación.

## Consecuencias

- La cadena auditada pasa a `inferno 0.12.8 → quick-xml 0.41`, sin los dos
  RUSTSEC.
- La obligación CDDL-1.0 queda limitada a `inferno`; el binario de release no
  incluye la feature ni sus dependencias.
- La imagen `profile` se reconstruyó y validó en contenedores linux/arm64 y
  linux/amd64: 150/150 HTTP 200 por señal, salida 0, SVG válido, marcadores
  1/1 y Docker Scout 0 critical / 0 high. Los detalles están en
  `docs/evidence/v092-profiler-inferno.md`; la validación local macOS/aarch64
  sigue siendo la del render.
- Cuando `pprof` publique una versión con `inferno 0.12`, se elimina la
  dependencia directa y la excepción de licencia.
