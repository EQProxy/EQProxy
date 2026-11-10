# ADV-01 — Desincronización de respuestas con estados sin cuerpo

| Campo | Valor |
|---|---|
| ID | ADV-01 |
| Título | Respuesta *bodyless* con framing de cuerpo y cuerpo retrasado envenena el pool |
| Severidad | **Alta** (ver razonamiento) |
| Confianza | Alta (reproducido E2E con binario real) |
| Estado | **Confirmado**, abierto |
| Versión/commit | `release/v0.9.5`, `ecda55a46f91982842f8bfe87772bf44324df905` (árbol de trabajo) |
| Arquitectura | macOS arm64 (Darwin 25.5.0); lógica independiente de arquitectura |
| Componente | `eqproxy-net` — parser de respuesta y pool de conexiones |
| Archivos/líneas | `eqproxy-net/src/http1.rs:1047-1062` (framing bodyless), `:2052-2058` (`peer_gone`), `:4911-4929` (decisión de pool), `:3548-3586` (salida del bucle de respuesta) |

## Precondiciones y capacidad del atacante

- El atacante controla, o puede hacer fallar, un **upstream/backend** (F2 del
  `THREAT_MODEL.md`: “They may fail, return garbage or attack (malformed
  responses)”). No requiere acceso al cliente ni credenciales.
- El backend debe poder responder `204 No Content` (o `304`/`1xx`) con
  `Transfer-Encoding: chunked` o `Content-Length`, y enviar bytes de cuerpo
  **después** de la cabecera (p. ej. 1,2 s después, como en el PoC).
- La conexión debe volver al pool: EQProxy la reutiliza si no leyó bytes de más
  en el mismo *read* (`carry.is_empty()`) y `peer_gone()` no ve datos en ese
  instante.

No se atribuye explotación desde un cliente remoto sin un upstream hostile o
defectuoso. Con un backend meramente defectuoso el impacto es de
disponibilidad/integridad; con un backend hostil es inyección de respuesta
cruzada.

## Entrada mínima (upstream)

```http
HTTP/1.1 204 No Content
Transfer-Encoding: chunked

```
*(el terminador de chunk `0\r\n\r\n` o una respuesta completa se envía después,
fuera de la misma lectura que consume la cabecera)*

En el PoC se envía después una respuesta completa:

```http
HTTP/1.1 200 OK
Content-Length: 17

SMUGGLED-RESPONSE
```

## Pasos exactos de reproducción

```bash
cd /Volumes/Lexar/Cursor/EQProxy
cargo build --release -p eqproxy-cli
python3 report/adversarial-2026-11-09/lab/run_lab.py
```

El laboratorio arranca dos upstreams TCP reales y dos listeners de EQProxy en
puertos efímeros. El escenario S9:

1. Cliente 1: `GET /desync` al listener `desync` (backend dedicado, pool vacío).
2. El upstream responde `204` + `Transfer-Encoding: chunked` y **espera 1,2 s**.
3. EQProxy ve `BodyFraming::None`, comprueba `peer_gone()` (todavía sin datos),
   devuelve la conexión al pool y responde `204` al cliente 1.
4. A los 1,2 s el upstream envía la respuesta forjada completa.
5. Cliente 2 (1,6 s después): `GET /normal` al mismo listener.
6. EQProxy reutiliza la conexión envenenada y entrega la respuesta forjada.

## Comportamiento esperado vs observado

| | Esperado | Observado |
|---|---|---|
| Cliente 1 | `204` sin cuerpo | `204` con `Transfer-Encoding: chunked` reenviado |
| Cliente 2 | `200 NORMAL2` | `200` con cuerpo `SMUGGLED-RESPONSE` |
| Conexión | no reutilizada tras framing ilegal | reutilizada |

Evidencia (`report/adversarial-2026-11-09/lab/results.json`, entrada S9):

