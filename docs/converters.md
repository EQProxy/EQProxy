# Cómo leer el informe de `config import`

`eqproxy config import` escribe el `eqproxy.toml` y un informe Markdown con una
línea por directiva leída del origen: nada se descarta en silencio. La cabecera
reparte esas líneas en cuatro columnas.

| columna | severidad | significa |
|---|---|---|
| convertidas | `Done` | la directiva tiene un campo en el esquema y se escribe tal cual. |
| con degradación | `Degraded` | la configuración resultante conserva la directiva o su efecto, pero con menos seguridad que la que declaraba el origen. El motivo es obligatorio y nombra la propiedad perdida. |
| con aviso | `Info` / `Review` | no bloquea el arranque: la directiva no afecta al TOML (`Info`) o su traducción es aproximada y hay que leerla antes de cambiar (`Review`). |
| rechazadas | `Gap` / `Rejected` | EQProxy no tiene equivalente (`Gap`) o el valor no se puede traducir tal cual (`Rejected`). |

«Con degradación» y «con aviso» forman juntas la columna **revisar**: las cuatro
columnas de clasificación de la cabecera suman las directivas clasificadas.

## Qué se considera una degradación

Una directiva es `Degraded` cuando la configuración resultante conserva su efecto
pero pierde una propiedad de seguridad que la fuente sí declaraba. Hoy el
contrato reconoce estas:

| motivo | caso típico |
|---|---|
| `certificado de cliente (mTLS)` | `proxy_ssl_certificate` / `proxy_ssl_certificate_key`: el origen se autentica ante el backend con mTLS y `[backend.*.tls]` no tiene campo para el certificado. |
| `política de versiones TLS` | `proxy_ssl_protocols`: el tramo al backend negocia con la política de EQProxy (`min_version`). |
| `política de cifrados` | `proxy_ssl_ciphers`: EQProxy usa sus cifrados, no los del origen. |
| `profundidad de verificación` | `proxy_ssl_verify_depth`: se acepta una cadena de certificados más larga que la que el origen admitía. |
| `identidad TLS compartida` | dos `location` comparten backend y declaran combinaciones `proxy_ssl_*` distintas: el modelo guarda una sola, así que una de las rutas verifica un nombre que su origen no declaró. |

Una directiva cuya traducción no relaja nada no se degrada: `proxy_ssl_verify
off` es el valor por defecto de nginx y el esquema lo expresa, así que sale como
convertida. Tampoco se degrada una directiva que nginx ignora en ese contexto
(por ejemplo `proxy_ssl_*` sobre un `proxy_pass http://`): se informa de que no
aplica. El informe lleva, además, una sección **Convertido con degradación** con
la lista agrupada por motivo y el aviso de que la configuración no es
equivalente en seguridad hasta corregirla o aceptarla por escrito.

`proxy_ssl_*` sigue la herencia de nginx **por directiva**, no por bloque: un
valor declarado en `http` llega a cada `server` y `location` que no declare la
misma directiva, y uno declarado en `server` llega a cada `location` que no la
sobrescriba. Así, `proxy_ssl_verify on` en `server` genera
`[backend.*.tls] verify = true`; `proxy_ssl_verify off` en una `location`
concreta vuelve a `false` solo para esa ruta. Las propiedades que el esquema no
puede expresar (`proxy_ssl_certificate`, `proxy_ssl_protocols`,
`proxy_ssl_ciphers`, `proxy_ssl_verify_depth`) se marcan `Degraded` también
cuando vienen heredadas de `http` o `server`, porque el backend sigue sin
expresarlas. Cualquier `proxy_ssl_*` que el conversor no reconozca se clasifica
`Review` con su nombre y la familia, sin asumir que rellene ningún campo de
`[backend.*.tls]`; no cae en el aviso genérico de location/upstream.

La terminación TLS del listener sigue la misma herencia por directiva, ahora de
`http` a `server`: `ssl_protocols`, `ssl_certificate`, `ssl_certificate_key`,
`ssl_client_certificate` y `ssl_verify_client` se resuelven campo a campo. Un
`ssl_protocols TLSv1.3` en `http` genera `min_version = "1.3"` y
`max_version = "1.3"` en `[listener.*.tls]`; un certificado o CA declarado solo
en `http` se emite en el listener si el servidor no lo sobrescribe. Solo
`ssl_verify_client on` se convierte: `optional` y `optional_no_ca` se rechazan
porque EQProxy exige el certificado de cliente al configurar `client_ca`, y
nginx lo pedía como opcional. En un
listener compartido, un certificado distinto del heredado se añade a
`[[listener.*.tls.certs]]`; una política de versiones o una política mTLS
(CA de clientes o exigencia de certificado) distinta de la del `default_server`
no se puede aplicar por vhost y se marca `Degraded`. `ssl_ciphers` queda fuera de esta herencia porque EQProxy no acepta
una selección de cifrados por configuración (ADR-0035).

## `server` que solo redirige

Un bloque `server` cuyo único destino es un `return` de redirección (por
ejemplo el clásico 80 → 443) se convierte en un listener real con
`[listener.*.redirect]` y **sin** `backend`:

```nginx
server {
    listen 80;
    server_name ejemplo.com www.ejemplo.com;
    return 301 https://$host$request_uri;
}
```

```toml
[listener.web_ejemplo_com]
listen = "0.0.0.0:80"
mode = "http"

[listener.web_ejemplo_com.redirect]
status = 301
location = "https://$host$request_uri"
```

