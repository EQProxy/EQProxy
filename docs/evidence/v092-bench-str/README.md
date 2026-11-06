# v0.9.2 benchmark slice — S-STR-01 on netcup and Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 escenarios ejecutados; ninguno restante; la casilla 16 se
cerró el 2026-11-04 con perfiles por escenario y ADR-0048.** S-STR-01 se ejecutó en
ambos hosts con **5 repeticiones de 600 s**, **0 errores** y crudos publicados.
El arnés clasifica **Better** en los dos hosts y el `report.json` incluye
`gaps` y `classification`; no es una victoria limpia: en netcup el p50 y la
memoria incremental por conexión son peores; en Oracle la memoria incremental
no es calculable y el reporte publicado la deja en 0. El escenario
restante es **S-PLG-01**; S-OVL-01 ya tiene evidencia propia.

No se usó ninguna prueba de arranque como evidencia. Este documento publica
solo las series medidas de 600 s; los smokes de validación quedaron fuera.

## 1. Escenario y método

S-STR-01 es HTTP/1.1 persistente con **subida completa y descarga completa por
ciclo**:

- **32 conexiones persistentes en netcup; 16 en Oracle.** No se comparan los
  hosts entre sí: el de Oracle tiene 4 vCPU y el de netcup 8.
- **64 KiB de subida + 64 KiB de descarga** por ciclo; el origen Go lee el
  cuerpo con `Content-Length` y lo devuelve.
- **15 s de calentamiento + 600 s medidos**, 5 repeticiones alternando el orden
  de los productos.
- Cada proxy en Docker con **2 GiB y 2 CPU**; origen y generador sin esos
  límites. Sin puertos publicados en el camino medido.
- **No es full-duplex:** se sube y después se descarga. No se atribuye al
  arnés ninguna prueba de solapamiento.
- La calibración directa contra el origen debe alcanzar **1,20×** el proxy más
  rápido. Se midió con 3 s de calentamiento y 10 s.

Generador: `scripts/bench_v092_str_load` (Go, cliente HTTP/1.1 real). Arnés:
`scripts/bench_v092_str.py`. Las imágenes y sus IDs están en cada
`report.json`.

## 2. netcup — 32 conexiones

- Arquitectura: `x86_64`; Linux 6.8; Docker 29.6.1.
- Imágenes: EQProxy `eqproxy-bench:v092` / `a461b0f678128017`; HAProxy 3.4.6 /
  `e273915691a8eab3`; generador `eqproxy-strload:v092` / `a08f92386acd9861`.
- Calibración directa: **52.733,84 ciclos/s**, 0 errores, `sanity=ok`.
- Headroom: **3,114×**; válido.
- **11 crudos** (1 calibración + 10 series), 0 errores, **25/25 checksums**.

| Producto | Ciclos/s | MiB/s | p50 | p95 | p99 | CPU % de un core | HWM | Mem. incremental/conexión |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | **16.932,9** | **2.116,6** | 1,816 ms | 2,917 ms | **3,755 ms** | 204,0 | 20,55 MiB | 58.112 B |
| HAProxy | 14.393,8 | 1.799,2 | **1,231 ms** | 3,343 ms | 38,620 ms | 205,0 | 52,01 MiB | **46.848 B** |

Rangos de las cinco repeticiones:

| Producto | Ciclos/s | p50 | p99 | HWM | Mem./conexión |
|---|---:|---:|---:|---:|---:|
| EQProxy | 16.302,6–18.378,2 | 1,693–1,878 ms | 3,307–3,900 ms | 20,45–20,82 MiB | 51.456–61.440 B |
| HAProxy | 14.110,1–15.181,9 | 1,162–1.251 ms | 38,038–38,919 ms | 51,91–52,48 MiB | 44.288–47.360 B |

Ratos EQProxy/HAProxy: throughput **1,176×**; p50 HA/EQ **0,678×** (EQProxy
peor); p99 HA/EQ **10,284×** (EQProxy mejor); CPU **0,995×**; HWM **0,395×**;
memoria incremental **1,240×** (EQProxy peor).

**Lectura honesta:** el throughput y el p99 favorecen a EQProxy; el p50 y la
**memoria incremental por conexión** son peores. La memoria de la tabla es
`(HWM − baseline) / 32`, no el RSS: el HWM absoluto de EQProxy es menor
(20,55 frente a 52,01 MiB), pero su incremento por conexión es mayor. No es una
victoria limpia en todas las métricas.

El `report.json` solo guarda la carga del host al inicio (**0,29**); no hay
`load1` por repetición. No se afirma una cifra de contención posterior que no
esté en los crudos.

## 3. Oracle — 16 conexiones

- Arquitectura: `aarch64`; Linux 7.0; Docker 29.1.3; 4 vCPU.
- Imágenes: EQProxy `eqproxy-bench:v092` / `5a2e16f8b4d219ef`; HAProxy 3.4.6 /
  `e273915691a8eab3`; generador `eqproxy-strload:v092` / `5207a048143b22d1`.
- Calibración directa: **16.309,95 ciclos/s**, 0 errores, `sanity=ok`.
- Headroom: **2,272×**; válido.
- **11 crudos** (1 calibración + 10 series), 0 errores, **25/25 checksums**.

