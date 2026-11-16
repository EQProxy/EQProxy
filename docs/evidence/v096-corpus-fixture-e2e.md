# v0.9.6 — corpus nginx completo: import + validate + run con certificados de fixture

Fecha: 2026-11-14. Rama `release/v0.9.5`, HEAD `ae85cf9`, árbol con los cambios
locales ya existentes; este incremento no modifica el corpus ni el código de
producción. Todo el tráfico es loopback real en macOS; no hay stubs, mocks ni
respuestas cableadas.

## Objetivo y resultado

Cerrar para el corpus la limitación de los certificados `/etc/ssl` ausentes:
se copia temporalmente
[`v085-config-import/raw/nginx.conf`](v085-config-import/raw/nginx.conf), se
sustituyen sus certificados por fixtures generados de verdad y se ejecuta el
binario real de punta a punta: `config import`, `config validate` y `run` con
tráfico HTTP/2, dos vhosts TLS y sus backends.

**Resultado:** `import` exit **2** (contrato esperado: 4 directivas sin
equivalente), `validate` exit **0**, `run` sirvió `/`, `/api/` y el vhost
`panel.ejemplo.com`; con el origen apagado respondió **502**. La configuración
importada tiene **1 listener y 3 backends**. El bloque de nginx que solo hace
`return 301` en el puerto 80 no genera un listener (el conversor lo deja como
aviso manual); por eso el recorrido de tráfico usa el listener TLS convertido.

## Artefacto y entorno

| Dato | Valor |
|---|---|
| Binario | `target/debug/eqproxy`, `eqproxy 0.9.6` |
| SHA-256 del binario | `307ee25480a6e0de8dd552d38b0ed50fa3eed49072539f9f331cda44a5114fc3` |
| Fuente | HEAD `ae85cf9` + árbol local sucio; el corpus no se ha editado |
| Host | macOS, loopback; sin privilegios para puertos <1024 |
| Cliente | `curl 8.7.1` (nghttp2/1.68.1), `LibreSSL 3.3.6` |
| Certificados | fixtures reales generados por el build en `target/debug/build/eqproxy-cli-*/out/fixtures` |
| Fixture principal | `alpha.pem` / `alpha.key`, sujeto `CN=alpha.example.test` |
| Fixture del panel | `beta.pem` / `beta.key`, sujeto `CN=beta.example.test` |
| SHA-256 `alpha.pem` | `8d6fa26145cceead6b7ac8c59cf0612ac006056aad2e4cbbb4dc9c65fe03c34a` |
| SHA-256 `beta.pem` | `85492a91dff1106a0e6c849b9e573fbece8d57c62e2fcfdc90b33f052d3996c0` |
| Harness local | `scripts/evidence/v096-corpus-fixture-e2e/journey.sh` (local-only) |
| Salida cruda | [`v096-corpus-fixture-e2e/raw/`](v096-corpus-fixture-e2e/raw/) |

Las claves privadas de fixture se copian a un directorio temporal con modo
`0600` y no se guardan en `docs/`. Los ficheros `.pem` de la evidencia son
certificados públicos obtenidos por SNI.

## Copia temporal: qué se cambia y qué se conserva

Se conservan el `http {}`, los dos upstreams, los tres `server`, los
`server_name`, las locations, `http2 on`, `ssl_protocols`, `gzip`,
`proxy_set_header`, `add_header`, `auth_basic`, `return 301` y las relaciones
entre vhosts. Los cambios son solo los necesarios para poder ejecutar en esta
máquina:

1. `/etc/ssl/certs/ejemplo.pem` y `/etc/ssl/private/ejemplo.key` →
   fixtures `alpha.pem` / `alpha.key`.
2. `/etc/ssl/certs/panel.pem` y `/etc/ssl/private/panel.key` →
   fixtures `beta.pem` / `beta.key`.
