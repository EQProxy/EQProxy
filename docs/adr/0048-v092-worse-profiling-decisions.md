# ADR-0048 — Decisiones sobre los resultados *Worse* de v0.9.2

- **Estado:** aceptada; casilla 16 cerrada con perfiles por escenario
- **Fecha:** 2026-11-04
- **Contexto:** casilla 16 de `ROADMAP 0.9.2`; `BENCHMARK_PLAN.md` §8 exige
  perfilar cada resultado *Worse* y decidir entre optimizar o aceptar.

## Contexto

Los **18/18 escenarios** están ejecutados y las campañas de 5 repeticiones
están publicadas. `perf` y `bpftrace` no son utilizables en los hosts
(`kernel.perf_event_paranoid=4`). El perfilador opcional
(`eqproxy-cli/profile`, `pprof` con `SIGPROF`, sin `perf_event_open`) y la
etapa `profile` de la imagen permiten obtener SVG sin privilegios de kernel.

En esta sesión se perfilaron los escenarios *Worse* con **una repetición** en
`netcup-bench` (amd64). Las corridas están en
`docs/evidence/v092-profile-summary.md`; no son campañas publicables y no
sustituyen las cinco repeticiones. Los perfiles arm64 de Oracle quedan
**NOT EXECUTED** por un SIGSEGV de la imagen de perfil; se declara abajo y no
se usa ningún dato de Oracle como perfil.

## Perfiles ejecutados (netcup, amd64, 1 repetición)

Imagen `eqproxy-profile:v092` (`sha256:8c7f4956c2a49e7c…`).

| Escenario | EQProxy | HAProxy | Ratio | Muestras / SVG |
|---|---:|---:|---:|---:|
| S-H2-01 | 36 412,6 rps | 62 248,6 rps | 0,585 | 38 / 1+1 |
| S-H2-02 | 1 487,5 rps | 1 592,1 rps | 0,934 | 38 / 1+1 |
| S-H3-01 | 34 311,3 rps | 57 416,4 rps | 0,598 | 39 / 1+1 |
| S-H3-02 | 1 476,7 hs/s | 1 798,1 hs/s | 0,821 | 26 / 1+1 |
| S-TCP-01 | 282,4 conn/s | 422,2 conn/s | 0,669 | 13 / 1+1 |
| S-TCP-02 | 103 163,1 req/s | 89 799,4 req/s | 1,149 | 38 / 1+1 |
| S-FAIL-01 | 124 errores | 52 errores | 0,419 (HA/EQ) | 45 / 1+1 |
| S-CFG-01 | p99 2,231× | p99 1,748× | 1,277 (EQ/HA) | 44 / 1+1 |

Los SVG de netcup contienen direcciones, no nombres de función (limitación
conocida de amd64). La atribución se limita a memoria, FDs, sockets, latencias
y al comportamiento del escenario; no se inventa una función caliente.

## Perfil NOT EXECUTED (Oracle arm64)

- **S-H2-01:** `eqproxy-profile:v092-fixed`
  (`sha256:23c429d2bdf3b68b…`) sale con **139 (SIGSEGV)** durante la carga.
  La release del mismo escenario sí funciona (`run-20261104T194140Z`: 18 669
  vs 25 463 rps, 0 errores), así que el fallo es de la imagen de perfil.
- **S-H3-01:** la calibración de carga del sink pasa (27 400,9 rps), la de
  handshake queda en 0 y el contenedor `h3bench-eqproxy` sale con **139** al
  entrar en el escenario. Sink y origen siguen vivos; no hay SVG.
- Por tanto no hay perfil arm64 de Oracle y no se le atribuye CPU ni una
  función concreta. Las decisiones de H2/H3 usan los perfiles netcup.

## Decisiones

