# v0.9.2 benchmark slice — S-H3-01 / S-H3-02 en netcup y Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 escenarios ejecutados; ninguno restante; la casilla 16 se
cerró el 2026-11-04 con perfiles por escenario y ADR-0048. S-H3-01 y S-H3-02 se
ejecutaron en ambos hosts con 2 GiB y 2 CPU por proxy, 5 repeticiones y
40 crudos, pero NO se certifican. Tres de las cuatro celdas no superan la
calibración de 1,20× y la única celda válida (S-H3-01 en Oracle) es *Worse*.**

La casilla 16 se cerró el 2026-11-04. Este documento publica la ejecución y sus
límites; no convierte H3 en una victoria ni en un escenario certificado.

## 1. Objetivo y alcance

Comparar EQProxy 0.9.1 con HAProxy 3.4.6, ambos con HTTP/3 activo, en los dos
escenarios de `BENCHMARK_PLAN.md` §3:

- **S-H3-01** — HTTP/3 (QUIC), payload de 1 KiB, 8 conexiones QUIC × 8 streams
  concurrentes (64 en vuelo), GET `/`, 3 s de calentamiento y 15 s medidos,
  closed-loop.
- **S-H3-02** — handshakes QUIC masivos, 16 conexiones concurrentes, 2 s de
  calentamiento y 10 s medidos.

El arnés es `scripts/evidence/v092-h3-bench/run.py` (copia adaptada del arnés
H3 de v0.7). Cada proxy corre en un contenedor con:

```text
--cpus 2 --memory 2g --memory-swap 2g --ulimit nofile=65536
```

La aplicación está en `scripts/evidence/v092-h3-bench/run.py:189-190`. El
reporte no guarda un campo `limits`; la comprobación reproducible es esa línea
del arnés, no una afirmación inferida del `report.json`.

Los crudos, configuraciones, certificados y logs están en:

- [`netcup/`](netcup/) — `run-20261103T144951Z`, 20 crudos;
- [`oracle/`](oracle/) — `run-20261103T145724Z`, 20 crudos.

El primer intento de netcup, `run-20261103T144114Z`, se ejecutó **sin** esos
límites y queda descartado; no se publica como campaña válida. El primer
intento de Oracle también se detuvo antes de producir una campaña con límites
y no se usa.

## 2. Método y calibración

Cada serie tiene 5 repeticiones. La calibración mide el sink directo con el
mismo generador y exige un headroom de **1,20×** frente al proxy más rápido de
la serie; por debajo de ese umbral la comparación se clasifica **Not
comparable**. La calibración de handshakes imprime `rps=0` en el log por un
fallback del arnés; el valor real está en
`calibration.handshake_c16.generator.handshakes_per_sec` del `report.json`.

| Host | Escenario | Sink directo | Proxy más rápido | Headroom | Veredicto |
|---|---|---:|---:|---:|---|
| netcup (amd64, 8 vCPU) | S-H3-01 | 41.469,16 req/s | 53.635,84 req/s | 0,773× | **Not comparable** |
| netcup (amd64, 8 vCPU) | S-H3-02 | 1.529,97 hs/s | 1.856,60 hs/s | 0,824× | **Not comparable** |
| Oracle (arm64, 4 vCPU) | S-H3-01 | 26.723,52 req/s | 18.009,10 req/s | 1,484× | válida |
| Oracle (arm64, 4 vCPU) | S-H3-02 | 540,85 hs/s | 504,10 hs/s | 1,073× | **Not comparable** |

La carga del host al inicio fue `0,449/3,885/3,031` en netcup y
`0,113/0,362/0,508` en Oracle. La calibración de netcup por debajo del sink de
HAProxy invalida sus dos series; en Oracle, S-H3-02 queda solo un 7,3 % por
encima del proxy más rápido, por debajo del 20 % exigido. Solo **S-H3-01
Oracle** permite una clasificación de throughput.

## 3. Resultados

Medianas de 5 repeticiones. La latencia es la del generador; CPU es porcentaje
de un core y RSS es el pico del contenedor.

### netcup (amd64)

| Escenario | Producto | req/s o hs/s | p50 | p99 | CPU | RSS |
|---|---|---:|---:|---:|---:|---:|
| S-H3-01 | EQProxy | 38.123,3 | 1,54 ms | 4,00 ms | 182,6 % | 28,08 MiB |
| S-H3-01 | HAProxy | 53.635,8 | 0,92 ms | 4,61 ms | 190,1 % | 32,60 MiB |
| S-H3-02 | EQProxy | 1.676,9 | 10,17 ms | 14,95 ms | 115,4 % | 40,04 MiB |
| S-H3-02 | HAProxy | 1.856,6 | 6,94 ms | 29,83 ms | 193,8 % | 38,56 MiB |

### Oracle (arm64)

| Escenario | Producto | req/s o hs/s | p50 | p99 | CPU | RSS |
|---|---|---:|---:|---:|---:|---:|
| S-H3-01 | EQProxy | 13.909,9 | 4,35 ms | 10,36 ms | 136,8 % | 26,05 MiB |
| S-H3-01 | HAProxy | 18.009,1 | 3,08 ms | 10,42 ms | 130,0 % | 28,93 MiB |
| S-H3-02 | EQProxy | 525,9 | 33,67 ms | 45,27 ms | 104,5 % | 30,39 MiB |
| S-H3-02 | HAProxy | 504,1 | 25,43 ms | 87,88 ms | 193,8 % | 36,75 MiB |