3. `listen 80` y `listen 443 ssl` → dos puertos altos dinámicos, porque el
   proceso corre sin privilegios.
4. Los seis destinos `10.0.0.x` / `10.0.1.x` / `10.0.2.5` → seis orígenes HTTP
   reales en `127.0.0.1` con puertos dinámicos, para que `run` pueda ejercer
   los backends y las reglas `host` en lugar de devolver 502 por una red
   ausente.

El diff completo está en [`raw/source.diff`](v096-corpus-fixture-e2e/raw/source.diff);
la copia ejecutada queda en [`raw/nginx.conf`](v096-corpus-fixture-e2e/raw/nginx.conf).
El `nginx.conf` original del corpus no se ha modificado.

## Comandos y resultados

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo build -p eqproxy-cli
# exit 0

scripts/evidence/v096-corpus-fixture-e2e/journey.sh
# exit 0
```

El harness ejecuta, con esos puertos y certificados temporales:

```bash
target/debug/eqproxy config import \
  --input "$WORK/nginx.conf" \
  --output "$WORK/eqproxy.toml" \
  --report "$WORK/report.md"

target/debug/eqproxy config validate --config "$WORK/eqproxy.toml"

target/debug/eqproxy run --config "$WORK/eqproxy.toml"
```

Resultado literal de [`raw/journey.txt`](v096-corpus-fixture-e2e/raw/journey.txt):

```text
PASS import-exit: 2
PASS validate-exit: 0
PASS main-status: 200
PASS api-status: 200
PASS panel-status: 200
PASS origin-down-status: 502
PASS main-body-route: origin-64007|path=/|host=ejemplo.com:64006
PASS api-body-route: origin-64010|path=/api/|host=ejemplo.com:64006
PASS panel-body-route: origin-64012|path=/|host=panel.ejemplo.com:64006
PASS sni-ejemplo-cert: subject= /CN=alpha.example.test
PASS sni-panel-cert: subject= /CN=beta.example.test
PASS api-response-header: X-Frame-Options SAMEORIGIN
PASS main-protocol: HTTP/2
main_body=origin-64007|path=/|host=ejemplo.com:64006
api_body=origin-64010|path=/api/|host=ejemplo.com:64006
panel_body=origin-64012|path=/|host=panel.ejemplo.com:64006
sni_ejemplo=subject= /CN=alpha.example.test
sni_panel=subject= /CN=beta.example.test
origin_down_code=502
```

Salida de import y validate:

```text
import: …/nginx.conf (nginx) → …/eqproxy.toml: 25 converted, 0 degraded, 33 with notes, 4 rejected
  the generated file is valid; 4 directive(s) have no EQProxy equivalent yet (see the report)
