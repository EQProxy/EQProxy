# Remediación priorizada — auditoría adversarial 2026-11-09

Plan de corrección para los hallazgos confirmados y la hipótesis. **No se ha
aplicado ningún cambio de producto durante la auditoría**: este documento
propone el cambio, el criterio de aceptación y la prueba de regresión. El
código de producto sigue intacto.

---

## 1. Prioridad

| Orden | ID | Severidad | Esfuerzo | Riesgo de no corregir | Ventana sugerida |
|---|---|---|---|---|---|
| 1 | ADV-01 | Alta | Bajo/medio | Inyección/corrupción de respuestas entre clientes del mismo backend | Antes de cualquier release |
| 2 | ADV-02 | Media | Bajo | Fuga de cabeceras hop-by-hop en h2 y ruptura de semántica entre saltos | Siguiente parche |
| 3 | HYP-01 | Informativa | Bajo | Rama muerta/mapeo incorrecto si se alcanza con config malformada | Limpieza |
| 4 | CI | — | Bajo | Los hallazgos pueden reaparecer sin señal | Mismo PR que 1–2 |
| 5 | Docs | — | Bajo | Deriva entre `THREAT_MODEL.md` y el estado real | Mismo PR que 4 |

Criterio de cierre de la auditoría: **S9, S10 y S12 en PASS**, suite del
workspace verde, fuzz sin crashes y revisión de la matriz de cobertura sin
regresiones.

---

## 2. ADV-01 — Respuesta bodyless con framing y cuerpo retrasado

### 2.1 Cambio recomendado (mínimo y conservador)

En `eqproxy-net/src/http1.rs`, dentro de `parse_response_head`, **antes** de
calcular `BodyFraming::None`:

```rust
// RFC 9112 §6.3: Transfer-Encoding is never valid on a response that
// cannot carry content. A backend that sends it is malformed or hostile;
// treating it as bodyless leaves unread bytes on a pooled connection.
if bodyless && te_chunked {
    return Err(bad_gateway); // 502, never pool
}

// Content-Length on 1xx/204 is equally invalid. On 304/HEAD it is legal
// metadata about the selected representation, so it is handled below.
if bodyless && content_length.is_some() && (status < 200 || status == 204) {
    return Err(bad_gateway);
}
```

Y para `304`/`HEAD` con `Content-Length` (metadato legítimo pero riesgo de
cuerpo retrasado), la opción conservadora:

```rust
let wants_close = connection_close
    || (version == 10 && !connection_keep_alive)
    || framing == BodyFraming::UntilEof
    // A bodyless response that still declares a length cannot be proven
    // empty without waiting; do not reuse the connection.
    || (bodyless && content_length.is_some());
```

Esto convierte el `204`+`TE` del PoC en `502` y evita reutilizar la conexión en
`304`/`HEAD` con `CL`, que es donde un cuerpo retrasado también podría quedar.

### 2.2 Alternativas y por qué no son la primera opción

| Alternativa | Problema |
|---|---|
| Leer y descartar `Content-Length` bytes en `304`/`HEAD` | Un backend hostil puede no enviarlos y bloquear el worker hasta el timeout; convierte un fallo de integridad en uno de disponibilidad. |
| Poll corto tras la respuesta bodyless | No cubre un cuerpo que llegue después del poll (el PoC espera 1,2 s). No es una garantía, solo reduce la ventana. |
| Cerrar siempre las respuestas bodyless | Correcto pero pierde keep-alive en `204`/`304` legítimos; el cierre selectivo por framing declarado es mejor relación seguridad/rendimiento. |

Se puede combinar la opción conservadora con un contador
`upstream_bodyless_framing_rejected` para observar backends defectuosos.

### 2.3 Prueba de regresión

- **E2E (obligatoria):** el escenario S9 de
  `report/adversarial-2026-11-09/lab/run_lab.py`. Criterio:
  - el primer cliente recibe `204` **sin** `Transfer-Encoding`;
  - el segundo recibe `200` con cuerpo `NORMAL2`;
  - nunca aparece `SMUGGLED-RESPONSE`.
- **Unidad (recomendada):** añadir a `eqproxy-net/src/http1.rs` (módulo
  `tests`) casos de `parse_response_head`:
  - `HTTP/1.1 204 ... Transfer-Encoding: chunked` → `Err`;
  - `HTTP/1.1 204 ... Content-Length: 5` → `Err`;
  - `HTTP/1.1 304 ... Content-Length: 123` → `Ok`, `wants_close == true`;
  - `HEAD` con `Content-Length` → `Ok`, `wants_close == true`;
  - `200` con `CL` sigue siendo poolable.
