# v0.9.2 benchmark slice — S-FAIL-01 on netcup and Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 escenarios ejecutados; ninguno restante; la casilla 16 se cerró el 2026-11-04 con perfiles por escenario y ADR-0048.** S-FAIL-01 se ejecutó en
ambos hosts con **5 repeticiones**, **2 GiB y 2 CPU** por proxy y 0 fallos del
generador en la tanda publicada. La clasificación es **Worse en netcup y Oracle**: EQProxy recupera el
origen A antes, pero tarda más en dejar de producir errores y acumula muchas
más respuestas 502/503. S-PLG-01 está ejecutado; el perfil de S-FAIL-01
está en `../v092-profile-summary.md`.

No se usó ninguna ejecución de arranque como evidencia. Los crudos publicados
son los 20 runs de 20 s (10 por host) más la calibración directa.

## 1. Escenario y método

S-FAIL-01 comprueba fallo y recuperación de un backend con dos orígenes HTTP
reales:

- **32 workers**, una conexión HTTP/1.1 nueva por petición, 1 KiB de respuesta;
  esto evita que el keep-alive fije el tráfico al backend muerto.
- **5 s de calentamiento + 20 s medidos**, 5 repeticiones alternando el orden
  de los productos.
- A los **8 s** del arranque se ejecuta `docker stop -t 1 fail-origin-a`; a los
  **15 s** se ejecuta `docker start` y se espera a que `/healthz` responda.
- Health check idéntico: HTTP `GET /healthz`, `200`, intervalo **200 ms**,
  timeout **100 ms**, `fall = 2`, `rise = 2`.
- Cada proxy en Docker con **2 GiB y 2 CPU**; orígenes y generador sin esos
  límites. Sin puertos publicados en el camino medido.
- Buckets de **100 ms**; esa es la resolución de los tiempos de recuperación.
- Calibración directa contra el origen A: **2 s de calentamiento + 5 s**; debe
  alcanzar 1,20× el proxy más rápido.

Generador/origen: `scripts/bench_v092_fail_load` (Go, HTTP/1.1 real).
Arnés: `scripts/bench_v092_fail.py`.

### Qué mide cada tiempo

- `failover_ms`: desde el inicio del `stop` hasta el primer bucket de 100 ms
  **después del último error** con tráfico y cero errores. Es el tiempo hasta
  que el tráfico vuelve a estar limpio, no solo hasta que el proxy marca A down.
- `a_recovery_ms`: desde el inicio del `restart` hasta la primera respuesta
  correcta servida por A. Prueba que el health check volvió a levantarlo.
- `full_recovery_ms`: primer bucket, tras el reinicio, con A y B sirviendo y
  cero errores. En estas series coincide con `a_recovery_ms`.
- `errors_outage`: errores dentro de la ventana fallo→reinicio. `errors` es el
  total del run y puede incluir alguno fuera de esa ventana.

## 2. netcup — 8 vCPU, 32 workers

- Arquitectura `x86_64`; Linux 6.8; Docker 29.6.1.
- Imágenes: EQProxy `eqproxy-bench:v092` / `a461b0f678128017`; HAProxy 3.4.6 /
  `e273915691a8eab3`; generador `eqproxy-failload:v092` / `44629ee11e9a79e0`.
- Carga inicial: `0,97 4,93 3,05` (1/5/15 min); el 5-min ya era alto.
- Calibración directa: **31.105,0 rps**, 0 errores; headroom **2,745×**;
  válida.
- **11 crudos** (1 calibración + 10 series); **25/25 checksums**.

| Producto | RPS | p50 | p99 | Fallo→limpio | A vuelve | Errores (mediana) | Errores outage | Tasa de error |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 1.345,6 | 15,534 ms | 51,986 ms | 5.034,6 ms | **512,5 ms** | 124 | 121 | 0,498 % |
| HAProxy | **11.330,4** | **2,059 ms** | **5,824 ms** | **4.025,5 ms** | 625,5 ms | **48** | **48** | **0,0212 %** |

Rangos de las cinco repeticiones:

| Producto | RPS | p50 | p99 | Fallo→limpio | A vuelve | Errores |
|---|---:|---:|---:|---:|---:|---:|
| EQProxy | 1.067,5–1.749,7 | 12,42–20,74 ms | 36,91–57,80 ms | 5.002,5–5.112,5 ms | 386,9–634,5 ms | 106–150 |
| HAProxy | 10.287,6–12.019,7 | 1,93–2,26 ms | 5,49–6,38 ms | 4.008,5–4.033,6 ms | 533,6–710,3 ms | 36–51 |

**Lectura honesta:** EQProxy vuelve a anunciar A antes (512,5 vs 625,5 ms),
pero **tarda más en dejar de dar errores** (5,03 vs 4,03 s) y acumula **2,6×
más errores** (124 vs 48). El throughput no se usa para la clasificación de
este escenario; se publica porque el generador lo mide. Es una carga de
conexión nueva por petición, y EQProxy queda muy por debajo en esa carga.

## 3. Oracle — 4 vCPU, 32 workers

- Arquitectura `aarch64`; Linux 7.0; Docker 29.1.3.
- Imágenes: EQProxy `eqproxy-bench:v092` / `5a2e16f8b4d219ef`; HAProxy 3.4.6 /
  `e273915691a8eab3`; generador `eqproxy-failload:v092` / `217fd230c6228625`.
- Carga inicial: `0,79 3,07 1,90`.
- Calibración directa: **10.967,0 rps**, 0 errores; headroom **2,612×**;
  válida.