config OK: 1 listener(s), 3 backend(s) in …/eqproxy.toml
```

El informe generado está en
[`raw/report.md`](v096-corpus-fixture-e2e/raw/report.md) y el TOML en
[`raw/eqproxy.toml`](v096-corpus-fixture-e2e/raw/eqproxy.toml). Las 4
directivas rechazadas son las que el corpus ya declaraba sin equivalente:
`error_log`, `root`/`expires` de `/static/` y una location sin proxy; el
`return 301` del puerto 80 queda como aviso para crear la redirección a mano.
No se convierte el bloque HTTP en un listener y no se prueba su redirección en
este recorrido.

> **Actualización posterior (2026-11-14).** El conversor ya traduce ese bloque
> a un listener con `[listener.*.redirect]` y sin backend. La prueba vigente es
> `imported_redirect_only_listener_serves_301_without_a_backend` en
> `eqproxy-cli/tests/cli_e2e.rs`: importa un `server` de solo `return 301`,
> valida el TOML generado y sirve un 301 real con el binario. Este documento y
> sus artefactos `raw/` se conservan como evidencia histórica del recorrido
> anterior.

## Evidencia adjunta

| Fichero | Contenido |
|---|---|
| [`raw/environment.txt`](v096-corpus-fixture-e2e/raw/environment.txt) | fecha, binario, hashes, fixtures y puertos |
| [`raw/source.diff`](v096-corpus-fixture-e2e/raw/source.diff) | cambios de la copia temporal |
| [`raw/import.txt`](v096-corpus-fixture-e2e/raw/import.txt), `import.exit` | salida y código de import |
| [`raw/validate.txt`](v096-corpus-fixture-e2e/raw/validate.txt), `validate.exit` | salida y código de validate |
| [`raw/run.log`](v096-corpus-fixture-e2e/raw/run.log) | arranque, métricas y parada de `eqproxy run` |
| [`raw/origin.log`](v096-corpus-fixture-e2e/raw/origin.log) | peticiones observadas por los seis orígenes |
| `main.*`, `api.*`, `panel.*` | cuerpos, cabeceras y códigos de las tres rutas |
| `sni-ejemplo.pem`, `sni-panel.pem` | certificados públicos seleccionados por SNI |
| [`raw/journey.txt`](v096-corpus-fixture-e2e/raw/journey.txt) | veredictos de la ejecución |
| [`raw/baseline.log`](v096-corpus-fixture-e2e/raw/baseline.log) | fmt, Clippy, suite completa y build release |

## Baseline del workspace

Después de la ejecución E2E se ejecutó el baseline de la guía del repositorio en
el árbol actual:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Los cuatro comandos terminaron con exit **0**. La suite completa dio **850
tests pasados, 0 fallos, 41 suites**; el build release terminó en 23,08 s. La
salida completa, incluidos los recuentos por suite, está en
[`raw/baseline.log`](v096-corpus-fixture-e2e/raw/baseline.log). Esto no cambia
el hecho de que la ejecución E2E usa el binario `target/debug/eqproxy` citado
arriba: el build release es una comprobación adicional, no el artefacto del
journey.

## Límites

- La copia usa **fixtures** `alpha`/`beta`, no los certificados originales del
  host de origen. Se prueba que `validate` resuelve y `run` carga esos PEM, no
  que los certificados reales de `/etc/ssl` existan en esta máquina.
- Los orígenes son seis `ThreadingHTTPServer` reales en loopback, no nginx ni
  los backends `10.0.0.x` del corpus. Se prueba el cruce de tráfico por el
  listener convertido, las reglas `host`, el backend API y el SNI; no la
  equivalencia de comportamiento con nginx ni el balanceo real de la red.
- El puerto 80 del corpus no se ejecuta: el conversor no genera listener para un
  bloque que solo redirige. `auth_basic` y `/static/` tampoco se ejecutan; el
  informe los deja como revisión/rechazo.
- Es macOS loopback con build `debug`; no se ha probado en Linux, Docker,
  arm64 ni con orígenes remotos. No se ejecutaron Veyra Proof, Rust Audit ni
  CodeGraph para este incremento documental (no cambia código de producto); se
  reportan como **NOT EXECUTED**, no como PASS. El baseline de Rust sí se
  ejecutó y está arriba. No hay commit, push ni tag.

## Reproducir

```bash
cargo build -p eqproxy-cli
scripts/evidence/v096-corpus-fixture-e2e/journey.sh
```

El harness busca los fixtures generados en `target/debug/build/eqproxy-cli-*/out/fixtures`.
Si no existen, hay que compilar los tests/build de `eqproxy-cli` primero; no
inventa certificados.

## Memoria

La consulta de apertura `hindsight-recall --area config,evidence --check-tree`
respondió exit 2 (el banco no tenía memorias coincidentes); no se aplicó ni se
sustituyó ningún id. El cierre se escribió con `hindsight-note --kind evidence`:
doc `25889816-9575-4846-a274-2dd611a423b5`. Veyra Proof, Rust Audit y CodeGraph
quedan **NOT EXECUTED** para este incremento documental, no como PASS.
