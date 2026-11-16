# Informe de conversión nginx → EQProxy

- Entrada: `/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/nginx.conf`
- Salida: `/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/eqproxy.toml`
- Listeners generados: **1** · Backends generados: **3**
- Directivas leídas: **62** · clasificadas: **62** — convertidas **25** · con degradación **0** · con aviso **33** · rechazadas **4**

«Nunca se descarta en silencio»: cada directiva del origen aparece abajo. «Con degradación» son directivas que sí están en el TOML pero con menos seguridad que la que declaraba el origen; junto con «con aviso» forman la columna «revisar». El fichero generado se ha parseado y validado con el propio parser de EQProxy (`schema_version = 1`) antes de escribirlo. La conversión es estática: no se ha ejecutado nada de la configuración de origen ni se ha medido tráfico.

## Conversión generada

| listener | escucha | modo | backend | TLS |
|---|---|---|---|---|
| `web_ejemplo_com` | 0.0.0.0:64006 | http | `app` | sí |

| backend | servidores (peso) | balance | health check | PROXY | TLS backend |
|---|---|---|---|---|---|
| `api_v2` | 127.0.0.1:64010 (5), 127.0.0.1:64011 (2) | roundrobin | — | none | no |
| `app` | 127.0.0.1:64007 (3), 127.0.0.1:64008 (1), 127.0.0.1:64009 (1) backup | leastconn | — | none | no |
| `backend_127_0_0_1_64012` | 127.0.0.1:64012 (1) | roundrobin | — | none | no |

## Detalle de la clasificación

### ❌ no traducible (1)

- `location` — *server ejemplo.com*: el bloque no tiene ninguna location con proxy_pass ni deny: EQProxy no sirve contenido estático

### ⛔ rechazado (sin equivalente en EQProxy) (3)