1. **S-H2-01 — mantener 16 KiB como cambio de memoria, sin claim de RPS.**
   Reducir `PIPE_BUFFER` de 64 KiB a 16 KiB, como H3, sigue siendo un cambio
   de memoria. La re-medición de 5 repeticiones dio netcup **0,594×** y Oracle
   **0,744×**; el perfil netcup de 1 repetición da **0,585×**. El RSS pasó de
   102 236 a 138 224 KiB y los FDs de 11 a 291: el crecimiento sigue sockets y
   streams, no el payload. **Implementado el 2026-11-04** en
   `eqproxy-net/src/http2.rs`; no se declara mejora de throughput.
2. **S-H2-02 — aceptar con este ADR.** El perfil de 1 repetición da 0,934×,
   p99 favorable (ratio 1,339) y RSS 149,54 vs 84,53 MiB; no hay fuga. El
   coste es la traducción H2↔H1 con payload de 1 MiB. Se revisa memoria tras
   el cambio del punto 1, sin prometer una optimización en 0.9.2.
3. **S-H3-01 Oracle — aceptar con este ADR.** El perfil netcup de S-H3-01 da
   0,598× y el de S-H3-02 0,821×; el coste es la traducción H3→H1. No hay
   perfil arm64 de Oracle, así que no se atribuye a una función ni se declara
   una optimización no medida.
4. **S-TCP-01/S-TCP-02 — aceptar y corregir la métrica de memoria.** El perfil
   netcup da 34 227 vs 3 671 B por conexión en S-TCP-01 y 98 222 vs 26 173 B
   en S-TCP-02. La cifra publicada es crecimiento de pico, no estado
   estacionario; se mantiene como límite de producto y candidato a revisión en
   0.9.3. El throughput de S-TCP-02 es 1,149× en este perfil, pero no se
   promociona desde una repetición.
5. **S-FAIL-01 — aceptar el *Worse* con la configuración equivalente.** La
   corrida de perfil, ya con `connect_timeout = "1s"`, da 124 vs 52 errores,
   failover 1 106 vs 3 987 ms y recuperación completa 606 vs 588 ms. EQProxy
   falla más rápido pero deja más errores en la ventana de indisponibilidad.
   No se cambia el health check en 0.9.2; queda como brecha conocida para
   0.9.3.
6. **S-CFG-01 — aceptar con este ADR.** La campaña de 5 repeticiones sigue
   siendo la referencia (netcup Better 1,209× vs 1,633×; Oracle Comparable
   1,652× vs 1,572×). El perfil de 1 repetición se ejecutó con carga 7,60 y
   dio 2,231× vs 1,748×; se declara la carga y **no** se usa para reclasificar
   el escenario. No se cambia el producto.
7. **S-PLG-01 — sin cambio.** Es un delta medido, no un *Worse* contra el
   producto; su decisión ya está en ADR-0031.

## Correcciones metodológicas incluidas

- El arnés de perfil manda `SIGINT` (`docker kill -s INT`) y espera con
  `docker wait`. Las imágenes antiguas no completaban `pprof::finish` con el
  `SIGTERM` de `docker stop`; las corridas sin `PPROF_SVG_BEGIN`/`END` no se
  consideran perfiles.
- Los textos de ayuda de `bench_v092_h2.py`, `bench_v092_tcp.py`,
  `bench_v092_fail.py`, `bench_v092_cfg.py` y `v092-h3-bench/run.py` se
  actualizan a SIGINT.
- La imagen `profile` se reconstruyó en Oracle desde el árbol actual
  (`docker build --target profile --build-arg PROFILE=1`,
  `sha256:23c429d2bdf3b68b…`); la compilación terminó, pero la ejecución bajo
  carga H2/H3 sale con 139. No se oculta ni se sustituye por una corrida sin
  muestras.

## Consecuencias

La **casilla 16 queda cerrada** con 18/18 escenarios, perfiles por escenario y
una decisión registrada para cada *Worse*. La limitación es explícita: los
perfiles arm64 de Oracle **no se ejecutaron** por el SIGSEGV de la imagen de
perfil; no se presentan como PASS ni se usan para atribuir CPU. Cualquier cifra
de este ADR procedente de una sola repetición se cita como perfil, nunca como
resultado publicable. No hubo commit, push, tag ni release.