- **No regresión:** el resto de escenarios del laboratorio (S1–S8, S11)
  deben seguir en PASS.

### 2.4 Criterio de aceptación

1. `python3 report/adversarial-2026-11-09/lab/run_lab.py` → exit 0 (S9 PASS).
2. `cargo test --workspace --no-fail-fast` → exit 0.
3. `cargo +nightly fuzz run framing ...` → 0 crashes (presupuesto de CI).
4. El `204` legítimo sin `TE`/`CL` sigue devolviendo `204` y reutilizando
   conexión.

### 2.5 Riesgo de la corrección

- Backends que hoy envían `204` con `CL: 0` pasarán a `502`; es intencional
  según RFC 9112, pero conviene anunciarlo en el changelog.
- `304`/`HEAD` con `CL` dejan de reutilizar conexión: coste de handshake en
  rutas de caché condicional. Es el precio de no poder probar ausencia de
  cuerpo.

---

## 3. ADV-02 — Cabeceras nominadas por `Connection` en h2

### 3.1 Cambio recomendado

En `eqproxy-net/src/http2.rs`:

1. Añadir un helper que, a partir de las líneas de un head h1, devuelva el
   conjunto de nombres nominados por `connection`:

```rust
fn connection_nominated<'a>(lines: impl Iterator<Item = &'a [u8]>) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in lines {
        let Some(colon) = line.iter().position(|b| *b == b':') else { continue };
        if !line[..colon].eq_ignore_ascii_case(b"connection") {
            continue;
        }
        for token in line[colon + 1..].split(|b| *b == b',') {
            let token = std::str::from_utf8(token).unwrap_or("").trim();
            if !token.is_empty() {
                set.insert(token.to_ascii_lowercase());
            }
        }
    }
    set
}
```

2. En `response_headers`, calcular el conjunto **antes** del bucle y filtrar
   `name` si `is_hop_by_hop(name) || nominated.contains(name)`.

3. En `build_request_head`, calcular el conjunto desde el `HeaderMap` h2
   (`headers.get_all("connection")`, varios valores posibles) y omitir
   `name` si está nominado, además de la lista fija.

4. Considerar **rechazar** la petición h2 que traiga `connection`
   (`RST_STREAM(PROTOCOL_ERROR)` o `400`), porque RFC 9113 §8.2.2 lo prohíbe.
   El stripping es el mínimo aceptable; el rechazo es más estricto y puede
   romper clientes que hoy lo envían (curl lo hace). Recomendación: stripping
   primero, rechazo detrás de una opción de configuración.

5. Revisar `http3.rs`: si reutiliza `HOP_BY_HOP`/`is_hop_by_hop`, aplicar el
   mismo tratamiento.

### 3.2 Pruebas

- **Unidad (sin red):**
  - `response_headers(b"HTTP/1.1 200 OK\r\nConnection: X-Secret\r\nX-Secret: leaked\r\nContent-Length: 2\r\n\r\n")`
    no contiene `x-secret`;
  - `build_request_head` con `connection: x-evil` + `x-evil` no escribe
    `x-evil` en el head h1;
  - `connection: a, b` elimina ambos;
  - una cabecera llamada `connection` en mayúsculas también se procesa.
- **E2E:** S10 y S12 del laboratorio.
- **Negativos:** `keep-alive`, `te`, `upgrade` siguen eliminándose como antes;
  una cabecera normal (`x-request-id`) sigue pasando.

### 3.3 Criterio de aceptación

1. S10 y S12 en PASS.
2. S11 (h2c básico) sigue en PASS.
3. Tests unitarios nuevos en verde.
4. `cargo test --workspace --no-fail-fast` en verde.

### 3.4 Riesgo

- Si un backend usa `Connection: X-Foo` para pedir un comportamiento especial,
  EQProxy dejará de reenviar `X-Foo`. Eso es lo correcto por RFC; documentarlo.
- El stripping de petición puede cambiar el comportamiento de clientes h2 que
  hoy dependen del bug. Es un cambio de contrato deliberado.

---

## 4. HYP-01 — Mapeo `hdr`/`hdr_dom` en el importador HAProxy

### 4.1 Cambio recomendado

