# Perfiles de la casilla 16 (v0.9.2)

- **Fecha:** 2026-11-04
- **Ámbito:** perfiles de los escenarios *Worse* de `BENCHMARK_PLAN.md` §8.
- **Imagen de perfil netcup:** `eqproxy-profile:v092`, `sha256:8c7f4956c2a49e7c…`
  (linux/amd64, 8 CPU).
- **Imagen de perfil Oracle:** `eqproxy-profile:v092-fixed`,
  `sha256:23c429d2bdf3b68b…` (linux/arm64, 4 CPU).
- **Naturaleza:** todas las corridas de este documento son de **1 repetición**
  y existen para atribuir o aceptar un *Worse*. No sustituyen las campañas de
  5 repeticiones ni se publican como cifras de rendimiento.

El arnés manda `SIGINT` (`docker kill -s INT`) y espera al proceso. Las
imágenes de perfil antiguas no completaban `pprof::finish` con el `SIGTERM` de
`docker stop`; las corridas antiguas sin `PPROF_SVG_BEGIN` / `PPROF_SVG_END` no
se usan como perfil.

## Corridas válidas (netcup, amd64)

| Escenario | Evidencia | EQProxy | HAProxy | Ratio EQ/HA | Muestras `/proc` | SVG |
|---|---|---:|---:|---:|---:|---|
| S-H2-01 | `docs/evidence/v092-profile-h2/netcup/run-20261104T200700Z` | 36 412,6 rps | 62 248,6 rps | 0,585 | 38 | 1/1 |
| S-H2-02 | `docs/evidence/v092-profile-h2/netcup/run-20261104T201218Z` | 1 487,5 rps | 1 592,1 rps | 0,934 | 38 | 1/1 |
| S-H3-01 | `docs/evidence/v092-bench-h3/run-20261104T194248Z` | 34 311,3 rps | 57 416,4 rps | 0,598 | 39 | 1/1 |
| S-H3-02 | `docs/evidence/v092-bench-h3/run-20261104T194248Z` | 1 476,7 hs/s | 1 798,1 hs/s | 0,821 | 26 | 1/1 |
| S-TCP-01 | `docs/evidence/v092-profile-tcp` | 282,4 conn/s | 422,2 conn/s | 0,669 | 13 | 1/1 |
| S-TCP-02 | `docs/evidence/v092-profile-tcp` | 103 163,1 req/s | 89 799,4 req/s | 1,149 | 38 | 1/1 |
| S-FAIL-01 | `docs/evidence/v092-profile-fail/run-20261104T195933Z` | 124 errores | 52 errores | 0,419 (HA/EQ) | 45 | 1/1 |
| S-CFG-01 | `docs/evidence/v092-profile-cfg/run-20261104T200049Z` | p99 2,231× | p99 1,748× | 1,277 (EQ/HA) | 44 | 1/1 |

Datos adicionales:

- **S-H2-01:** RSS 102 236 → 138 224 KiB; FDs 11 → 291; sockets TCP 1 → 282.
- **S-H2-02:** RSS 101 832 → 148 180 KiB (máx. 153 492); FDs 11 → 48;
  sockets TCP 1 → 38. La memoria escala con los streams, no con el payload.
- **S-H3-01/S-H3-02:** RSS máximo 125 548 / 135 852 KiB; 0 errores.
- **S-TCP-01:** memoria por conexión 34 227 B vs 3 671 B; 0 errores.
- **S-TCP-02:** memoria por conexión 98 222 B vs 26 173 B; 0 errores; p99
  2,04 ms vs 37,99 ms.
- **S-FAIL-01:** failover 1 106 ms vs 3 987 ms; recuperación completa
  606 ms vs 588 ms; 0 errores de arnés.
- **S-CFG-01:** 0 errores; carga del host 7,60 al inicio, por lo que esta
  corrida no se usa para reclasificar el escenario (la campaña de 5
  repeticiones sigue siendo la referencia).

Los SVG de netcup contienen direcciones, no nombres de función (limitación ya
documentada para amd64). Por eso las decisiones no atribuyen el coste a una
función concreta; usan la memoria, las latencias y el comportamiento medido.

## Oracle arm64: perfil NOT EXECUTED

- **H2-01:** con `eqproxy-profile:v092-fixed` el contenedor
  `h2bench-eqproxy` salió con **139 (SIGSEGV)** durante la carga, sin
  `PPROF_SVG_BEGIN/END` (log local: `docs/evidence/v092-profile-oracle/h2-01/run.log`). La
  misma compilación release sí completa el escenario:
  `docs/evidence/v092-profile-oracle/h2-release/`, 18 669 vs 25 463 rps, 0 errores. El
  fallo es de la imagen de perfil, no del escenario.
- **H3-01:** la calibración de carga del sink dio 27 400,9 rps, pero la de
  handshake dio 0; al entrar en S-H3-01 el contenedor `h3bench-eqproxy` salió
  con **139 (SIGSEGV)**, sin marcadores (log local:
  `docs/evidence/v092-profile-oracle/h3-01/run.log`). El sink y el origen siguieron vivos.
- No se presenta ningún perfil arm64 de Oracle. Las decisiones de H2/H3 se
  apoyan en los perfiles netcup de la tabla y declarar esta limitación.

## Reproducción

```bash
# netcup, ejemplo H2
python3 scripts/bench_v092_h2.py --reps 1 --scenarios S-H2-01 \
  --eq-image eqproxy-profile:v092 --profile \
  --load-binary /root/bench-v092-h2/bin/load-amd64 \
  --probe-binary /root/bench-v092-h2/bin/probe-amd64

# netcup, TCP/FAIL/CFG
python3 scripts/bench_v092_tcp.py --reps 1 --profile \
  --eq-image eqproxy-profile:v092 --out /root/eqproxy-v092/docs/evidence/v092-profile-tcp \
  --load-binary /root/eqproxy-tcp/bin/tcp_load \
  --probe-binary /root/eqproxy-tcp/bin/probe
python3 scripts/bench_v092_fail.py --reps 1 --profile \
  --eq-image eqproxy-profile:v092 --out /root/eqproxy-v092/docs/evidence/v092-profile-fail \
  --load-binary /root/eqproxy-fail/fail-load-amd64
python3 scripts/bench_v092_cfg.py --reps 1 --profile \
  --eq-image eqproxy-profile:v092 --out /root/eqproxy-v092/docs/evidence/v092-profile-cfg \
  --load-binary /root/eqproxy-cfg/cfg-load-amd64
```

Comprobación de un perfil válido, no solo del código de salida:

```bash
grep -E 'INFO shutdown signal received|INFO pprof flamegraph written|PPROF_SVG_(BEGIN|END)' \
  docs/evidence/v092-profile-h2/netcup/run-20261104T200700Z/logs/S-H2-01-eqproxy-rep1.log
```
