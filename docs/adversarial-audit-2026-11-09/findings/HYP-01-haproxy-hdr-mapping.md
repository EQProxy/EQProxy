# HYP-01 — El importador HAProxy mapea `hdr`/`hdr_dom` a `rule.host`

| Campo | Valor |
|---|---|
| ID | HYP-01 |
| Título | `condition_rule` interpreta un token `hdr`/`hdr_dom` como match de Host |
| Severidad | **Informativa / baja** (no se ha demostrado impacto) |
| Confianza | Media (código claro; alcanzabilidad no confirmada) |
| Estado | **Hipótesis**, no confirmado |
| Versión/commit | `release/v0.9.5`, `ecda55a46f91982842f8bfe87772bf44324df905` |
| Componente | `eqproxy-config` — importador HAProxy |
| Archivo/línea | `eqproxy-config/src/import/haproxy.rs:1815` |

## Observación

```rust
// eqproxy-config/src/import/haproxy.rs:1811
match name.as_str() {
    "path_beg" => rule.path_prefix = Some(value),
    "path" => rule.path = Some(value),
    "path_end" | "path_reg" | "path_sub" => return None,
    "hdr" | "hdr_dom" => rule.host = Some(value),   // <- HYP-01
    ...
}
```

`split_args` mantiene `hdr(host)` como un único token, por lo que `name` sería
`"hdr(host)"` y caería en `_ => return None` (revisión explícita). Para que la
rama se ejecute haría falta un token literal `hdr`/`hdr_dom`, que no es
sintaxis HAProxy válida en un ACL inline (`hdr(<nombre>)` es el fetcher).

## Por qué importa si se alcanzara

- `rule.host = Some(value)` compara el valor contra el **Host completo** de la
  petición, no contra el header nombrado. Un ACL `hdr(X-Tenant) vip` se
  convertiría en “Host == vip”, que enruta de forma distinta a la intención.
- Es una divergencia semántica silenciosa, justo lo que el importador dice
  evitar (“only conditions the schema can evaluate are accepted”).

## Estado de la comprobación

- No se ha construido un caso E2E con HAProxy real porque el token no es
  válido; el fuzzing de `config_import` (1.080.684 ejecuciones) no encontró
  una entrada que lo alcance de forma natural.
- No hay evidencia de explotación. Se registra para que el propietario decida
  entre eliminar la rama (`return None`) o implementar `hdr(<nombre>)` con un
  match de header real y rechazar los nombres no soportados.

## Propuesta

1. Si solo se soporta `host`, cambiar la rama a
   `"hdr(host)" | "hdr_dom(host)" => rule.host = Some(value)` y devolver
   `None` para cualquier otro `hdr(...)`.
2. O mapear `hdr(<nombre>)` a un match de header del `Rule` (si el esquema lo
   permite) y diagnosticar el resto.
3. Añadir tests de importación con `use_backend ... if { hdr(host) -i x }`,
   `{ hdr(X-Tenant) vip }` y `{ hdr_dom(host) x }`, comprobando el TOML
   generado y el informe.

## Evidencia

- Código: `eqproxy-config/src/import/haproxy.rs:95-129` (`split_args`),
  `:1785-1834` (`condition_rule`), `:1949-1989` (test existente).
- Fuzz: `report/adversarial-2026-11-09/fuzz-config-import.log` (sin crash;
  no valida equivalencia semántica).