En `eqproxy-config/src/import/haproxy.rs` (rama de `:1815`):

- **Opción A (preferida):** eliminar `"hdr" | "hdr_dom"` del mapeo a
  `rule.host`, porque la sintaxis real es `hdr(<nombre>)` y no produce un token
  suelto. Si el parser ve un token suelto, debe emitir un diagnóstico de
  directiva no soportada, no una regla de host.
- **Opción B:** parsear `hdr(<nombre>)`; solo `hdr(host)` puede mapear a
  `rule.host`; el resto debe rechazarse con un aviso explícito.
- Añadir un test con `acl is_x hdr(X-Test) value` y comprobar que **no** se
  genera una regla de host.
- Actualizar la matriz de paridad del importador.

### 4.2 Criterio de aceptación

- El importador no genera `rule.host` a partir de `hdr`/`hdr_dom` sueltos.
- Los tests existentes del importador siguen verdes.
- No se afirma que HYP-01 sea explotable: es limpieza/robustez.

---

## 5. CI y observabilidad

1. **Fuzz en CI (semanal o por PR con presupuesto corto):**
   - `cargo +nightly fuzz run framing` / `config_import` / `rules` con
     `-max_total_time=60` y `-rss_limit_mb=2048`.
   - El target `rules` ya compila tras el arreglo del harness.
2. **Suite de propiedades:** `cargo test -p eqproxy-net --test
   adversarial_properties` en cada PR.
3. **Laboratorio E2E:** ejecutar `run_lab.py` como job opcional; hoy falla por
   diseño en S9/S10/S12 y debe pasar tras las correcciones. No bloquear el
   merge hasta que 1–2 estén corregidos; después, bloquear.
4. **Métricas:** añadir contadores de respuestas upstream rechazadas por
   framing bodyless y de cabeceras hop-by-hop eliminadas, para detectar
   backends/clientes anómalos en producción.
5. **Presupuesto de fuzzing mayor** antes de release: 10–30 min por target y
   campañas de h2/h3 cuando existan harness.

---

## 6. Documentación

- `THREAT_MODEL.md` T01: cambiar “parser fuzzing pending, Phase 2c” por el
  estado real (tres targets existen, `rules` reparado, presupuesto ejecutado).
- Añadir a la matriz de paridad/README la cobertura de fuzzing y propiedades.
- Documentar en el changelog el cambio de comportamiento de `204` con `CL`
  (paso a `502`) y de `304`/`HEAD` con `CL` (no pool).
- Enlazar este informe y el laboratorio desde `docs/audit-2026-11-09/` o el
  índice de auditorías, sin duplicar contenido.

---

## 7. Verificación final (checklist)

```bash
cd /Volumes/Lexar/Cursor/EQProxy

# 1. Suite completa
cargo test --workspace --no-fail-fast

# 2. Propiedades
cargo test -p eqproxy-net --test adversarial_properties

# 3. Binario de release
cargo build --release -p eqproxy-cli

# 4. Regresión E2E (debe salir 0 tras corregir ADV-01/ADV-02)
python3 report/adversarial-2026-11-09/lab/run_lab.py

# 5. Fuzz corto
cargo +nightly fuzz run framing fuzz/corpus/framing -- -max_total_time=60
cargo +nightly fuzz run config_import fuzz/corpus/config_import -- -max_total_time=60
cargo +nightly fuzz run rules fuzz/corpus/rules -- -max_total_time=60
```

| Check | Antes | Objetivo |
|---|---|---|
| S9 | FAIL (`SMUGGLED-RESPONSE`) | PASS (`NORMAL2`) |
| S10 | FAIL (`x-secret`) | PASS (sin `x-secret`) |
| S12 | FAIL (`x-evil`) | PASS (sin `x-evil`) |
| S1/S2 (AUD-01/02) | PASS | PASS (no regresión) |
| Workspace | PASS | PASS |
| Fuzz | 0 crashes | 0 crashes |

---

## 8. Qué **no** se propone

- No se propone reescribir el pool ni el parser completo: el fallo es una
  validación que falta y una decisión de reutilización demasiado optimista.
- No se propone confiar en timeouts como mitigación principal: no cierran la
  ventana de forma determinista.
- No se propone cambiar la severidad de ADV-01: el modelo de amenaza F2 ya
  considera al backend hostil.
- No se ha aplicado ningún parche durante la auditoría; este documento es una
  propuesta para el equipo.