El status debe ser 301, 302, 303, 307 o 308 y el destino solo puede usar las
variables que EQProxy renderiza (`$host`, `$uri`, `$request_uri`). Si el
bloque también tiene `location`, `deny` u otro destino, nginx aplica el
`return` antes que las locations y el conversor lo deja como aviso manual en
lugar de prometer una equivalencia que no tiene.

## `server_name` con comodines y expresiones

El conversor traduce las tres formas de nombre de nginx a condiciones de host
reales, conservando la precedencia de nginx: exacto, wildcard inicial más
largo, wildcard final más largo, expresiones en orden de aparición y, por
último, el fallback del listener.

| nginx | condición EQProxy | `certs[].names` |
|---|---|---|
| `api.example.org` | `host = "api.example.org"` | `api.example.org` |
| `*.example.org` | `host_regex = "^[^.]+(?:\\.[^.]+)*\\.example\\.org$"` | `*.example.org` |
| `.example.org` | `host = "example.org"` + el `host_regex` anterior | `example.org`, `*.example.org` |
| `mail.*` | `host_prefix = "mail."` | no se emite |
| `~^api-[0-9]+\.example\.org$` | `host_regex` con la expresión | no se emite |

`*.example.org` cubre uno o varios niveles (`a.example.org` y
`a.b.example.org`), pero no el apex; `.example.org` añade el apex exacto. `~*`
emite siempre `(?i)`; `~` lo añade cuando la expresión contiene mayúsculas,
porque nginx la compila case-insensitive. El resto se valida con el motor
`regex` de EQProxy antes de escribir el TOML. Si usa construcciones PCRE que
ese motor no admite (por ejemplo look-around), la expresión se rechaza y se
explica en el informe: nunca se elimina la condición para convertir el vhost
en catch-all.

## `include`, globs y ciclos

Sin `--include-root`, `include` sigue siendo una fila de revisión manual: el
importador no adivina el prefijo de nginx ni lee ficheros por su cuenta. Con
`--include-root <DIR>` expande cada `include` **en su posición del árbol**,
antes de agrupar vhosts, de modo que los `server`, `upstream`, `location` y
demás bloques de los ficheros incluidos se clasifican igual que si estuvieran
escritos en el fichero principal:

```bash
eqproxy config import --from nginx --input nginx.conf \
  --include-root /etc/nginx --output eqproxy.toml --report report.md
```

- Las rutas relativas se resuelven contra `--include-root`; una ruta relativa
  con `..` que salga de esa raíz se rechaza. Las rutas absolutas se leen tal
  como están escritas, como haría nginx. Las rutas relativas que resuelven por
  symlink fuera de la raíz también se rechazan.
- Los globs de nginx (`*`, `?`, `[...]`) se expanden con orden lexicográfico de
  ruta, no con el orden del sistema de ficheros. Un glob sin coincidencias es
  válido en nginx y aparece como `Review` en el informe; una ruta literal que
  no existe es un error.
- Los `include` anidados se expanden en profundidad y se clasifican uno a uno.
  Cada fichero se identifica por su ruta canónica: un ciclo `a → b → a` falla
  con la cadena completa y no se convierte en un bucle de lectura.
- Los topes de una importación son **16 niveles** de include, **256 ficheros**
  incluidos y **4 MiB** de contenido expandido (el fichero principal cuenta en
  los bytes). Superarlos es un error explícito; nunca se trunca el origen.
- El fichero incluido debe ser un fichero regular UTF-8. Variables de nginx en
  la ruta del `include`, rutas vacías y `include` con bloque se rechazan con
  el contexto donde aparecen.

La fila del `include` queda como `Info` cuando se expande (con el número de
ficheros leídos) o como `Review` si el glob no coincide con nada. Las
directivas de los ficheros incluidos conservan su clasificación normal, así
que un vhost incluido no desaparece del TOML ni del informe.

`certs[].names` expresa nombres exactos y wildcards `*.dominio` de **una**
etiqueta. Por eso un wildcard de nginx de varios niveles se enruta con toda su
semántica, pero para un nombre más profundo el handshake SNI recibe el
certificado por defecto del listener. Los nombres con `mail.*` o regex no se
pueden anunciar en `certs[].names`; su certificado no se emite y el informe lo
declara. Un nombre `$hostname` o un comodín mal formado se deja como aviso
manual en lugar de inventar un valor.

## El resumen de una línea

Al terminar, `config import` imprime una línea con el recuento y **los nombres
de las directivas degradadas** (hasta tres, con `+N more`), porque el informe
puede ir a un fichero y quien lee el log de CI no debe tener que abrirlo para
enterarse de que se perdió una propiedad de seguridad:

```
import: nginx.conf (nginx) → eqproxy.toml: 4 converted, 1 degraded (proxy_ssl_certificate /etc/client.pem), 2 with notes, 0 rejected
  1 directive(s) were converted with less security than the source declared (see the report)
```

## Códigos de salida

- `0`: conversión limpia: ninguna directiva rechazada y **ninguna degradada**.
- `2`: el fichero de origen es válido y el TOML se escribe, pero hay directivas
  rechazadas o degradadas; el resumen y el informe las nombran y la
  configuración **no es equivalente** (en el caso degradado, no lo es en
  seguridad) sin resolverlas o aceptarlas por escrito. Es el mismo código para
  los dos casos porque la acción del operador es la misma: leer el informe antes
  de cambiar; el resumen distingue cuál de los dos motivó la salida.
- `1`: no se escribió nada (entrada ilegible, sintaxis inválida o la salida no
  valida con el propio parser de EQProxy).
