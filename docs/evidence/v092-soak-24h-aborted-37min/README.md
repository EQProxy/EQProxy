# v0.9.2 — primera corrida de soak, ABORTADA por instrumento

## Estado: ABORTADA — no cuenta para el gate

La primera corrida de 24 h arrancó en `netcup-bench` el **2026-11-04 a las
20:43 UTC** (PID 1726470, proxy 1726471). Se detuvo a los **2 340 s** al
descubrir que la métrica publicada de FDs no medía crecimiento: el rango
`máximo − mínimo` contaba como crecimiento una caída normal de descriptores.

- Artefacto: `/root/eqproxy-v092/eqproxy-v093` (de `eqproxy-bench:v093`),
  SHA-256 `854fbd02ea2f6883e6aef209b8fbe6f544b394c9f900136056e0c648c29c2579`.
- Última muestra real: `elapsed_s=2340`, **468 032** respuestas OK, RSS
  **26 552 KiB**, **118 FDs**, **45,39 s** de CPU acumulada, 0 errores en
  `eqproxy.log`.
- Métrica anterior: primer valor de régimen **137 FDs**, mínimo posterior
  **113**, rango **25** → `fd_growth=25`, por encima del presupuesto 16.
  No hubo crecimiento neto: el último valor fue **123**; la caída es churn de
  conexiones, no una fuga.
- No se escribió `summary.json` ni `summary.txt`; el proceso se detuvo
  manualmente y no hay veredicto de producto. `runner.log` conserva la parada.

Los crudos permanecen en este directorio: `rss.csv` (80 líneas), `eqproxy.log`,
`soak.toml`, `soak.pid` y `runner.log`. Esta corrida **no se cita como PASS**
ni como FAIL del producto; solo como evidencia de la corrección del arnés.

La nueva métrica, publicada en `../v092-soak-plan.md`, conserva el presupuesto
de **16** y mide **máximo − primer valor de régimen** (solo subida). El rango
completo y el crecimiento neto se publican como diagnóstico.
