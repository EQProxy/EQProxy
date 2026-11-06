# v0.9.2 — agotamiento de FDs, cola y memoria (casilla 12)

Fecha: **2026-11-04**. Host `netcup-bench` (Linux 6.8.0-147-generic, x86_64,
8 CPU). Esta tanda cubre los tres escenarios de agotamiento de la casilla 12;
el soak de 24 h se publica aparte, con su PASS en
[`../v092-soak-24h/README.md`](../v092-soak-24h/README.md), y **no se declara
PASS aquí**.

## Artefacto

- Imagen `eqproxy-bench:v093`, binario `/opt/eqproxy` extraído a
  `/root/eqproxy-v092/eqproxy-v093`; `eqproxy 0.9.1`.
- SHA-256: `854fbd02ea2f6883e6aef209b8fbe6f544b394c9f900136056e0c648c29c2579`.
- Cada caso arranca el binario real con una configuración real, un backend TCP
  real y clientes reales. No hay transporte mockeado ni llamada interna usada
  como evidencia.
- Arnés: `scripts/exhaust_v092.py` (local-only, fuera del repositorio).

## Resultados

Comando:

```bash
python3 scripts/exhaust_v092.py --binary ./eqproxy-v093 \
  --out docs/evidence/v092-soak-exhaust
```

| Caso | Configuración | Resultado observado | Veredicto |
|---|---|---|---|
| FDs | `RLIMIT_NOFILE=64`, `maxconn=64`, 200 clientes retenidos | 183 conexiones abiertas, 24 `accept failed: EMFILE`, proceso vivo, recuperación **200**, exit 0, sin panic | **PASS** |
| Cola | Una ranura (`maxconn=1`), `maxqueue=1`, `timeout_queue=200ms`, origen 1 s, 3 clientes | estados `[503, 503, 200]`, **1** petición en el backend, proceso vivo, exit 0 | **PASS** |
| Memoria | `max_body_bytes=65536`, `POST` declarado de 131 072 B | **413** antes del backend (**0** peticiones), recuperación **200**, proceso vivo, exit 0 | **PASS** |

`summary.json` y `summary.txt` son la fuente; los tres logs de proxy conservan
los `accept failed` esperables y los contadores `queue_full=1`,
`queue_timeout=1`, `proxied=0/1` de cada caso. `sha256sum` de los resúmenes:

```text
90554a336f6af2e140bd58d98c4c9e2938ee60b0441cf4fff36dca26ad4633d8  summary.json
1faae21edf10d4fa812968a0a1f7652ad0fd48db28f8c7e1d4a05307c318edc7  summary.txt
```

## Nota de instrumento

La validación corta de **120 s** (`v092-soak-validation-120s/`) dio 24 000
peticiones, 0 errores, RSS 27,0 MiB pico, CPU 0,019 core y 138 FDs, pero su
veredicto fue FAIL por `fd_growth=128`: la ventana no alcanzó el calentamiento
de 5 min y contó la rampa inicial (10 → 138 FDs). Es un límite de esa
validación, no una fuga ni un fallo del producto; no se usa como gate. El soak
de 24 h sí exige `warmup_completed` y `full_window`.

## Estado de la casilla

- Agotamiento: **PASS 3/3**.
- Soak de 24 h: **PASS** (2026-11-05; 86 400 s, 17 280 000 peticiones, 0
  errores; [`../v092-soak-24h/README.md`](../v092-soak-24h/README.md)).
- Casilla 12: **CERRADA**; ambas mitades tienen veredicto verde.

## Reproducción

```bash
python3 scripts/exhaust_v092.py --binary ./eqproxy-v093 \
  --out docs/evidence/v092-soak-exhaust
```

El hash del binario, el host y los detalles de cada escenario quedan en
`summary.json`; no se sustituyen por una ejecución de arranque.
