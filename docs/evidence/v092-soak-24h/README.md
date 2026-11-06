# v0.9.2 — soak de 24 h, corrida definitiva (casilla 12)

## Estado: PASS — casilla 12 cerrada

Iniciado en `netcup-bench` el **2026-11-04 a las 21:31 UTC** y terminado el
**2026-11-05 a las 21:31 UTC**. Es la segunda corrida, con el arnés corregido;
la primera está preservada como `../v092-soak-24h-aborted-37min/` y **no
cuenta**.

- Proceso `soak.py`: PID **1728946**; proxy release: PID **1728948**. Ambos
  terminaron al cerrar la ventana; no quedan procesos.
- Artefacto: `/root/eqproxy-v092/eqproxy-v093`, extraído de
  `eqproxy-bench:v093`; SHA-256
  `854fbd02ea2f6883e6aef209b8fbe6f544b394c9f900136056e0c648c29c2579`,
  `eqproxy 0.9.1`.
- Carga: HTTP/1.1 keep-alive, 200 rps objetivo, 64 workers, dos backends HTTP
  reales, `maxconn=4096`, `maxqueue=256`, `timeout_queue=500ms`, `retries=1` y
  health check activo + pasivo.
- Presupuestos: [`../v092-soak-plan.md`](../v092-soak-plan.md). La métrica de
  FDs es unilateral (`máximo − primer valor de régimen`), con presupuesto 16;
  `fd_net_growth` y `fd_range` se publican como diagnóstico.
- Comando:
  `python3 scripts/soak.py --binary ./eqproxy-v093 --duration 24h --rate 200 --workers 64 --out docs/evidence/v092-soak-24h`.

## Resultado

| Métrica | Resultado | Presupuesto |
|---|---:|---:|
| Duración | 86 400.0 s | 86 400 s |
| Peticiones | 17 280 000 OK; 0 errores; 0 bad status | 0 % de error |
| Throughput | 200.0 rps | 200 rps objetivo |
| Latencia | avg 2.245 ms; p50 4; p95 4; p99 8 ms | — |
| RSS en régimen | 25.96 MiB | ≤ 64 MiB |
| RSS pico | 27.02 MiB | ≤ 128 MiB |
| CPU media | 0.017 cores | ≤ 0.50 cores |
| FDs pico | 138 | ≤ 512 |
| Crecimiento de FDs | 8 | ≤ 16 |
| Apagado | exit 0, limpio | exit 0 |
| Ventana | `full_window=true`, `warmup_completed=true` | ambas verdaderas |
| Veredicto | `pass=true`, **`gate_pass=true`** | PASS |

Diagnóstico adicional: `fd_net_growth=3`, `fd_range=24`. Las dos mitades del
tráfico fueron reales: 8 640 000 peticiones al backend A y 8 640 000 al B.
`validation_only=false`; no es una corrida corta.

`summary.txt` completo:

```text
EQProxy soak summary
duration: 86400.0s of 86400s requested
requests: ok=17280000 errors=0 bad_status=0
throughput: 200.0 rps (target 200.0)
latency ms: avg=2.245 p50=4 p95=4 p99=8
rss: steady=25.96 MiB (budget 64) peak=27.02 MiB (budget 128) measured=True
cpu: steady=0.017 cores (budget 0.5) measured=True
fds: peak=138 (budget 512) growth=8 net=3 range=24 (growth budget 16) measured=True
error rate: 0.0% (budget 0%)
shutdown: exit=0 clean=True
window: full=True warmup=True validation_only=False
VERDICT: PASS
```

## Evidencia final

Los cinco archivos finales se copiaron desde
`netcup-bench:/root/eqproxy-v092/docs/evidence/v092-soak-24h/` y sus SHA-256
coinciden con los del host:

```text
1ab4f675da4b5510f71cdc29d68a656b5199379617425f05656fd54b7401dc38  summary.json
d1403890d39f3444fddb1ae89f87192e47f3b1be31915b97e8d3b91d6c7f0be9  summary.txt
26036b46466487fa5c1333134f00625c44c49119e5d7d2572a679205b35227a9  runner.log
4169595cc1cd6dd4a17fc4f0eb003e84b790561a0223869650a1a946963a7ccf  rss.csv
e30b1c152bdba648b94e31234eebfe647ca71a8e3f236bacdc05ee3516ffe545  eqproxy.log
```

`summary.json` es la fuente estructurada; `summary.txt` y `runner.log`
conservan la salida del cierre; `rss.csv` tiene las 2 880 muestras de 30 s y
`eqproxy.log` el log completo del proxy. `soak.toml` y `soak.pid` permanecen
junto a ellos.

## Cierre de la casilla

- Soak de 24 h: **PASS**, con la ventana completa y el apagado limpio.
- Agotamiento de FDs, cola y memoria: **PASS 3/3**; ver
  [`../v092-soak-exhaust/README.md`](../v092-soak-exhaust/README.md).
- Casilla 12: **CERRADA** el 2026-11-05 con ambas mitades verdes.

La validación de 360 s sigue siendo **VALIDATION PASS**, no el gate; la de
120 s falló por la rampa inicial de FDs y se conserva como fallo de
instrumento. Ninguna de las dos se usa para este PASS. Los presupuestos no se
cambiaron después de medir.

## Límites

La corrida es de un solo host y una sola arquitectura: `netcup-bench`,
Linux x86_64. La carga es HTTP/1.1 keep-alive; no cubre h2/h3 ni TLS. No
sustituye el veredicto de producción de la casilla 19 ni una certificación
arm64.