- **11 crudos**; **25/25 checksums**.

| Producto | RPS | p50 | p99 | Fallo→limpio | A vuelve | Errores (mediana) | Errores outage | Tasa de error |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 3.277,1 | 6,855 ms | 24,015 ms | 5.015,1 ms | **596,2 ms** | 160 | 160 | 0,243 % |
| HAProxy | **4.198,1** | **5,777 ms** | **13,686 ms** | **4.004,5 ms** | 604,4 ms | **38** | **32** | **0,0452 %** |

Rangos:

| Producto | RPS | p50 | p99 | Fallo→limpio | A vuelve | Errores |
|---|---:|---:|---:|---:|---:|---:|
| EQProxy | 3.120,5–3.294,2 | 6,84–7,08 ms | 20,68–28,43 ms | 5.007,9–5.101,3 ms | 413,1–607,9 ms | 143–166 |
| HAProxy | 4.064,7–4.231,2 | 5,71–5,93 ms | 13,37–14,61 ms | 3.999,8–4.014,8 ms | 599,9–614,6 ms | 32–42 |

**Lectura honesta:** la vuelta de A es prácticamente igual (596,2 vs 604,4 ms),
pero EQProxy vuelve a tardar ~1 s más en limpiar los errores y acumula **4,2×
más errores** (160 vs 38). La diferencia es peor que la de netcup en la métrica
de errores.

## 4. Veredicto

| Host | Recuperación HA/EQ | Errores HA/EQ | Clasificación |
|---|---:|---:|---|
| netcup | 1,220× (HA más lento) | 0,387× (HA muchos menos) | **Worse** |
| Oracle | 1,014× (casi igual) | 0,238× (HA muchos menos) | **Worse** |

El criterio del arnés exige que EQProxy no empeore la recuperación y que no
acumule más errores. La primera parte se cumple en netcup, pero la segunda no;
en Oracle tampoco hay ventaja de recuperación. **S-FAIL-01 queda ejecutado y
publicado, no certificado como victoria.** El perfilado de la causa de los
errores y de la caída de RPS queda pendiente; no se atribuye a una causa interna
sin `perf`.

Tipos de error observados en las cinco repeticiones:

- netcup EQProxy: 480 × `502`, 156 × red; HAProxy: 68 × `502`, 160 × `503`.
- Oracle EQProxy: 625 × `502`, 160 × red; HAProxy: 28 × `502`, 160 × `503`.

## 5. Auditoría de los crudos

- **10/10 series por host** coinciden con `report.json` en generador, control,
  análisis y `config_sha256`.
- Cada host tiene **11 crudos**: 1 calibración y 10 series (5 por producto).
- Las **5 repeticiones de cada producto son válidas** (`valid: true`) y el
  generador terminó con 0 fallos de arnés.
- `checksums.sha256` verifica **25/25** en netcup y **25/25** en Oracle.
- El `checksums.sha256` raíz cubre los dos directorios y este README.
- **Divergencia declarada:** la primera tanda completa se descartó antes de
  publicar porque el análisis iniciaba la ventana en `failure_completed` y
  perdía los errores producidos durante el `docker stop`. No se publicaron sus
  crudos. La tanda publicada usa `failure_started` y se validó con 9 pruebas
  unitarias. La ejecución corta de preflight tampoco se publica ni se usa como
  evidencia.

## 6. Límites y NOT EXECUTED

1. **Una máquina por arquitectura**; orígenes, proxies y generador comparten
   host, sin afinidad de CPU. La carga inicial y su evolución son visibles.
2. **32 workers y 1 KiB por petición**; no se compara con otros tamaños.
3. **Resolución de 100 ms** para recuperación; no se afirma precisión de
   microsegundos.
4. **`Connection: close`**: cada petición crea una conexión. El RPS no es una
   comparación de keep-alive y no se usa para el veredicto.
5. **Formato del generador:** el binario medido se compiló antes de un `gofmt`
   final que solo alineó campos; no cambió la lógica. El código recompilado y
   `go vet` pasan, pero la campaña no se repitió por ese cambio cosmético.
6. **Sin perfilado** (`perf_event_paranoid=4`), sin `heaptrack`, sin Docker
   Scout y sin imagen de release nueva.
7. **Sin TLS, reglas, rewrites, compresión ni plugins** en este escenario.
8. **No se reejecutaron Veyra, Rust Audit ni CodeGraph**: este incremento no
   cambió código Rust; solo añadió arnés Go/Python y evidencia. Queda **NOT
   EXECUTED** para este cierre.
9. **No se usan smokes como evidencia.** La validación de arranque no se
   publica.

## 7. Reproducción

```bash
# netcup (amd64)
python3 bench_v092_fail.py --out <dir> --eq-image eqproxy-bench:v092 \
  --load-binary ./fail-load-amd64 --reps 5 --conns 32 \
  --warmup 5 --duration 20 --kill-at 8 --restart-at 15 \
  --calibration-warmup 2 --calibration-duration 5

# Oracle (arm64)
python3 bench_v092_fail.py --out <dir> --eq-image eqproxy-bench:v092 \
  --load-binary ./fail-load-arm64 --reps 5 --conns 32 \
  --warmup 5 --duration 20 --kill-at 8 --restart-at 15 \
  --calibration-warmup 2 --calibration-duration 5
```

## 8. Integridad

```bash
(cd netcup  && sha256sum -c checksums.sha256)
(cd oracle  && sha256sum -c checksums.sha256)
sha256sum -c checksums.sha256
```
