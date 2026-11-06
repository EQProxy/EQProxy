# v0.9.2 — plan de soak y agotamiento (casilla 12)

Fecha de publicación: **2026-11-04**, antes de medir. Este documento fija los
presupuestos y el criterio de PASS/FAIL; los resultados se publican aparte en
`v092-soak-24h/` y `v092-soak-exhaust/`.

## Artefacto

- Binario release Linux amd64 extraído de la imagen `eqproxy-bench:v093`
  (`/opt/eqproxy`), no el binario debug.
- El hash SHA-256 del binario y el host se registran en cada `summary.json`.
- Una ejecución de 120 s solo valida el instrumento: **no es el gate**. El
  gate exige la ventana completa de **86 400 s**, 5 min de calentamiento y
  muestras cada 30 s.

## Soak de 24 h

Carga: HTTP/1.1 keep-alive, **200 rps objetivo**, **64 workers**, backend HTTP
real, `maxconn = 4096`, `maxqueue = 256`, `timeout_queue = 500ms`, `retries = 1`
y health check activo + pasivo. La configuración exacta queda en el directorio
de la corrida.

| Presupuesto | Valor | Cómo se comprueba |
|---|---:|---|
| RSS en régimen (media tras 5 min) | ≤ 64 MiB | `rss.csv` y `budgets.steady_rss_mib` |
| RSS pico | ≤ 128 MiB | `budgets.peak_rss_mib` |
| CPU media en régimen | ≤ 0,50 core | delta de `cpu_s` / tiempo entre muestras |
| FDs pico | ≤ 512 | `/proc/<pid>/fd` |
| Crecimiento de FDs en régimen | ≤ 16 | máximo − primer valor de la ventana (solo subida); el rango completo se publica como diagnóstico |
| Tasa de error | 0 % (errores y no-200) | contadores del cliente |
| Apagado SIGINT | exit 0 | `proxy_exit_code` |

El veredicto `PASS` solo se emite con `full_window: true` y
`warmup_completed: true`. Una corrida corta puede dar `VALIDATION PASS`, pero
`gate_pass` queda en `false` y no cierra la casilla.

## Agotamiento (tres escenarios)

Cada escenario arranca el binario real con una configuración real y un backend
TCP real; las conexiones y los cuerpos son reales. No se acepta un mock como
evidencia.

1. **FDs.** `RLIMIT_NOFILE = 64`, `maxconn = 64`, 200 clientes que se mantienen
   abiertos. Se exige: proceso vivo durante el agotamiento, recuperación con
   un `200` tras cerrar los clientes, sin panic/SIGSEGV, y salida limpia.
   Se registran los `accept failed` esperables de `EMFILE`.
2. **Cola.** Un solo servidor con `maxconn = 1`, `maxqueue = 1` y
   `timeout_queue = 200ms`, con un origen que tarda 1 s. Tres clientes
   simultáneos: exactamente **un 200**, **dos 503** y **una sola** petición
   en el backend.
3. **Memoria.** `limits.max_body_bytes = 65536` y un `POST` declarado de
   131 072 bytes. Se exige **413 antes del backend** (0 peticiones en el
   origen), recuperación con un `200` normal, proceso vivo y salida limpia.

## Comandos

```bash
python3 scripts/soak.py --binary ./eqproxy-v093 --duration 24h \
  --rate 200 --workers 64 --out docs/evidence/v092-soak-24h
python3 scripts/soak.py --binary ./eqproxy-v093 --duration 120s \
  --rate 200 --workers 64 --out /tmp/v092-soak-validation
python3 scripts/exhaust_v092.py --binary ./eqproxy-v093 \
  --out docs/evidence/v092-soak-exhaust
```

Los límites se declaran antes de la primera medida; si un escenario falla, se
publica el fallo y no se cambia el presupuesto para obtener un PASS.

## Enmienda de instrumento (2026-11-04 21:25 UTC)

La primera corrida de 24 h se detuvo a los 2 340 s: la métrica original
`máximo − mínimo` daba **25 FDs** porque los descriptores bajaron de 137 a 113,
no porque crecieran. Una caída por churn no es crecimiento. Se conserva el
presupuesto **≤ 16** y se corrige solo la operacionalización: `fd_growth` es
ahora **máximo − primer valor de régimen** (unilateral); `fd_net_growth` y
`fd_range` se publican aparte como diagnóstico. La corrida detenida queda en
`v092-soak-24h-aborted-37min/`, sin `summary`, y **no se usa para PASS/FAIL**.
El gate solo puede emitirse con la corrida nueva completa, `full_window` y
`warmup_completed`.

## Validación del instrumento (360 s, 2026-11-04)

La corrida de 6 min con el arnés corregido dio **VALIDATION PASS**: 72 000
peticiones, 0 errores, RSS 25,67/27,03 MiB, CPU 0,016 core, 138 FDs pico y
`fd_growth=7`; `warmup_completed=true`, `full_window=false` y
`gate_pass=false`. Es una validación del instrumento, **no el gate**. La
evidencia está en `v092-soak-validation-360s/`.