Ratos EQProxy/HAProxy:

| Celda | Throughput | p50 HA/EQ | p99 HA/EQ | CPU EQ/HA |
|---|---:|---:|---:|---:|
| S-H3-01 netcup | 0,711× | 0,600× | 1,152× | 0,961× |
| S-H3-02 netcup | 0,903× | 0,682× | 1,995× | 0,595× |
| S-H3-01 Oracle | 0,772× | 0,708× | 1,006× | 1,052× |
| S-H3-02 Oracle | 1,043× | 0,755× | 1,941× | 0,539× |

Observaciones que no dependen de la calibración:

- **p50 es peor en las cuatro celdas**: EQProxy tarda entre 1,32× y 1,67× más
  en la mediana.
- **p99 es mejor en las cuatro celdas**: el ratio HA/EQ va de 1,006× a 1,995×.
- **CPU**: EQProxy usa menos CPU en tres celdas; en S-H3-01 Oracle usa un 5,2 %
  más. El generador fue la parte limitante en varias series.
- **RSS**: EQProxy usa menos memoria en tres celdas; en S-H3-02 netcup usa
  40,04 frente a 38,56 MiB (4 % más).
- **Errores**: 0 en los 40 crudos y en las cuatro series; no hay respuestas
  truncadas ni fallos de handshake registrados.

## 4. Clasificación honesta

- **S-H3-01 Oracle — Worse.** Es la única celda con calibración válida:
  13.909,9 frente a 18.009,1 req/s (0,772×), p50 4,35 frente a 3,08 ms y p99
  prácticamente igual (1,006×). La pérdida de throughput es material.
- **S-H3-01 netcup — Not comparable.** 0,711× de throughput, pero el sink
  directo solo alcanza 0,773× del proxy más rápido.
- **S-H3-02 Oracle — Not comparable.** El throughput bruto es 1,043× (dentro
  del ruido), pero la calibración de 1,073× no llega al 1,20×. No se presenta
  como *Comparable* ni como victoria; el p50 es peor (0,755×).
- **S-H3-02 netcup — Not comparable.** 0,903× de throughput con calibración
  0,824×.

**H3 no se certifica.** H3 llevó el contador a 12/18 porque ambos escenarios
se ejecutaron y sus crudos se publican, no porque H3 haya pasado el gate. Con
S-STR-01, S-FAIL-01, S-CFG-01, S-OVL-01 y S-PLG-01, el contador global es 18/18. El resultado válido es una
pérdida; S-PLG-01 es un delta medido y no cambia esa conclusión.

## 5. Límites y NOT EXECUTED

1. **No hay certificación H3.** Solo una de las cuatro celdas supera la
   calibración y es *Worse*. La casilla 16 se cerró con el perfil de escenario
   y su decisión en ADR-0048; H3 sigue sin certificarse.
2. **No queda ningún escenario de benchmark:** S-PLG-01 ya tiene
   arnés y evidencia en `../v092-bench-plg/`; la casilla 16 se cerró con
   perfiles por escenario y ADR-0048.
3. **Perfil:** la campaña de 5 repeticiones no perfila CPU/memoria; el perfil
   de 1 repetición de S-H3-01/S-H3-02 está en
   `../v092-profile-summary.md` (netcup, SVG 1/1; Oracle arm64 NOT EXECUTED).
   `perf` no está disponible (`perf_event_paranoid=4`); no se atribuye la
   diferencia de throughput a una función concreta.
4. **No hay imagen de release nueva ni Docker Scout** para este incremento.
5. **No se probó 0-RTT ni reanudación QUIC**; H3 se mide con handshakes
   completos.
6. **Una sola máquina por arquitectura**, componentes comparten host; no hay
   afinidad de CPU ni generador separado. Los valores no se generalizan a
   producción.
7. El `report.json` no incluye el campo de límites del contenedor. La
   aplicación de `--cpus 2 --memory 2g` se verifica en el arnés citado; el
   primer run sin límites se descartó y no se publica.

## 6. Reproducción

```bash
# netcup
cd /root/eqproxy-h3/scripts/evidence/v092-h3-bench
python3 run.py --reps 5 --eq-image eqproxy-bench:v092 \
  --load-binary /root/eqproxy-h3/bin/h3load \
  --probe-binary /root/eqproxy-h3/bin/probe

# Oracle
cd /home/ubuntu/eqproxy-h3/scripts/evidence/v092-h3-bench
python3 run.py --reps 5 --eq-image eqproxy-bench:v092 \
  --load-binary /home/ubuntu/eqproxy-h3/bin/h3load \
  --probe-binary /home/ubuntu/eqproxy-h3/bin/probe
```

Artefactos: `netcup/report.json`, `oracle/report.json`, 20 crudos por host,
4 configuraciones, 6 certificados y 40 logs. Los checksums están en
[`checksums.sha256`](checksums.sha256): **95/95 archivos, 0 discrepancias**.
