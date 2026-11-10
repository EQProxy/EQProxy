# v0.9.5 / EXT-H — reproducción de los seis hallazgos de la auditoría externa

Fecha: 2026-11-10. Rama `release/v0.9.5`, HEAD `ecda55a`, con los cambios
sin commitear de 0.9.5 que ya estaban en el árbol (`cache.rs`, `http1.rs`,
`e2e_cache.rs`, ADR-0034, fuzz). No se ha modificado código de producción,
no se ha commiteado, pusheado, taggeado ni publicado nada.

Los seis hallazgos `EXT-H-01…EXT-H-06` del informe externo (estático, sobre
`95ca56c`) **se reproducen con el binario actual**. Ninguno se corrige en este
documento: el objetivo era separar el fallo real de la hipótesis estática.

## Artefacto y entorno

| Dato | Valor |
|---|---|
| Binario | `target/debug/eqproxy`, `eqproxy 0.9.4` |
| SHA-256 | `4c20a197d90d4e55b6f186b7dcac276565c4fdf32cc0f0ee3c34a5247d623bc2` |
| Fuente | HEAD `ecda55a46f91982842f8bfe87772bf44324df905` + árbol sucio de 0.9.5 |
| Host | macOS, loopback |
| Cliente | `curl 8.7.1` (nghttp2/1.68.1); `openssl` LibreSSL 3.3.6 |
| Harness | `scripts/evidence/ext-h-2026-11-10/repro.py` (local-only) |
| Salida cruda | [`raw/v095-ext-h-repro.log`](raw/v095-ext-h-repro.log) |

Método: cada caso arranca el binario real con un `eqproxy.toml` real y habla
por sockets reales. Los pares son reales, no dobles:

- un origen HTTP/1.1 real (Python `ThreadingHTTPServer`) con contador de hits;
- un servidor TLS real con una CA y una hoja cuyo único SAN es
  `DNS:localhost`, para separar «verificar el nombre» de «verificar la IP»;
- una segunda instancia de EQProxy con TLS+ALPN `h2` y una regla
  `direct_response 418`, que marca «la petición cruzó el gate»;
- dos listeners del mismo proceso para el cupo de conexiones.

## Resultado

| ID | Hallazgo | Veredicto | Evidencia observable |
|---|---|---|---|
| EXT-H-01 | HIT con cuerpo y `Content-Encoding`/`Content-Length` incoherentes | **REPRODUCED** | HIT: `content-encoding: gzip`, `content-length: 2048`, cuerpo de texto plano; `gzip.decompress` → `BadGzipFile` |
| EXT-H-02 | gRPC evita el plugin y `max_body_bytes` | **REPRODUCED** | plugin: gRPC → 418 (backend), HTTP/1.1 → 403; `max_body_bytes=1024`: gRPC 5005 B → 418, HTTP/1.1 5005 B → 413 |
| EXT-H-03 | PROXY se escribe después del TLS | **REPRODUCED** | el backend TLS descifra `PROXY TCP4 …\r\n` como primeros bytes de aplicación |
| EXT-H-04 | `proxy_pass https://` se importa como HTTP plano | **REPRODUCED** | TOML sin `[backend.*.tls]`; informe: `TLS backend | no`; 0 rechazadas y ningún aviso de TLS |
| EXT-H-05 | Se verifica la IP, no el hostname configurado | **REPRODUCED** | `server = "localhost:…"` sin `server_name` → 502 (`certificate not valid for name "::1"`); con `server_name = "localhost"` → 200 |
| EXT-H-06 | `maxconn` es global, no por listener | **REPRODUCED** | B solo → 200; A retiene una conexión → primera petición de B cerrada sin respuesta; al liberar A → 200 |

Resumen literal de la última ejecución:

```text
=== SUMMARY ===
h01: REPRODUCED
h02: REPRODUCED
h03: REPRODUCED
h04: REPRODUCED
h05: REPRODUCED
h06: REPRODUCED
```

---

## EXT-H-01 — caché + compresión

**Afirmación.** Un `HIT` puede servirse con un cuerpo que no corresponde a su
`Content-Encoding`/`Content-Length`.

**Montaje.** Origen real que devuelve 2048 bytes de texto con
`Cache-Control: public, max-age=60`; listener con caché y gzip:

```toml
[listener.web.cache]
default_ttl = "60s"

[listener.web.compression]
gzip = true
```

Dos `GET /obj` idénticos con `Accept-Encoding: gzip` y `Connection: close`.

**Observado.**

```text
first : 'HTTP/1.1 200 OK'
        x-cache='MISS' content-encoding='gzip' transfer-encoding='chunked'
        gzip decoded, matches origin: True
second: 'HTTP/1.1 200 OK'
        x-cache='HIT' content-encoding='gzip' content-length='2048'
        body starts b'The quick brown fox jumps over t' (2048 bytes)
        gzip decode of the HIT body: FAILED: BadGzipFile: Not a gzipped file (b'Th')
origin hits: 1 (must stay 1 for a HIT)
```

