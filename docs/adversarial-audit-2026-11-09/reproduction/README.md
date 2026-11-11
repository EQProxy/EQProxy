# Reproducción — auditoría adversarial EQProxy 2026-11-09

El laboratorio E2E ejecutable ya **no vive bajo `docs/`**: se movió a
`scripts/evidence/adversarial-2026-11-09/run_lab.py` (tooling local-only, fuera
del repositorio publicado). Esta carpeta conserva el README y los datos
registrados; `docs/` no contiene scripts ejecutables.

El hash del arnés en el momento de la auditoría era
`f130fd41d00aec46cad8b1d0784b28d67cdb4e1fb32ab85c6a74104d1d414929`; la copia
usada por la evidencia final de v0.9.5 incluye S12 h1 y el probe h2c en crudo
que sustituye al cliente curl para la petición h2. La ruta y el comando de uso
actuales están en el README de `scripts/evidence/adversarial-2026-11-09/`.

---

## 1. Requisitos

| Requisito | Versión usada | Notas |
|---|---|---|
| SO | macOS 26.5.1 arm64 | El laboratorio es Python + sockets; debería funcionar en Linux sin cambios. |
| Rust/Cargo | 1.99.1 | Para compilar el binario. |
| Python | 3.14.6 | Solo biblioteca estándar (sin `pip install`). |
| curl | 8.7.1 con nghttp2 | Necesario para S10/S11 (`--http2-prior-knowledge`); S12 usa frames h2c en crudo. |
| `target/release/eqproxy` | build de esta auditoría | SHA-256 `d68ec1dc…e1a44`. |

El script **no usa mocks**: arranca el binario real y dos servidores TCP reales
en `127.0.0.1`, y habla HTTP a nivel de socket. Los puertos son efímeros; no
hay puertos fijos que liberar.

Si `curl` no soporta h2c, S10/S11 se marcan como `BLOCKED` y el resto
sigue ejecutándose; el probe S12 h2 no depende de curl. En ese caso no se
puede confirmar la respuesta h2 con este arnés.

---

## 2. Compilar el binario

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo build --release -p eqproxy-cli
shasum -a 256 target/release/eqproxy
```

Debe existir `target/release/eqproxy`. El script falla con un mensaje claro si
no está.

---

## 3. Ejecutar el laboratorio

### Forma canónica (tooling local-only)

```bash
cd /Volumes/Lexar/Cursor/EQProxy
python3.13 scripts/evidence/adversarial-2026-11-09/run_lab.py
echo "EXIT:$?"
```

Genera:

- `scripts/evidence/adversarial-2026-11-09/eqproxy.toml` (config temporal),
- `scripts/evidence/adversarial-2026-11-09/eqproxy.log` (stdout/stderr del proxy),
- `scripts/evidence/adversarial-2026-11-09/results.json` (resultado por escenario),
- salida PASS/FAIL por consola.

La antigua copia archivada bajo `docs/adversarial-audit-2026-11-09/reproduction/`
se eliminó al mover el arnés: `docs/` solo lleva Markdown y datos.

---

## 4. Resultado en v0.9.5 (cerrado)

Con el árbol de release y el binario `0.9.5`:

```
14/14 scenarios passed
EXIT:0
```

S9, S10, S12 h2 y S12 h1 pasan. La evidencia cruda está en
`docs/evidence/v095-p0-3-p0-4-results.json`. La tabla 10/13 de la auditoría
documenta el estado anterior y ya no es el resultado esperado.

---

## 5. Qué hace cada escenario (resumen)

- **S1 — AUD-01:** un `304` con `Set-Cookie`/`X-Injected` revoca la entrada;
  el siguiente cliente recibe cuerpo fresco y no ve esas cabeceras.
- **S2 — AUD-02:** `Cache-Control: max-age=0` de un cliente no contamina a
  otro; solo fuerza la revalidación de su propia petición.
- **S3 — request smuggling:** CL+TE, CL duplicado, TE no chunked y espacio
  antes de `:` deben dar `400`; nada llega al origen.
- **S4 — inyección de cabeceras:** CR/LF desnudo y obs-fold deben dar `400`.
- **S5 — path traversal:** normalización correcta y rechazo de `..` por encima
  de la raíz.
- **S6 — Host:** ausente o duplicado en HTTP/1.1 debe dar `400`.
- **S7 — Vary:** variantes `gzip`/`identity` no se cruzan.
- **S8 — HEAD:** se sirve desde una entrada `GET`, sin cuerpo y con `CL`.
- **S9 — ADV-01:** backend `204` + `TE: chunked` con respuesta forjada
  retrasada; el segundo cliente no debe recibirla.
- **S10 — ADV-02 respuesta:** upstream `Connection: X-Secret` + `X-Secret`;
  el cliente h2 no debe ver `x-secret`.
- **S11 — h2c básico:** control positivo de traducción h2↔h1.
- **S12 — ADV-02 petición (h2c):** frames h2c en crudo con `Connection`;
  el protocolo h2 los rechaza antes de la traducción y el origen no recibe la
  petición.
- **S12 — ADV-02 petición (h1):** `Connection: X-Evil` + `X-Evil` en el
  listener HTTP/1.1 real; el origen ve `host` y `x-keep`, nunca `x-evil`, y la
  variante que nomina `content-length` se rechaza con `400`.

Detalle de implementación: `run_lab.py`, clase `Origin` (método `respond`) y
`main` (bloques `# --- S…`).

