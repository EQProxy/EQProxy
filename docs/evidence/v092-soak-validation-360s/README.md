# v0.9.2 — validación del instrumento de soak (360 s)

Fecha: **2026-11-04**. Host `netcup-bench` (Linux x86_64, 8 CPU). Esta corrida
valida el arnés y la métrica corregida de FDs; **no es el gate de 24 h** y no
cierra la casilla 12.

## Resultado

Comando:

```bash
python3 scripts/soak.py --binary ./eqproxy-v093 --duration 6m \
  --rate 200 --workers 64 --out docs/evidence/v092-soak-validation-360s
```

Salida real:

```text
duration: 360.0s of 360s requested
requests: ok=72000 errors=0 bad_status=0
throughput: 200.0 rps (target 200.0)
latency ms: avg=2.664 p50=4 p95=4 p99=8
rss: steady=25.67 MiB (budget 64) peak=27.03 MiB (budget 128) measured=True
cpu: steady=0.016 cores (budget 0.5) measured=True
fds: peak=138 (budget 512) growth=7 net=7 range=7 (growth budget 16) measured=True
error rate: 0.0% (budget 0%)
shutdown: exit=0 clean=True
window: full=False warmup=True validation_only=True
VERDICT: VALIDATION PASS (not the 24 h gate)
```

`summary.json` confirma `pass=true`, `warmup_completed=true`,
`full_window=false` y **`gate_pass=false`**. El artefacto es
`eqproxy-v093` (`sha256 854fbd02…`), el mismo binario release usado en la
corrida de 24 h.

## Por qué esta corrida importa

La métrica anterior de FDs (`máximo − mínimo`) contaba una caída normal como
crecimiento y fue invalidada; la corrida abortada está en
`../v092-soak-24h-aborted-37min/`. La métrica corregida mide **máximo − primer
valor de régimen** y publica además `fd_net_growth=7` y `fd_range=7`. El
presupuesto sigue siendo **16**, sin cambiarlo para obtener un PASS.

La validación de 120 s anterior sigue en `../v092-soak-validation-120s/` como
FAIL de instrumento por la rampa inicial; no se usa como gate ni se oculta.