El segundo `GET` es un `HIT` real (el origen vio una sola petición) y llega con
la cabecera del `MISS` comprimido, pero con el cuerpo sin comprimir.

**Causa.** La cabeza que se guarda ya es la reescrita por compresión
(`http1.rs:5071` reescribe `response_head_bytes`; `http1.rs:5106` la pasa a
`ResponseAssembly`). `store_plan` (`http1.rs:2732`) guarda
`cache::end_to_end(&facts.headers)` (`http1.rs:2759`), y `MANAGED_HEADERS`
(`cache.rs:73`) solo elimina `content-length`, `transfer-encoding`, `x-cache`
y `age`: **no elimina `content-encoding`**. El cuerpo que se almacena es el del
backend, sin comprimir. Al servir, `render_head` (`cache.rs:1409`) añade
`content-length` del cuerpo almacenado y `serve_cached` (`http1.rs:2777`)
no recomprime porque `compression::eligible` descarta toda respuesta que ya
declare `content-encoding` (`compression.rs:180`).

## EXT-H-02 — gRPC evita el plugin y `max_body_bytes`

**Afirmación.** La ruta de pass-through gRPC no pasa por los mismos gates que
HTTP/2 normal.

**Montaje.** Dos listeners en la misma instancia:

- el primero, con `[listener.front.plugin] path = "…/deny_403.wasm"`;
- el segundo, sin plugin y con `[listener.front2.limits] max_body_bytes = 1024`.

Ambos envían a una segunda instancia EQProxy con TLS+ALPN `h2` y una regla
`path_prefix = "/"` que responde `418 reached-h2-backend`. Esa respuesta es la
marca de que la petición cruzó el gate del front. Peticiones con
`curl --http2-prior-knowledge -X POST`:

**Observado.**

```text
listener with plugin,   grpc 5 B   -> HTTP 418 body=b'reached-h2-backend'
listener with plugin,   h1   5 B   -> HTTP 403 body=b''
listener with max_body, grpc 5005 B-> HTTP 418 body=b'reached-h2-backend'
listener with max_body, h1   5005 B-> HTTP 413 body=b''
```

El plugin de denegación devuelve 403 a la misma petición cuando se traduce a
HTTP/1.1, pero el pass-through gRPC la entrega al backend (418). El límite de
1024 bytes rechaza el cuerpo HTTP/1.1 de 5005 bytes (413) y no el gRPC.

**Causa.** `http2.rs:257` llama a `grpc::serve` antes de construir el pipe hacia
el motor HTTP/1.1. `grpc::serve` (`grpc.rs:94`) aplica rate limit, auth, ruta,
acciones de cabecera, forwarded y rewrite (`grpc.rs:193`), pero no consulta
`cfg.plugin` ni `cfg.limits.max_body_bytes`; esos gates viven en la preparación
HTTP/1.1 (`http1.rs:3719` y `http1.rs:3851`).

## EXT-H-03 — PROXY después del TLS

**Afirmación.** El prefacio PROXY se escribe sobre el flujo TLS, después del
handshake, en lugar de antes del `ClientHello`.

**Montaje.** Backend `proxy_protocol = "v1"` + TLS con `verify = false` y
`server_name = "localhost"`, apuntando a un servidor TLS real. El servidor
completa el handshake y registra los primeros bytes **descifrados**.

**Observado.**

```text
[tls:h03] first decrypted bytes: b'PROXY TCP4 127.0.0.1 127.0.0.1 58393 58390\r\n'
TLS records captured: 1
  decrypted payload: b'PROXY TCP4 127.0.0.1 127.0.0.1 58393 58390\r\n'
```

El backend solo ve `PROXY …` después de terminar TLS, como datos de aplicación.

**Causa.** `proxy.rs:2891` hace primero
`client_tls.connector.connect(server_name, socket)`; después, sobre el
`TlsStream` resultante, `proxy.rs:2916` escribe `proxy_proto::write_v1(&mut
stream, …)`. La asimetría es real dentro del producto: el receptor de EQProxy
lee el PROXY **antes** de terminar TLS (`proxy.rs:3029`), de modo que un
EQProxy que envía a otro que recibe con `proxy_receive = true` no puede
funcionar.

## EXT-H-04 — `proxy_pass https://` se importa como HTTP plano

**Afirmación.** El conversor de nginx descarta el esquema `https` y genera un
backend sin TLS, sin avisar.

**Montaje.**

```nginx
http {
    server {
        listen 8080;
        server_name app.example;
        location / {
            proxy_pass https://backend.internal:8443;
        }
    }
}
```

Comando:

```bash
target/debug/eqproxy config import --from nginx \
  --input nginx.conf --output out.toml --report report.md
```