```json
"first_raw": "HTTP/1.1 204 No Content\r\nServer: EQProxy\r\nTransfer-Encoding: chunked\r\n\r\n",
"second": {
  "status": 200,
  "headers": { "server": "EQProxy", "content-length": "17" },
  "body": "SMUGGLED-RESPONSE"
}
```

Log de la ejecución: `report/adversarial-2026-11-09/lab-run.log`.

## Impacto y límites

- **Integridad:** una respuesta generada para una petición se entrega como
  respuesta de otra petición distinta del mismo backend.
- **Disponibilidad:** el cliente afectado recibe `502` o datos ajenos.
- **Cruce de hosts/listeners:** si un mismo backend se comparte entre
  listeners/vhosts, la respuesta forjada puede cruzar esa frontera.
- **Límite:** requiere framing ilegal en el upstream. Un upstream que respeta
  RFC 9112 §6.3 no lo dispara. No se ha demostrado con un cliente remoto sin
  control del backend.
- La variante con `Content-Length` en `1xx/204`, o con cuerpo en `304`/`HEAD`,
  pertenece a la misma clase y no se ha explotado por separado; el PoC usa
  `TE` + `204`.

## Causa raíz

En `parse_response_head`:

```rust
// eqproxy-net/src/http1.rs:1047
if te_chunked && content_length.is_some() {
    return Err(bad_gateway);
}
let bodyless = status < 200 || status == 204 || status == 304 || head_request;
let framing = if bodyless {
    BodyFraming::None          // <- ignora TE/CL declarados
} else if te_chunked { ... }
```

No se rechaza `Transfer-Encoding` en un estado que por definición no lleva
cuerpo (RFC 9112 §6.3), ni `Content-Length` en `1xx/204`. El exchange no lee
cuerpo (`framing == None`), y el pool decide reutilizar con
`carry.is_empty() && !peer_gone()` (`http1.rs:4911-4929`). `peer_gone` solo
mira si hay bytes **ya disponibles** en ese instante (`:2052-2058`); un cuerpo
retrasado llega después y queda en la conexión reutilizada.

## Propuesta de corrección

1. En `parse_response_head`, para `bodyless`:
   - `te_chunked` → `502` (nunca es válido en `1xx/204/304/HEAD`).
   - `content_length.is_some() && (status < 200 || status == 204)` → `502`.
2. Para `304`/`HEAD` con `Content-Length` (metadato legítimo), **no reutilizar**
   la conexión: `wants_close = true`, salvo que se pueda probar que la longitud
   coincide con la representación almacenada (`304` con entrada stale) y el
   backend no envió bytes. La opción conservadora es no pool.
3. Alternativa equivalente: tras una respuesta `bodyless`, hacer una lectura
   con timeout corto y, si hay cualquier byte, cerrar sin pool. No sustituye
   al rechazo de framing ilegal, que es el contrato explícito del producto.
4. Añadir contador/métrica de respuestas upstream rechazadas por framing
   bodyless, para detectar backends defectuosos.

No se ha aplicado ninguna corrección (regla de la auditoría).

## Prueba de regresión

- Añadir un test E2E (o reutilizar `report/adversarial-2026-11-09/lab/run_lab.py`,
  S9) que:
  1. arranque un origen que responda `204` + `TE: chunked` y un cuerpo forjado
     retrasado;
  2. haga una segunda petición por el mismo listener;
  3. exija `200 NORMAL2` y **no** `SMUGGLED-RESPONSE`, y que el `204` no lleve
     `Transfer-Encoding`.
- El escenario S9 ya es esa regresión y hoy **falla** (ver `lab-run.log`).

## Evidencia

- `report/adversarial-2026-11-09/lab/run_lab.py` (escenario S9, upstream
  `desync`).
- `report/adversarial-2026-11-09/lab/results.json` (S9).
- `report/adversarial-2026-11-09/lab-run.log`.
- `report/adversarial-2026-11-09/lab/eqproxy.toml` (config generada).
- Código: `eqproxy-net/src/http1.rs:1047-1062`, `:2052-2058`, `:4911-4929`.
