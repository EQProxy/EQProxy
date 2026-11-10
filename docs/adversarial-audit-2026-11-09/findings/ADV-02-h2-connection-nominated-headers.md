# ADV-02 — Las cabeceras nominadas por `Connection` cruzan la traducción h2

| Campo | Valor |
|---|---|
| ID | ADV-02 |
| Título | h2↔h1 no elimina las cabeceras nominadas por `Connection` |
| Severidad | **Media** (ver razonamiento) |
| Confianza | Alta (reproducido E2E en las dos direcciones) |
| Estado | **Confirmado**, abierto |
| Versión/commit | `release/v0.9.5`, `ecda55a46f91982842f8bfe87772bf44324df905` (árbol de trabajo) |
| Arquitectura | macOS arm64; lógica independiente de arquitectura |
| Componente | `eqproxy-net` — traducción HTTP/2 ↔ HTTP/1.1 |
| Archivos/líneas | `eqproxy-net/src/http2.rs:68-78` (`HOP_BY_HOP`), `:136-138` (`is_hop_by_hop`), `:357-418` (`build_request_head`), `:739-763` (`response_headers`) |

## Precondiciones y capacidad del atacante

- **Dirección respuesta (h1→h2):** el upstream controla la respuesta y envía
  `Connection: <nombre>` + `<nombre>: <valor>`. El cliente h2 (remoto, sin
  autenticar) recibe `<nombre>`. Capacidad: upstream hostil/defectuoso (F2).
- **Dirección petición (h2→h1):** un cliente h2 sin autenticar envía
  `Connection: <nombre>` + `<nombre>: <valor>`; EQProxy elimina `connection`
  pero reenvía `<nombre>` al upstream. Capacidad: cliente remoto.
- El cliente h2 puede enviar `Connection` con `curl --http2-prior-knowledge`
  (confirmado); el crate `h2` no lo rechaza en este camino.

## Entrada mínima

Respuesta del upstream:

```http
HTTP/1.1 200 OK
Content-Length: 2
Connection: X-Secret
X-Secret: leaked

ok
```

Petición h2 del cliente (equivalente curl):

```bash
curl --http2-prior-knowledge -H 'Connection: X-Evil' -H 'X-Evil: smuggled-h2' \
  http://127.0.0.1:<puerto>/echo
```

## Pasos exactos de reproducción

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo build --release -p eqproxy-cli
python3 report/adversarial-2026-11-09/lab/run_lab.py
```

Escenarios S10 (respuesta) y S12 (petición). El upstream `/hop` devuelve
`Connection: X-Secret` + `X-Secret: leaked`; `/echo` devuelve las cabeceras
que recibió.

## Comportamiento esperado vs observado

| Escenario | Esperado (RFC 9113 §8.2.2) | Observado |
|---|---|---|
| S10 h2 response | `x-secret` ausente | `x-secret: leaked` presente |
| S12 h2 request | `x-evil` ausente en el origen | `x-evil=smuggled-h2` en el origen |

Evidencia (`lab/results.json`):

```json
"S10": { "curl_headers": "http/2 200 \nserver: eqproxy\ncontent-length: 2\nx-secret: leaked\nx-cache: miss\n\n" }
"S12": { "origin_echo": "accept=*/*|host=127.0.0.1:64508|user-agent=curl/8.7.1|x-evil=smuggled-h2" }
```

Log: `report/adversarial-2026-11-09/lab-run.log`.

## Impacto y límites

- **Fuga de información:** cabeceras que el origen marcó hop-by-hop (p. ej.
  debug interno, tokens de un salto, `X-Forwarded-*`, `X-Internal-*`) llegan a
  clientes h2 que no deberían verlas.
- **Bypass de semántica entre saltos:** un cliente h2 puede hacer que una
  cabecera nominada llegue al upstream aunque un salto intermedio (o el propio
  cliente) la haya marcado como hop-by-hop; puede romper supuestos de
  stripping en el backend.
- **Límite:** no se ha demostrado bypass de autenticación ni de reglas de
  routing con este primitivo. Es una violación de protocolo con impacto de
  fuga/robustez, no un RCE.
- La variante h3 no se ha probado; el mismo patrón podría existir en
  `http3.rs` si reutiliza la misma lista.

## Causa raíz

`HOP_BY_HOP` es una lista fija de 9 nombres:

```rust
// eqproxy-net/src/http2.rs:70
const HOP_BY_HOP: [&str; 9] = [
    "connection", "keep-alive", "proxy-connection", "proxy-authenticate",
    "proxy-authorization", "transfer-encoding", "upgrade", "te", "trailer",
];
pub(crate) fn is_hop_by_hop(name: &str) -> bool { HOP_BY_HOP.contains(&name) }
```

RFC 9110 §7.6.1 y RFC 9113 §8.2.2 exigen además eliminar **cada cabecera
nominada por `Connection`**. `response_headers` (`:739-763`) y
`build_request_head` (`:357-418`) solo consultan `is_hop_by_hop`, así que
`x-secret`/`x-evil` sobreviven; `connection` sí se elimina por estar en la
lista fija.

## Propuesta de corrección

1. En `response_headers`: antes de filtrar, recorrer las líneas y recoger los
   tokens de todas las cabeceras `connection` (split por `,`, `trim`,
   `to_lowercase`); añadirlos al conjunto de nombres a descartar.
2. En `build_request_head`: hacer lo mismo con el `HeaderMap` h2: leer
   `headers.get_all("connection")`, construir el conjunto nominado y omitir
   esas cabeceras al escribir el head h1.
3. Considerar **rechazar** (`400`/`RST_STREAM`) una petición h2 que traiga
   `connection`, ya que RFC 9113 lo prohíbe; el stripping es el mínimo.
4. Añadir unit tests:
   - `response_headers(b"HTTP/1.1 200 OK\r\nConnection: X-Secret\r\nX-Secret: leaked\r\nContent-Length: 2\r\n\r\n")`
     no contiene `x-secret`.
   - `build_request_head` con `connection: x-evil` + `x-evil` no escribe
     `x-evil` en el head h1.
5. Revisar `http3.rs` por el mismo patrón.

No se ha aplicado ninguna corrección (regla de la auditoría).

## Prueba de regresión

Los escenarios S10 y S12 del laboratorio ya son la regresión y hoy **fallan**.
Para CI, convertir el caso de respuesta en un test unitario de
`response_headers` y el de petición en uno de `build_request_head` (no
necesitan red).

## Evidencia

- `report/adversarial-2026-11-09/lab/run_lab.py` (S10, S12).
- `report/adversarial-2026-11-09/lab/results.json`.
- `report/adversarial-2026-11-09/lab-run.log`.
- Código: `eqproxy-net/src/http2.rs:68-78,136-138,357-418,739-763`.