**Observado.** Exit 0; `3 converted, 2 with notes, 0 rejected`. El TOML
generado contiene:

```toml
[backend.backend_backend_internal_8443]
server = "backend.internal:8443"
```

No hay `[backend.backend_backend_internal_8443.tls]`. La fila del backend en
el informe es:

```text
| `backend_backend_internal_8443` | backend.internal:8443 (1) | roundrobin | — | none | no |
```

La columna «TLS backend» es `no`. Los dos avisos son informativos (`listen` y
`server_name`); **ninguno menciona el esquema `https`, TLS ni el puerto 8443**,
y no hay ninguna directiva rechazada.

**Causa.** `nginx.rs:1540` elimina el esquema con
`target.split_once("//").map(|(_, rest)| rest)` antes de resolver el destino;
`resolve_backend` (`nginx.rs:1708`) llama a `normalize_addr(…, Some(80), …)`
y crea un backend de un solo servidor sin TLS. El esquema nunca se conserva
ni se usa para rellenar `tls`.

## EXT-H-05 — verificación TLS contra la IP

**Afirmación.** Sin `server_name`, la verificación usa la IP resuelta, no el
hostname configurado en `server`.

**Montaje.** `server = "localhost:PORT"` (hostname, no IP), CA propia, hoja con
`subjectAltName = DNS:localhost` únicamente, y un servidor TLS real. Mismo
listener en dos ejecuciones:

```toml
[backend.api]
server = "localhost:PORT"

[backend.api.tls]
ca_file = "…/ca.pem"
# sin server_name en la primera; server_name = "localhost" en el control
```

**Observado.**

```text
server = "localhost:58395", no server_name -> HTTP/1.1 502 bad gateway
  proxy log TLS error: ['ERROR [web] failed TLS handshake to [::1]:58395 …:
    invalid peer certificate: certificate not valid for name "::1";
    certificate is only valid for DnsName("localhost")']
control with server_name = "localhost"     -> HTTP/1.1 200 OK
```

**Causa.** `tls.rs:637` (`server_name_for`) devuelve el `server_name`
configurado o, si falta, `ServerName::IpAddress(IpAddr::from(addr.ip()))`.
El hostname ya se perdió al resolver `ServerTarget::Name` a un `SocketAddr`.
El propio esquema documenta lo contrario: «Name to verify/SNI; defaults to the
host part of the address» (`eqproxy-config/src/schema.rs:263`).

## EXT-H-06 — `maxconn` global

**Afirmación.** El cupo de conexiones es un contador de proceso comparado con
el `maxconn` del listener que acepta, así que un listener consume el cupo de
otro.

**Montaje.** Dos listeners en el mismo proceso, ambos `maxconn = 1`, al mismo
origen real. Secuencia: petición a B sin nada retenido; abrir una conexión
TCP a A sin enviar nada; petición a B; cerrar A; petición a B.

**Observado.**

```text
listener B with nothing held          -> HTTP/1.1 200 OK
listener A holds one idle connection  -> <connection closed with no response>
listener B after A's connection closed-> HTTP/1.1 200 OK
origin hits: 2
```

La primera petición de B se cierra sin respuesta aunque B tiene su propio
`maxconn = 1` y ninguna conexión suya. El origen solo vio las dos peticiones
que sí pasaron.

**Causa.** `proxy.rs:2001` llama a
`metrics.try_acquire_connection(cfg.maxconn)`; el contador es el global
`connections_active` de `metrics.rs:768`, no uno por listener. El valor
comparado es el `maxconn` del listener que acepta, pero el consumo lo hacen
todos los listeners del proceso.

---

## Qué no se ha verificado

- **No se ha corregido nada.** Los seis siguen abiertos; este documento solo
  separa hipótesis estática de fallo reproducido.
- EXT-H-02 usa una segunda instancia EQProxy como backend marcador, no un
  servidor gRPC real: prueba que el gate se salta, no las semánticas de
  trailers/flow control de gRPC.
- EXT-H-04 prueba el conversor, no ejecuta nginx ni un origen HTTPS real: el
  resultado observable es el TOML y el informe generados.
- Todo es loopback, macOS y build `debug`; no se ha probado en otra
  arquitectura, red real ni con TLS de producción.
- No se ejecutaron Veyra Proof, Rust Audit ni CodeGraph: esta rebanada no toca
  código de producto, solo añade evidencia y un harness local. Se reportan
  como **NOT EXECUTED** para este incremento documental, no como PASS.

## Reproducir

```bash
cargo build -p eqproxy-cli --locked
python3 scripts/evidence/ext-h-2026-11-10/repro.py all
```

El harness imprime un veredicto por caso y escribe
`.repro-ext-h/run/verdicts.json`. La salida completa de la ejecución citada
está en [`raw/v095-ext-h-repro.log`](raw/v095-ext-h-repro.log).