| Producto | Ciclos/s | MiB/s | p50 | p95 | p99 | CPU % de un core | HWM | Mem. incremental/conexión |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | **7.179,9** | **897,48** | **2,073 ms** | **4,080 ms** | **5,250 ms** | **149,6** | 16,19 MiB | 0 * |
| HAProxy | 5.999,7 | 749,97 | 2,502 ms | 4,919 ms | 6,350 ms | 178,0 | 47,34 MiB | **47.616 B** |

Rangos de las cinco repeticiones:

| Producto | Ciclos/s | p50 | p99 | CPU | HWM | Mem./conexión |
|---|---:|---:|---:|---:|---:|---:|
| EQProxy | 7.141,6–7.215,4 | 2,064–2,087 ms | 5,233–5,286 ms | 148,6–150,3 % | 16,07–16,27 MiB | 0 * |
| HAProxy | 5.798,6–6.151,1 | 2,428–2,624 ms | 6,180–6,391 ms | 169,7–180,3 % | 47,31–47,36 MiB | 47.104–48.640 B |

Ratos EQProxy/HAProxy: throughput **1,197×**; p50 HA/EQ **1,207×** (EQProxy
mejor); p99 HA/EQ **1,209×** (EQProxy mejor); CPU **0,840×**; HWM **0,342×**.
El `run.log` y el `report.json` clasifican **Better**. El `report.json` **sí
contiene `gaps` y `classification`**; los ratos anteriores coinciden con sus
campos de resultados.

`*` **No interpretar el 0 como cero bytes.** El cálculo del arnés es
`(HWM − baseline) / 16`; para EQProxy el HWM (16.920.576 B) es menor que el
baseline (17.092.608 B), el delta es negativo y el reporte publicado lo deja en
0. La memoria incremental por conexión **no es calculable** con esta fórmula;
para comparar memoria se usa el HWM absoluto, que sí es menor. El arnés ya
corregido escribe `null` y `memory_per_connection_status: not_computable` en
este caso; no se reescribió el reporte publicado.

La carga del host al inicio era **7,11** y no hay `load1` por repetición. Es un
host de 4 vCPU con contención; los valores absolutos no se generalizan. Aun
así, las cinco repeticiones son estrechas y no hay un outlier que explique el
resultado.

## 4. Auditoría de los crudos

- **netcup:** los 10 crudos de series coinciden con `report.json` en generador,
  recursos y `config_sha256`; la fórmula de memoria se cumple en **10/10**.
- **Oracle:** los 10 crudos coinciden en generador, recursos y
  `config_sha256`. La fórmula de HAProxy se cumple en **5/5**; en EQProxy da 0
  porque el delta es negativo, como se explica arriba.
- **Checksums:** `netcup/checksums.sha256` verifica **25/25**; 
  `oracle/checksums.sha256` verifica **25/25**. El `checksums.sha256` raíz
  cubre los dos directorios y este README.
- **Estabilidad:** las cinco repeticiones de netcup mantienen el p50 de EQProxy
  en 1,69–1,88 ms y el p99 de HAProxy en 38,0–38,9 ms; no es un único outlier.
  Oracle mantiene p50 2,06–2,09 ms y p99 5,23–5,29 ms en EQProxy.
- **Errores/reconexiones:** 0 en todas las series de ambos hosts.
- **Límite de la comparación:** 32 conexiones frente a 16; los hosts tienen
  distinto tamaño y no se comparan entre sí.

## 5. Límites y NOT EXECUTED

1. **S-STR-01 queda ejecutado en ambos hosts**; el contador global es
   **18/18** y no queda ningún escenario de benchmark. La casilla 16 se cerró
   el 2026-11-04; los perfiles por escenario están en
   `../v092-profile-summary.md`.
2. **No full-duplex**; la prueba es upload seguido de download.
3. **32 conexiones en netcup y 16 en Oracle** por el distinto tamaño de host;
   no se comparan las dos arquitecturas entre sí.
4. **Una máquina por arquitectura**, componentes comparten host; sin afinidad
   de CPU ni generador separado. La carga no se registró por repetición.
5. **Defecto del reporte publicado de Oracle:** `gaps` y `classification` sí
   están; el único defecto es que la memoria incremental de EQProxy queda en 0
   por el delta negativo. No se editó el `report.json`; se documenta el
   defecto. No se usa el 0 como dato de memoria.
6. **Sin perfilado** (`perf_event_paranoid=4`), sin imagen de release nueva y
   sin Docker Scout en este incremento.
7. **Sin smoke como evidencia.** Las validaciones de arranque no se publican.
8. La calibración de netcup es 3,114× y la de Oracle 2,272×; ambas superan la
   puerta de 1,20×.

## 6. Reproducción

```bash
# netcup (32 conexiones)
python3 bench_v092_str.py --out <dir> --eq-image eqproxy-bench:v092 \
  --load-binary ./bin/strload --reps 5 --conns 32 --body 65536 \
  --warmup 15 --duration 600 --calibration-warmup 3 --calibration-duration 10

# Oracle (16 conexiones)
python3 bench_v092_str.py --out <dir> --eq-image eqproxy-bench:v092 \
  --load-binary ./bin/strload --reps 5 --conns 16 --body 65536 \
  --warmup 15 --duration 600 --calibration-warmup 3 --calibration-duration 10
```

## 7. Integridad

```bash
(cd netcup  && sha256sum -c checksums.sha256)
(cd oracle  && sha256sum -c checksums.sha256)
sha256sum -c checksums.sha256
```