- `error_log /var/log/nginx/error.log` — *global*: directiva global «error_log» sin equivalente conocido: revísela a mano
- `root /var/www` — *server ejemplo.com location /static/*: EQProxy no sirve ficheros estáticos: hace falta un backend de origen
- `expires 30d` — *server ejemplo.com location /static/*: EQProxy no sirve ficheros estáticos: hace falta un backend de origen

### ⚠️ convertido con degradación (revisar) (0)


### ⚠️ revisar (17)

- `max_fails=3` — *upstream app*: chequeo pasivo de nginx: EQProxy tiene check TCP activo y detección pasiva (health_check.passive)
- `fail_timeout=30s` — *upstream app*: chequeo pasivo de nginx: EQProxy tiene check TCP activo y detección pasiva (health_check.passive)
- `keepalive 32` — *upstream app*: pool de EQProxy: 64 conexiones idle por servidor, expiración 60 s
- `include /etc/nginx/mime.types` — *http*: include no se expande: revise «/etc/nginx/mime.types» a mano y vuelva a convertir
- `log_format main $remote_addr - $remote_user [$time_local] "$request"  $status $body_bytes_sent "$http_referer"` — *http*: [access_log] con format = "json" (campos estables) o "combined" (forma nginx)
- `access_log /var/log/nginx/access.log main` — *http*: [access_log].path (una línea por petición; la ruta y el formato se eligen en el TOML)
- `gzip_types text/css application/json` — *http*: tipos de gzip: EQProxy comprime text/*, application/json y xml por defecto (compression.types)
- `return 301 https://$host$request_uri` — *server ejemplo.com*: redirección a nivel de server: [[listener.*.rules]].redirect, pero un listener necesita un backend real; si el bloque solo redirige, créelo a mano
- `ssl_ciphers HIGH:!aNULL` — *server ejemplo.com*: selección de cipher suites: fuera por decisión (ADR-0035)
- `keepalive_timeout 65s` — *server ejemplo.com*: listener.idle_timeout = «65s» (aproximado: en EQProxy la inactividad es bidireccional)
- `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for` — *server ejemplo.com location /*: «$proxy_add_x_forwarded_for» → [listener.*].forwarded_headers = "append" (EQProxy añade la IP de cliente a X-Forwarded-For)
- `proxy_connect_timeout 3s` — *server ejemplo.com location /*: el timeout es de ámbito listener: el valor de esta location no se aplica al listener generado; configúrelo en el bloque server o en [listener.*] a mano
- `proxy_read_timeout 30s` — *server ejemplo.com location /*: el timeout es de ámbito listener: el valor de esta location no se aplica al listener generado; configúrelo en el bloque server o en [listener.*] a mano
- `limit_req zone=api burst=20` — *server ejemplo.com location /api/*: [[listener.*.rules]].rate_limit (rate/per/burst) por IP: transcríbalo a mano
- `add_header` — *server ejemplo.com*: las cabeceras se emiten en una regla: las peticiones que no casen con ninguna regla no las llevan
- `auth_basic restringido` — *server panel.ejemplo.com*: [[listener.*.rules]].auth.users (htpasswd inline): el fichero no se lee, copie las líneas
- `auth_basic_user_file /etc/nginx/.htpasswd` — *server panel.ejemplo.com*: [[listener.*.rules]].auth.users (htpasswd inline): el fichero no se lee, copie las líneas

### ℹ️ informativo (16)

- `worker_processes auto` — *global*: directiva global sin traducción (no afecta a eqproxy.toml)
- `pid /run/nginx.pid` — *global*: directiva global sin traducción (no afecta a eqproxy.toml)
- `events` — *global*: bloque de infraestructura sin traducción (no afecta a eqproxy.toml)
- `default_type application/octet-stream` — *http*: cosmético de nginx (tipos MIME): EQProxy solo proxifica
- `sendfile on` — *http*: rendimiento interno de nginx sin mando expuesto
- `listen 64004` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `listen 64006 ssl` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `http2 on` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `ssl_certificate /var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/ejemplo.pem` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `ssl_certificate_key /var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/ejemplo.key` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `ssl_protocols TLSv1.2 TLSv1.3` — *server ejemplo.com*: binding y TLS del listener: aplicados arriba en [listener.*.tls]
- `http2` — *server ejemplo.com*: ALPN h2 en [listener.*.tls] (HTTP/2 nativo)
- `listen 64006` — *server panel.ejemplo.com*: comparte el listener del default_server «ejemplo.com»: EQProxy tiene un listener por IP:puerto
- `listen 64006 ssl` — *server panel.ejemplo.com*: TLS del vhost: certificado a [listener.*.tls].certs[] y política comparada con el default_server
- `ssl_certificate /var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/panel.pem` — *server panel.ejemplo.com*: TLS del vhost: certificado a [listener.*.tls].certs[] y política comparada con el default_server
- `ssl_certificate_key /var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/panel.key` — *server panel.ejemplo.com*: TLS del vhost: certificado a [listener.*.tls].certs[] y política comparada con el default_server

### ✅ convertido (25)

- `least_conn` — *upstream app*: balance = "leastconn" (least-connections ponderado)
- `server 127.0.0.1:64007 weight=3 max_fails=3 fail_timeout=30s` — *upstream app*: servidor 127.0.0.1:64007 (peso 3) en el upstream
- `server 127.0.0.1:64008` — *upstream app*: servidor 127.0.0.1:64008 (peso 1) en el upstream
- `backup` — *upstream app*: backup = true (B8: solo se usa cuando ningún primario es elegible)
- `server 127.0.0.1:64009 backup` — *upstream app*: servidor 127.0.0.1:64009 (peso 1) en el upstream
- `server 127.0.0.1:64010 weight=5` — *upstream api_v2*: servidor 127.0.0.1:64010 (peso 5) en el upstream
- `server 127.0.0.1:64011 weight=2` — *upstream api_v2*: servidor 127.0.0.1:64011 (peso 2) en el upstream
- `gzip on` — *http*: [listener.*.compression].gzip = true en cada listener generado
- `listen 64004` — *server ejemplo.com*: listener de EQProxy en 64004
- `server_name ejemplo.com www.ejemplo.com` — *server ejemplo.com*: host = «ejemplo.com», «www.ejemplo.com» en cada regla del vhost (2 nombre(s))
- `listen 64006` — *server ejemplo.com*: listener de EQProxy en 64006 con TLS y ALPN h2
- `ssl_certificate / ssl_certificate_key` — *server ejemplo.com*: [listener.*.tls] cert = «/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/ejemplo.pem», key = «/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/ejemplo.key»
- `ssl_protocols TLSv1.2 TLSv1.3` — *server ejemplo.com*: [listener.*.tls].min_version/max_version (EQProxy nunca acepta TLS 1.0/1.1)
- `server_name ejemplo.com` — *server ejemplo.com*: host = «ejemplo.com» en cada regla del vhost (1 nombre(s))
- `client_header_timeout 10s` — *server ejemplo.com*: listener.client_header_timeout = «10s»
- `proxy_pass http://app` — *server ejemplo.com location /*: regla con use_backend = «app»
- `proxy_set_header Host $host` — *server ejemplo.com location /*: set_headers += { Host, $host }
- `proxy_set_header X-Real-IP $remote_addr` — *server ejemplo.com location /*: set_headers += { X-Real-IP, $remote_addr }
- `proxy_pass http://api_v2` — *server ejemplo.com location /api/*: regla con use_backend = «api_v2»
- `proxy_set_header Host $host` — *server ejemplo.com location /api/*: set_headers += { Host, $host }
- `add_header X-Frame-Options SAMEORIGIN always` — *server ejemplo.com*: [[listener.*.rules]].response_headers (se emite en la regla que cubre el listener)
- `ssl_certificate / ssl_certificate_key` — *server panel.ejemplo.com*: [listener.*.tls].certs[] cert = «/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/panel.pem», key = «/var/folders/7z/2dgl35v966d7hs3w_jbg8klr0000gn/T//eqproxy-corpus-e2e.GUlhA0/certs/panel.key», names = [panel.ejemplo.com]
- `server_name panel.ejemplo.com` — *server panel.ejemplo.com*: host = «panel.ejemplo.com» en cada regla del vhost (1 nombre(s))
- `proxy_pass 127.0.0.1:64012` — *server panel.ejemplo.com location /*: origen literal convertido en backend «backend_127_0_0_1_64012» (127.0.0.1:64012)
- `proxy_pass http://127.0.0.1:64012` — *server panel.ejemplo.com location /*: regla con use_backend = «backend_127_0_0_1_64012»

## Qué no se ha traducido

- EQProxy no sirve ficheros estáticos: hace falta un backend de origen (2)
- directiva global «error_log» sin equivalente conocido: revísela a mano (1)
- el bloque no tiene ninguna location con proxy_pass ni deny: EQProxy no sirve contenido estático (1)

Cada uno de estos puntos es un cambio de comportamiento frente al proxy de origen. Hasta que se resuelvan (o se acepten por escrito), la configuración **no es equivalente**.

## Siguiente paso

1. `eqproxy config validate --config <salida>` — el mismo gate que usa el binario.
2. Leer las secciones «revisar» y «rechazado» de arriba y corregir a mano lo que falte.
3. `eqproxy config diff --help` para comparar con la configuración actual antes de `config apply`.