---

## 6. Reproducir las pruebas de propiedades

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo test -p eqproxy-net --test adversarial_properties
```

Esperado: `11 passed; 0 failed` (exit 0). No necesita red ni binario release.
El fichero es `eqproxy-net/tests/adversarial_properties.rs`; usa un PRNG
xorshift determinista, así que el resultado es reproducible.

---

## 7. Reproducir el fuzzing

Requiere `cargo-fuzz` y toolchain nightly:

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo +nightly fuzz run framing fuzz/corpus/framing -- \
  -max_total_time=120 -rss_limit_mb=2048 -print_final_stats=1
cargo +nightly fuzz run config_import fuzz/corpus/config_import -- \
  -max_total_time=120 -rss_limit_mb=2048 -print_final_stats=1
cargo +nightly fuzz run rules fuzz/corpus/rules -- \
  -max_total_time=120 -rss_limit_mb=2048 -print_final_stats=1
```

Esperado en el árbol auditado: exit 0, sin ficheros en `fuzz/artifacts/`.
Los corpus crecen en cada ejecución; es normal. Para un corte limpio se puede
usar un corpus temporal:

```bash
mkdir -p /tmp/eqproxy-corpus-framing
cargo +nightly fuzz run framing /tmp/eqproxy-corpus-framing -- -max_total_time=60
```

No se deben borrar los corpus de `fuzz/corpus/` sin querer: son material de
partida, no evidencia de un fallo.

---

## 8. Limpieza

El script termina el proceso `eqproxy` en un bloque `finally` y los orígenes
son hilos daemon; al salir no queda nada escuchando. Para limpiar artefactos
generados por ejecuciones del laboratorio:

```bash
cd /Volumes/Lexar/Cursor/EQProxy

# Solo artefactos regenerables del laboratorio:
rm -f report/adversarial-2026-11-09/lab/eqproxy.toml \
      report/adversarial-2026-11-09/lab/eqproxy.log \
      report/adversarial-2026-11-09/lab/results.json
rm -rf report/adversarial-2026-11-09/lab/__pycache__

# Artefactos del harness movido a scripts/:
rm -f scripts/evidence/adversarial-2026-11-09/eqproxy.toml \
      scripts/evidence/adversarial-2026-11-09/eqproxy.log \
      scripts/evidence/adversarial-2026-11-09/results.json
rm -rf scripts/evidence/adversarial-2026-11-09/__pycache__
```

**No borrar** `report/adversarial-2026-11-09/*.log` ni
`lab/results.json`: son la evidencia primaria de la auditoría. Si se quiere
conservar una ejecución nueva, copiarla antes de sobrescribir.

Si un `eqproxy` quedara vivo por un corte abrupto (Ctrl-C durante el arranque):

```bash
pgrep -fl 'target/release/eqproxy'   # revisar antes de matar
pkill -f 'target/release/eqproxy'    # solo si es el del laboratorio
```

Los puertos son efímeros; no hace falta liberar puertos concretos.

---

## 9. Solución de problemas

| Síntoma | Causa probable | Acción |
|---|---|---|
| `eqproxy did not bind both listeners` | Binario ausente, config inválida o puerto ocupado | Ver `lab/eqproxy.log`; recompilar; reintentar (los puertos se reservan de nuevo). |
| S10/S11/S12 `BLOCKED: curl h2c unavailable` | curl sin nghttp2 o demasiado antiguo | Usar un curl con HTTP/2 o instalar `nghttp2`; sin esto ADV-02 no se puede confirmar E2E. |
| El laboratorio sale 0 inesperadamente | Se corrigieron ADV-01/ADV-02 | Verificar que S9/S10/S12 están en PASS y que no se relajó la aserción. |
| `Address already in use` | Otra instancia del laboratorio o un proceso ajeno | Matar la instancia previa; el script elige puertos libres, pero hay una carrera mínima. |
| Fuzz `rules` no compila | Regresión del harness (`RequestFacts`) | Comprobar que `fuzz/fuzz_targets/rules.rs` incluye `maps`. |

---

## 10. Evidencia de referencia

- `report/adversarial-2026-11-09/lab-run.log` — salida PASS/FAIL.
- `report/adversarial-2026-11-09/lab/results.json` — datos crudos por escenario.
- `report/adversarial-2026-11-09/lab/eqproxy.toml` — configuración generada.
- `report/adversarial-2026-11-09/workspace-test.log` — suite base.
- `report/adversarial-2026-11-09/fuzz-*.log` — campañas.
- `report/adversarial-2026-11-09/property-tests.log` — propiedades.
- `report/adversarial-2026-11-09/build-release.log` — build y hash.
- `docs/adversarial-audit-2026-11-09/findings/` — fichas de hallazgos.
