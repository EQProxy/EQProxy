# ADR-0048 — Decisiones sobre los resultados *Worse* de v0.9.2

- **Estado:** aceptada (decisiones provisionales donde el perfil está pendiente)
- **Fecha:** 2026-11-04
- **Contexto:** casilla 16 de `ROADMAP 0.9.2`; `BENCHMARK_PLAN.md` §8 exige
  perfilar cada resultado *Worse* y decidir entre optimizar o aceptar.

## Contexto

Los 18 escenarios están ejecutados, pero la casilla 16 sigue **EN CURSO**
porque faltaban perfiles y decisiones. `perf` y `bpftrace` no son utilizables
en los hosts (`kernel.perf_event_paranoid=4`). Se añadió un perfilador
opcional (`eqproxy-cli/profile`, `pprof 0.15` con `SIGPROF`, sin
`perf_event_open`) y una etapa `profile` en la imagen, nunca la de release.

### Perfil ejecutado

| Celda | Resultado |
|---|---|
| S-H2-01 netcup | 98 muestras `/proc` en 48,8 s; RSS 102 388 → 136 836 KiB (máx. 142 808), FDs 11 → 355, sockets TCP 1 → 347, `syscw` 751 747 |
| S-H2-02 netcup | ejecutado con la misma imagen; no se extrajo la serie temporal |

La relación observada es **≈98 KiB de RSS por socket**: el crecimiento sigue
el número de streams/conexiones, no el payload. Es coherente con
`PIPE_BUFFER = 64 KiB` por stream en `eqproxy-net/src/http2.rs:61` (H3 ya usa
16 KiB en `http3.rs`).

Estas ejecuciones son **perfilado de 1 repetición**, no sustituyen las
campañas de 5 repeticiones: S-H2-01 dio 0,653× RPS y S-H2-02 0,874×, cifras
no comparables con las publicadas (0,605×/0,709× y 0,700×).

### Perfil NOT EXECUTED

H2 en Oracle, S-H3-01, S-TCP-01, S-FAIL-01 y S-CFG-01. Motivo: el tiempo de
build y la ventana de campaña; no hay datos de perfil que los respalde. Las
imágenes `eqproxy-profile:v092` están construidas y validadas en ambos hosts
(`pprof_strings=1`), pero eso no es un perfil.

## Decisiones

1. **S-H2-01 — optimizar.** Reducir `PIPE_BUFFER` de 64 KiB a 16 KiB, como
   H3, y re-medir con 5 repeticiones. El perfil muestra que la memoria escala
   con los sockets (≈98 KiB/socket); 400 streams justifican ≈19 MB de ahorro
   potencial. La decisión es provisional hasta la re-medición. **Implementado
   el 2026-11-04** en `eqproxy-net/src/http2.rs`; la re-medición de 5
   repeticiones sigue **NOT EXECUTED** y no se declara una mejora.
2. **S-H2-02 — aceptar con este ADR.** El coste es de traducción H2↔H1 con
   payload de 1 MiB; el perfil no muestra una fuga, y el p99 de EQProxy fue
   mejor (ratio 1,219). Se revisa la memoria después del cambio del punto 1.
3. **S-H3-01 Oracle — aceptar con este ADR.** Es el coste de traducir H3→H1
   en la celda Oracle; sin perfil no se atribuye a una función concreta. No se
   declara optimización sin evidencia.
4. **S-TCP-01 Oracle — aceptar con este ADR y corregir la métrica.** La
   «memoria por conexión» publicada es crecimiento de pico, no estado
   estacionario; se sustituye por muestreo en régimen. El throughput queda
   aceptado como coste de la pila TCP propia, sin perfil que lo contradiga.
5. **S-FAIL-01 — no aceptar el *Worse* anterior; re-ejecutar.** Los configs no
   eran equivalentes: HAProxy usaba `timeout connect 1s` y el TOML de EQProxy
   no declaraba `connect_timeout` (defecto 5 s). Se añadió
   `connect_timeout = "1s"` al listener y se separó `failover_ms` en
   `error_burst_ms` + `blackout_ms`. El blackout de ~5 s era el timeout
   configurado, no un fallo del health check.
6. **S-CFG-01 — aceptar con este ADR.** El p99 posterior al reload se mide en
   una ventana de 2 s que incluye el intercambio; se declara la ventana y no se
   certifica como victoria limpia.
7. **S-PLG-01 — sin cambio.** Es un delta medido, no un *Worse* contra el
   producto; su decisión ya está en ADR-0031.

## Correcciones metodológicas incluidas

- `docker/Dockerfile`: `ARG PROFILE` se declara **dentro** del stage
  `builder`; antes el `if` tomaba la rama de release y la imagen «profile» no
  contenía el perfilador. Verificado con `strings` (`pprof sampler started`).
- Etapa `profile` reconstruida y validada en contenedor linux/arm64 y
  linux/amd64: 150/150 por señal, SVG válido y Scout 0C/0H (2026-11-04).
- `scripts/bench_v092_fail.py`: paridad de timeout de conexión y campos
  `error_burst_ms`/`blackout_ms` en `analyze`, con prueba sintética
  (`valid=True`, `failover=1900 ms`, `blackout=1900 ms`).
- Perfilador validado de punta a punta en local: 150/150 HTTP 200, salida 0,
  4 muestras y SVG de 81 921 bytes con ambos marcadores.

## Consecuencias

La casilla 16 **no se cierra**: faltan los perfiles de Oracle/H3/TCP/FAIL/CFG
y la re-medición de H2 con 5 repeticiones. Cualquier cifra de este ADR
procedente de una sola repetición se cita como perfil, nunca como resultado
publicable.
