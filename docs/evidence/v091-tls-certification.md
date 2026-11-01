# v0.9.1-c — Certificación del ciclo de secretos TLS (B-E9 fin)

**Objetivo.** Certificar, con el binario release y sockets reales, el ciclo
completo de un secreto TLS que [ADR-0025](../adr/0025-certificate-renewal-without-restart.md)
y [ADR-0035](../adr/0035-hot-secrets-and-drift-runbook.md) prometen: leer,
rotar, recargar por la acción admin y por `SIGHUP`, informar el fingerprint que
está en el cable, fallar cerrado y no tirar conexiones ya establecidas. No es
una funcionalidad nueva: es la evidencia que faltaba en la casilla 11 del
roadmap 0.9.1.

## 1. La certificación encontró un fallo real

El paso 4 del journey (candidato con certificado y clave de parejas distintas)
devolvía **`200`** y dejaba el listener vivo: `certified_key` construía el
`CertifiedKey` sin comprobar la correspondencia. El fallo no se veía al cargar,
solo en el siguiente handshake, así que una renovación mal escrita parecía
correcta y rompía el servicio después.

Corregido en [`certified_key`](../../eqproxy-net/src/tls.rs#L551): tras
construir el `CertifiedKey` se llama a `keys_match()` (rustls compara el SPKI
de la clave con el de la hoja). Una pareja cruzada falla al resolver el
snapshot; el arranque se niega y una recarga viva responde `400
invalid_config` mientras la generación anterior sigue sirviendo.

Prueba unitaria:
[`server_config_rejects_a_key_from_another_pair`](../../eqproxy-net/src/tls.rs#L850)
(la pareja correcta sigue cargando). Prueba con el binario: paso 4 de abajo.

## 2. Qué cubre «certificado»

| Superficie | Qué se exige | Evidencia |
|---|---|---|
| Arranque | el fingerprint de la hoja servida es el del fichero, y `status` publica el mismo | journey 1 |
| Recarga sin cambios | `200`, `changed=false`, misma hoja, mismo PID y puerto | journey 2 |
| Rotación del certificado por defecto | `200`, `changed=true`, hoja nueva en handshakes nuevos, **conexión keep-alive establecida intacta** | journey 3 |
| Fail-closed | candidato inválido → `400`, hoja anterior servida, `config_reload_failures` +1 y `ERROR config reload rejected` con la causa | journey 4 |
| `SIGHUP` | rota el conjunto SNI sin plano admin; el nombre recibe la hoja nueva y el certificado por defecto no cambia | journey 5 |
| Permisos de la acción | anónimo `401`, rol de lectura `403`, cuerpo `400 body_not_allowed`, `Origin` de navegador `403 origin_forbidden`, escritura `200` | journey 6 |
| Rotación de confianza del backend | CA propia → tráfico; recarga solo con Mozilla → túnel cerrado y `tls_handshake_failures` +1; CA de vuelta → tráfico en el mismo socket | `backend_tls_trust_rotation_applies_on_reload_without_restart` (test existente, no re-ejecutado por este journey) |
| Rotación de certificado en QUIC | nuevo TLS en el mismo socket UDP | `http3_reload_applies_new_tls_on_the_same_socket` (test existente, no re-ejecutado por este journey) |
| Multi-instancia / drift | detectar topologías y fingerprints distintos; reparar con el mismo fichero + `SIGHUP` | [v085-hot-secrets-drift](v085-hot-secrets-drift.md) (evidencia de 0.8.5, histórica) |

Las filas marcadas como test existente se citan como cobertura previa, no como
resultado de esta ejecución; el journey de esta casilla no las repite.

## 3. Journey con el binario release (macOS arm64)

Script (árbol del maintainer, local-only):
`scripts/evidence/v091-tls-certification/journey.sh`; cliente keep-alive TLS
real: `keepalive_client.py`. Salidas: [raw/](v091-tls-certification/raw/).
Artefacto: `target/release/eqproxy`, SHA-256
`768f10f25bdddcf6069426cee977c9697f2c9ca24e8bfe95b1d1cb6ae488e63b`
(entorno en [00-environment.txt](v091-tls-certification/raw/00-environment.txt)).

| # | Hecho | Raw |
|---|---|---|
| 1 | El fichero, el acceptor y `GET /api/v1/status` coinciden (hoja por defecto y SNI beta); contadores a cero | [01](v091-tls-certification/raw/01-initial-state.txt) |
| 2 | Recarga sin cambios: `200`, `changed=false`, mismo fingerprint, `config_reloads=1` | [02](v091-tls-certification/raw/02-noop-reload.txt) |
| 3 | Rotación con una conexión TLS keep-alive abierta: `first` y `second` responden `200` en el **mismo** socket; handshakes nuevos ven la hoja nueva; PID y puerto no cambian; SNI beta intacta; `changed=true`, `config_reloads=2` | [03](v091-tls-certification/raw/03-rotation-live.txt) |
| 4 | Cert/clave cruzados: `400 invalid_config` con «the private key does not match the certificate's public key»; hoja anterior y `status` sin cambios; `config_reload_failures=1`; línea `ERROR config reload rejected` | [04](v091-tls-certification/raw/04-fail-closed.txt) |
| 5 | `SIGHUP`: beta pasa a la hoja nueva, el certificado por defecto sigue igual, `config_reloads=3` | [05](v091-tls-certification/raw/05-sni-sighup.txt) |
| 6 | Modelo de permisos completo; la recarga autorizada mantiene `changed=false`; `config_reloads=4` | [06](v091-tls-certification/raw/06-permissions.txt) |
| 7 | Estado final: contadores, hojas servidas, PID vivo, log completo | [07](v091-tls-certification/raw/07-final.txt) |

El journey usa `set -euo pipefail` y falla si cualquiera de las invariantes
anteriores deja de cumplirse; salió **0** el 2026-11-01. El origin, el plano
admin y el cliente TLS son sockets reales; no hay mocks.

## 4. Tests

| Nivel | Comando | Resultado |
|---|---|---|
| Unit, TLS | `cargo test -p eqproxy-net --lib tls::tests` | **13 passed**, 0 failed (incluye la pareja cruzada) |
| Full workspace | `cargo test --workspace` | **697 passed, 0 failed, 2 ignored**, 33 targets (2026-11-01) |
| Formato / lint / build | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --release` | todos exit 0 |

Identificadores citados: `eqproxy-net::tls::tests::server_config_rejects_a_key_from_another_pair`,
`eqproxy-net::e2e_resilience::backend_tls_trust_rotation_applies_on_reload_without_restart`,
`eqproxy-net::e2e_http3::http3_reload_applies_new_tls_on_the_same_socket`.

Regresión: el journey de [v091-tls-gaps](v091-tls-gaps.md) (rango TLS y
`errorloc`) se re-ejecutó contra este mismo binario con salida a `/tmp` y
volvió a salir 0; no reemplaza los crudos de aquella casilla.

## 5. Auditorías de este incremento

| Pata | Comando | Resultado | Evidencia |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit 0 sobre el árbol final: rustfmt, check, clippy, test, nextest, llvm-cov, audit, deny, machete, veyra-e2e y anti-theater OK. `cargo-geiger` **INCONCLUSIVE** (cotas inferiores, no un pase). Skipped: dylint (sin configuración) y semver-checks (sin baseline) | `.veyra/evidence/20261101T022805Z/report.md` |
| Rust Audit | `python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit 1 por regla: **0 critical / 7 high / 417 medium / 110 low**. Los siete high son los ya triados en [v085-rust-audit-triage](v085-rust-audit-triage.md) (3 R16 de caché, 4 R09 del arnés de fuzz); este incremento no añade ninguno | `report/rust-audit-20261101-032920.md` (local-only) + `report/not-executed-2026-11-01.md` |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091-cert.json`; `agent impact "what breaks if certified_key starts rejecting a mismatched certificate and key"`; `agent context "where is a TLS certificate renewal validated and applied without restarting"` | índice **exit 0**: 84 ficheros, 81 052 líneas, 8 863 nodos, 15 600 aristas, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms 82 805` (incremental: 83 ficheros cacheados). Hechos comprobados en fuente: `certified_key` [tls.rs:551](../../eqproxy-net/src/tls.rs#L551), `keys_match` [:564](../../eqproxy-net/src/tls.rs#L564), `can_adopt_certificates`/`adopt_certificates` usados en [proxy.rs:1695](../../eqproxy-net/src/proxy.rs#L1695) y [:1736](../../eqproxy-net/src/proxy.rs#L1736). El agente no dio la línea de `apply` ni de `CERTIFICATE_RELOAD_PATH`; se localizaron a mano, no se citan como hallazgo del grafo | `/tmp/cg-v091-cert.json` |
| Hindsight | `hindsight-note --title "v0.9.1-c B-E9 TLS secret lifecycle certification" --project EQProxy --tags tls,certification,v091` | stored (`status: completed`), 9 ids; primero `81ffb9f3-ddc3-456c-bb0d-1ff333a0bb09` | banco `eqproxy`, `http://127.0.0.1:8888/mcp/eqproxy/` |

El índice se reconstruyó después del último cambio de Rust (el fix de
`keys_match` y su test) y antes de tocar la documentación. Las respuestas del
agente se usaron como punteros: las líneas citadas arriba se comprobaron en el
fuente.

## 6. Límites

- **Plataformas.** La corrida de la sección 3 es macOS arm64 (Darwin
  25.5.0, LibreSSL). Linux arm64 y la imagen Docker arm64 se ejecutaron
  después y se documentan en la sección 7. La suite Linux corrió como
  **root**; la corrida Linux sin privilegios sigue **NOT EXECUTED** aquí
  (la cubre CI, que ejecuta ambos uid).
- **QUIC.** La rotación de certificado sobre HTTP/3 está cubierta por el test
  de integración existente, no por este journey; el cambio en vivo de ALPN,
  `client_ca` o rango TLS en un endpoint QUIC sigue requiriendo reinicio, como
  decide ADR-0025.
- **Validez del certificado.** No se comprueba expiración ni `not_after`
  (documentado en ADR-0025); la certificación no lo presenta como hecho.
- **Multi-instancia.** La reconciliación atómica sigue fuera de alcance; la
  evidencia de drift es la de 0.8.5 y se cita como histórica.
- **Docker Scout.** No se ejecutó en este incremento. La imagen se construyó
  y se ejercitó (sección 7), pero no se publicó ni se pasó el gate de Scout:
  ausencia de evidencia, no un pase.

## 7. Linux y la imagen Docker (2026-11-01)

Mismo ciclo, otros dos entornos reales. Docker Desktop es **linux/arm64
nativo** (engine 29.9.2, kernel 7.0.14-linuxkit, 10 vCPU, 7,9 GB), sin
emulación. El contenedor de trabajo es `rust:1.99.1` (Debian 13 trixie,
rustc 1.99.1, Python 3.13.5, OpenSSL 3.5.7).

### 7.1 Suite completa en Linux

Comando (root en el contenedor, target en un volumen aparte):

```bash
docker run --rm --platform linux/arm64 \
  -v "$PWD":/work -v eqproxy-linux-target:/target -w /work \
  -e CARGO_TARGET_DIR=/target -e CARGO_INCREMENTAL=0 \
  rust:1.99.1 bash -c 'cargo test --workspace -j 2 -- --nocapture'
```

| Hecho | Resultado |
|---|---|
| Suite | **698 passed, 0 failed, 2 ignored**, 36 targets, exit **0** |
| Gate de `SKIP` de uid | `2 printed, 2 declared`; `check-uid-skips.py --root-log` exit **0** |
| Log completo | raw-linux/full-suite-root-nocapture.log (local-only: `v091-tls-certification/raw-linux/full-suite-root-nocapture.log`) y [resumen](v091-tls-certification/raw-linux/suite-summary.txt) |

El primer intento compiló con el paralelismo por defecto y el enlazador
murió con `collect2: ld terminated with signal 9 [Killed]`: el VM de Docker
tiene 7,9 GB y varios `ld` simultáneos no caben. No fue un fallo del código;
se repitió con `-j 2` y `CARGO_INCREMENTAL=0` sobre la caché y la suite pasó
entera. Correr como root es la mitad root del contrato de CI; la mitad sin
privilegios en Linux no se ejecutó aquí (**NOT EXECUTED**).

### 7.2 Journey B-E9 en Linux

El binario es el **musl estático extraído de la imagen** (`/opt/eqproxy`),
SHA-256 `fd453a8c9175ac2a73908ff18f2c5ab01eb89030aaaeee96403ec2ecf99d375b`,
ejecutado dentro del contenedor Debian con el mismo `journey.sh` y sockets
reales:

```bash
docker run --rm --platform linux/arm64 -v "$PWD":/work \
  -v /tmp/eqproxy-v091-linux:/usr/local/bin/eqproxy-linux:ro \
  -v /tmp/eqp-v091-raw-linux:/raw \
  -e BIN=/usr/local/bin/eqproxy-linux -e RAW=/raw \
  -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory \
  -e GIT_CONFIG_VALUE_0=/work \
  rust:1.99.1 bash scripts/evidence/v091-tls-certification/journey.sh
```

Salió **0**; crudos en [raw-linux/](v091-tls-certification/raw-linux/)
(entorno en
[00-environment.txt](v091-tls-certification/raw-linux/00-environment.txt)).
El paso 4 registra el rechazo con la causa exacta y el paso 3 conserva la
conexión keep-alive (`first` y `second` `200`) durante la rotación.

### 7.3 Imagen Docker

Construida con el Dockerfile del repo, `--platform linux/arm64`, sin tocar
el pin del builder:

```bash
docker build --platform linux/arm64 -f docker/Dockerfile -t eqproxy:v091-arm64 .
```

| Dato | Valor |
|---|---|
| Resultado | exit **0**, gate `file` del binario estático dentro del build |
| Imagen | `sha256:4a11955cac1c7e21ba27e5bd4791002f88599f28be7b58be2e3417c1e7ce36d4`, `linux/arm64`, 67 754 121 bytes |
| Binario | `sha256:fd453a8c9175ac2a73908ff18f2c5ab01eb89030aaaeee96403ec2ecf99d375b` (ELF aarch64, estático) |
| Base | `cgr.dev/chainguard/wolfi-base@sha256:360896…923a7`, uid 65532 |
| Registro | [raw-image/build.txt](v091-tls-certification/raw-image/build.txt) |

Journey de la imagen, exit **0**:

```bash
IMAGE=eqproxy:v091-arm64 \
  bash scripts/evidence/v091-tls-certification/docker-journey.sh
```

(`docker-journey.sh` es local-only.) El runtime Wolfi no trae
python/openssl/curl, así que
el arnés conduce el ciclo desde el host contra los puertos publicados en
`127.0.0.1`, y el origin corre en un contenedor hermano de la misma red
Docker: TCP y TLS reales, sin mocks.

| # | Hecho | Raw |
|---|---|---|
| 0 | `--version`, `--help`; `config validate` válido exit 0 y un `mode` inválido exit 1 con fichero:línea | [00](v091-tls-certification/raw-image/00-artifact.txt) · [01](v091-tls-certification/raw-image/01-packaged-cli.txt) |
| 1 | Hoja servida, SNI y `status` coinciden con los ficheros; backend `eqp-v091-origin-…` resuelto a `172.19.0.2`, `dns_resolution_failures=0` | [02](v091-tls-certification/raw-image/02-initial-state.txt) |
| 2 | Recarga sin cambios: `200`, `changed=false`, `config_reloads=1` | [03](v091-tls-certification/raw-image/03-noop-reload.txt) |
| 3 | Rotación: `first` y `second` `200` en el **mismo** socket TLS; hoja nueva en handshakes nuevos; PID del contenedor igual (`87324`); `changed=true`, `config_reloads=2` | [04](v091-tls-certification/raw-image/04-rotation-live.txt) |
| 4 | Cert/clave cruzados: `400`, hoja anterior sirviendo, `config_reload_failures=1`, `ERROR config reload rejected` con la causa en el log de la imagen | [05](v091-tls-certification/raw-image/05-fail-closed.txt) |
| 5 | `SIGHUP` a PID 1: beta pasa a la hoja nueva, el certificado por defecto no cambia, `config_reloads=3` | [06](v091-tls-certification/raw-image/06-sni-sighup.txt) |
| 6 | Permisos: anónimo `401`, lectura `403`, cuerpo `400 body_not_allowed`, `Origin` `403 origin_forbidden`, escritura `200` | [07](v091-tls-certification/raw-image/07-permissions.txt) |
| 7 | Final: `config_reloads=4`, `config_reload_failures=1`, contenedor `running=true`, PID sin cambios, log completo | [08](v091-tls-certification/raw-image/08-final.txt) |

Dos matices del arnés, declarados para que no se lean como comportamiento
del producto:

- El admin escucha en `0.0.0.0` con `allow_remote = true` **solo** porque
  Docker no alcanza el loopback del contenedor a través de un puerto
  publicado; el publish del host queda en `127.0.0.1` y la imagen emite el
  aviso de token en claro. La configuración por defecto sigue siendo loopback
  y el viaje de macOS (sección 3) lo usa así.
- La imagen no se publicó, no se promocionó ningún digest y **Docker Scout no
  se ejecutó** en este incremento: **NOT EXECUTED**, no un pase.

### 7.4 Auditorías de este delta

Sin cambios de Rust: este incremento solo añade evidencia y los arneses
(local-only). Las cuatro patas se repitieron sobre el árbol final.

| Pata | Comando | Resultado | Evidencia |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit **0**: rustfmt, check, clippy, test, nextest, llvm-cov, audit, deny, machete, veyra-e2e y anti-theater OK. `cargo-geiger` **INCONCLUSIVE**; dylint y semver-checks skipped | `.veyra/evidence/20261101T032349Z/report.md` |
| Rust Audit | `python3.13 /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit 1 por regla: **0 critical / 7 high / 417 medium / 110 low**; los siete high son los triados, sin hallazgos nuevos | `report/rust-audit-20261101-042410.md` (local-only) |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091-linuxdg.json`; `agent impact "what breaks if certified_key starts rejecting a mismatched certificate and key"`; `agent context "where is a TLS certificate renewal validated and applied without restarting"` | índice exit **0**: 84 ficheros, 81 052 líneas, 8 863 nodos, 15 600 aristas, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms 1 021 107` (re-embed completo). Hechos comprobados en fuente: `can_adopt_certificates` [tls.rs:104](../../eqproxy-net/src/tls.rs#L104), `adopt_certificates` [:113](../../eqproxy-net/src/tls.rs#L113), `BoundProxy::apply` [proxy.rs:1644](../../eqproxy-net/src/proxy.rs#L1644), `server_config` [tls.rs:382](../../eqproxy-net/src/tls.rs#L382), `load_and_check` [main.rs:258](../../eqproxy-cli/src/main.rs#L258). El agente devolvió hashes en vez de línea para `server_config`/`load_and_check`; se corrigieron a mano y no se citan como hallazgo | `/tmp/cg-v091-linuxdg.json` |
| Hindsight | `hindsight-note --title "v0.9.1-c B-E9 certification on Linux and the Docker image" --project EQProxy --tags tls,certification,docker,v091` | stored (`status: completed`), 9 ids; primero `abee119b-2a99-42a5-9801-a6bb629fb5e6` | banco `eqproxy` |

## 8. Cierre

Con el ciclo certificado en macOS, Linux y la imagen arm64, la casilla **11**
pasa a hecha y el roadmap 0.9.1 a **11/19**. La certificación no se limita a
repetir pruebas: encontró un fallo de correspondencia cert/clave que habría
hecho pasar una renovación rota.
