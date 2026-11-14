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
