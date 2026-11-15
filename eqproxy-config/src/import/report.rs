//! TOML emitter and Markdown report of the configuration importer.
//!
//! The emitter writes only the fields the schema does not default to, so the
//! file stays readable; the converter then parses and validates the result
//! with `EQProxy`'s own parser before handing it over (`import::convert`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Conv, Counts, Severity, Source, toml_key, toml_string};

/// Serialises the accumulated IR as `eqproxy.toml`.
#[allow(clippy::too_many_lines)]
pub fn emit_toml(conv: &Conv) -> String {
    let mut out = String::new();
    out.push_str("# Generado por `eqproxy config import`.\n");
    out.push_str("# Revise el informe de conversión antes de usarlo en producción.\n");
    out.push_str("schema_version = 1\n");

    for (name, backend) in &conv.backends {
        let _ = writeln!(out, "\n[backend.{}]", toml_key(name));
        if backend.servers.len() == 1
            && backend.servers[0].weight == 1
            && !backend.servers[0].backup
            && backend.servers[0].maxconn == 0
        {
            let _ = writeln!(out, "server = {}", toml_string(&backend.servers[0].addr));
        } else if !backend.servers.is_empty() {
            let rendered: Vec<String> = backend
                .servers
                .iter()
                .map(|server| {
                    let mut fields = vec![format!("addr = {}", toml_string(&server.addr))];
                    if server.weight != 1 {
                        fields.push(format!("weight = {}", server.weight));
                    }
                    if server.backup {
                        fields.push("backup = true".to_owned());
                    }
                    if server.maxconn != 0 {
                        fields.push(format!("maxconn = {}", server.maxconn));
                    }
                    format!("{{ {} }}", fields.join(", "))
                })
                .collect();
            let _ = writeln!(out, "servers = [{}]", rendered.join(", "));
        }
        if !backend.balance.is_empty() && backend.balance != "roundrobin" {
            let _ = writeln!(out, "balance = {}", toml_string(&backend.balance));
        }
        if backend.proxy_protocol_v1 {
            out.push_str("proxy_protocol = \"v1\"\n");
        }
        if backend.retries > 0 {
            let _ = writeln!(out, "retries = {}", backend.retries);
        }
        if backend.maxqueue > 0 {
            let _ = writeln!(out, "maxqueue = {}", backend.maxqueue);
        }
        if let Some(slow_start) = &backend.slow_start {
            let _ = writeln!(out, "slow_start = {}", toml_string(slow_start));
        }

        if let Some(health) = &backend.health_check {
            let _ = writeln!(out, "\n[backend.{}.health_check]", toml_key(name));
            let _ = writeln!(out, "interval = {}", toml_string(&health.interval));
            let _ = writeln!(out, "timeout = {}", toml_string(&health.timeout));
            let _ = writeln!(out, "fall = {}", health.fall);
            let _ = writeln!(out, "rise = {}", health.rise);
            if let Some(kind) = health.kind {
                let _ = writeln!(out, "type = {}", toml_string(kind));
            }
            if let Some(path) = &health.http_path {
                let _ = writeln!(out, "http_path = {}", toml_string(path));
            }
            if let Some(status) = &health.expect_status {
                let _ = writeln!(out, "expect_status = {}", toml_string(status));
            }
            if let Some(host) = &health.http_host {
                let _ = writeln!(out, "http_host = {}", toml_string(host));
            }
        }

        if let Some(tls) = &backend.tls {
            let _ = writeln!(out, "\n[backend.{}.tls]", toml_key(name));
            let _ = writeln!(out, "verify = {}", tls.verify);
            if let Some(ca) = &tls.ca_file {
                let _ = writeln!(out, "ca_file = {}", toml_string(ca));
            }
            if let Some(server_name) = &tls.server_name {
                let _ = writeln!(out, "server_name = {}", toml_string(server_name));
            }
            if !tls.alpn.is_empty() {
                let _ = writeln!(out, "alpn = {}", string_list(&tls.alpn));
            }
            if let Some(version) = tls.min_version {
                let _ = writeln!(out, "min_version = {}", toml_string(version));
            }
        }

        if let Some(affinity) = &backend.affinity {
            let _ = writeln!(out, "\n[backend.{}.affinity]", toml_key(name));
            let _ = writeln!(out, "mode = {}", toml_string(&affinity.mode));
            if let Some(cookie) = &affinity.cookie {
                let _ = writeln!(out, "cookie = {}", toml_string(cookie));
            }
        }
    }

    for (name, listener) in &conv.listeners {
        let _ = writeln!(out, "\n[listener.{}]", toml_key(name));
        let _ = writeln!(out, "listen = {}", toml_string(&listener.listen));
        let _ = writeln!(out, "mode = {}", toml_string(&listener.mode));
        if let Some(backend) = &listener.backend {
            let _ = writeln!(out, "backend = {}", toml_string(backend));
        }
        if listener.maxconn != 1024 {
            let _ = writeln!(out, "maxconn = {}", listener.maxconn);
        }
        for (field, value) in [
            ("connect_timeout", &listener.connect_timeout),
            ("idle_timeout", &listener.idle_timeout),
            ("client_header_timeout", &listener.client_header_timeout),
            ("server_header_timeout", &listener.server_header_timeout),
        ] {
            if let Some(value) = value {
                let _ = writeln!(out, "{field} = {}", toml_string(value));
            }
        }
        if let Some(forwarded) = listener.forwarded_headers {
            let _ = writeln!(out, "forwarded_headers = {}", toml_string(forwarded));
        }
        if listener.proxy_receive {
            out.push_str("proxy_receive = true\n");
        }
        if listener.quic {
            out.push_str("quic = true\n");
        }

        if let Some((status, location)) = &listener.redirect {
            let _ = writeln!(out, "\n[listener.{}.redirect]", toml_key(name));
            if let Some(status) = status {
                let _ = writeln!(out, "status = {status}");
            }
            let _ = writeln!(out, "location = {}", toml_string(location));
        }

        if let Some(tls) = &listener.tls {
            let _ = writeln!(out, "\n[listener.{}.tls]", toml_key(name));
            let _ = writeln!(out, "cert = {}", toml_string(&tls.cert));
            let _ = writeln!(out, "key = {}", toml_string(&tls.key));
            if !tls.alpn.is_empty() {
                let _ = writeln!(out, "alpn = {}", string_list(&tls.alpn));
            }
            if let Some(version) = tls.min_version {
                let _ = writeln!(out, "min_version = {}", toml_string(version));
            }
            if let Some(version) = tls.max_version {
                let _ = writeln!(out, "max_version = {}", toml_string(version));
            }
            if let Some(ca) = &tls.client_ca {
                let _ = writeln!(out, "client_ca = {}", toml_string(ca));
            }
            for cert in &tls.certs {
                let _ = writeln!(out, "\n[[listener.{}.tls.certs]]", toml_key(name));
                let _ = writeln!(out, "cert = {}", toml_string(&cert.cert));
                let _ = writeln!(out, "key = {}", toml_string(&cert.key));
                let _ = writeln!(out, "names = {}", string_list(&cert.names));
            }
        }
        if listener.compression_gzip {
            let _ = writeln!(out, "\n[listener.{}.compression]", toml_key(name));
            out.push_str("gzip = true\n");
            if let Some(level) = listener.compression_level {
                let _ = writeln!(out, "level = {level}");
            }
            if let Some(min_size) = listener.compression_min_size {
                let _ = writeln!(out, "min_size = {min_size}");
            }
        }
        if let Some(max_body_bytes) = listener.max_body_bytes {
            let _ = writeln!(out, "\n[listener.{}.limits]", toml_key(name));
            let _ = writeln!(out, "max_body_bytes = {max_body_bytes}");
        }
        if listener.cache {
            let _ = writeln!(out, "\n[listener.{}.cache]", toml_key(name));
        }
        for page in &listener.error_pages {
            let _ = writeln!(out, "\n[[listener.{}.error_pages]]", toml_key(name));
            let codes = page
                .codes
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "codes = [{codes}]");
            if let Some(file) = &page.file {
                let _ = writeln!(out, "file = {}", toml_string(file));
            }
            if let Some(body) = &page.body {
                let _ = writeln!(out, "body = {}", toml_string(body));
            }
            if let Some(location) = &page.location {
                let _ = writeln!(out, "location = {}", toml_string(location));
            }
            if page.raw {
                out.push_str("format = \"raw\"\n");
            }
            if let Some(status) = page.status {
                let _ = writeln!(out, "status = {status}");
            }
            if page.intercept {
                out.push_str("intercept = true\n");
            }
        }
        for rule in &listener.rules {
            let _ = writeln!(out, "\n[[listener.{}.rules]]", toml_key(name));
            if !rule.name.is_empty() {
                let _ = writeln!(out, "name = {}", toml_string(&rule.name));
            }
            if let Some(backend) = &rule.use_backend {
                let _ = writeln!(out, "use_backend = {}", toml_string(backend));
            }
            if let Some(prefix) = &rule.path_prefix {
                let _ = writeln!(out, "path_prefix = {}", toml_string(prefix));
            }
            if let Some(path) = &rule.path {
                let _ = writeln!(out, "path = {}", toml_string(path));
            }
            if let Some(host) = &rule.host {
                let _ = writeln!(out, "host = {}", toml_string(host));
            }
            if let Some(prefix) = &rule.host_prefix {
                let _ = writeln!(out, "host_prefix = {}", toml_string(prefix));
            }
            if let Some(regex) = &rule.host_regex {
                let _ = writeln!(out, "host_regex = {}", toml_string(regex));
            }
            if !rule.headers.is_empty() {
                let rendered: Vec<String> = rule
                    .headers
                    .iter()
                    .map(|header| {
                        let condition = if header.present {
                            "present = true".to_owned()
                        } else if let Some(exact) = &header.exact {
                            format!("exact = {}", toml_string(exact))
                        } else if let Some(regex) = &header.regex {
                            format!("regex = {}", toml_string(regex))
                        } else {
                            "present = false".to_owned()
                        };
                        format!("{{ name = {}, {condition} }}", toml_string(&header.name))
                    })
                    .collect();
                let _ = writeln!(out, "headers = [{}]", rendered.join(", "));
            }
            if !rule.methods.is_empty() {
                let _ = writeln!(out, "methods = {}", string_list(&rule.methods));
            }
            if !rule.ip.is_empty() {
                let _ = writeln!(out, "ip = {}", string_list(&rule.ip));
            }
            if !rule.set_headers.is_empty() {
                let _ = writeln!(out, "set_headers = {}", header_ops(&rule.set_headers));
            }
            if !rule.add_headers.is_empty() {
                let _ = writeln!(out, "add_headers = {}", header_ops(&rule.add_headers));
            }
            if !rule.response_headers.is_empty() {
                let _ = writeln!(
                    out,
                    "response_headers = {}",
                    header_ops(&rule.response_headers)
                );
            }
            if let Some(origin) = &rule.cors_origin {
                let _ = writeln!(out, "cors = {{ allow_origin = {} }}", toml_string(origin));
            }
            if let Some((rate, per, burst)) = &rule.rate_limit {
                let burst = burst.map_or_else(String::new, |value| format!(", burst = {value}"));
                let _ = writeln!(
                    out,
                    "rate_limit = {{ rate = {rate}, per = {}{burst} }}",
                    toml_string(per)
                );
            }
            if let Some(prefix) = &rule.rewrite_prefix {
                let _ = writeln!(
                    out,
                    "rewrite = {{ path_prefix_rewrite = {} }}",
                    toml_string(prefix)
                );
            }
            if let Some((status, body)) = &rule.direct_response {
                match body {
                    Some(body) => {
                        let _ = writeln!(
                            out,
                            "direct_response = {{ status = {status}, body = {} }}",
                            toml_string(body)
                        );
                    }
                    None => {
                        let _ = writeln!(out, "direct_response = {{ status = {status} }}");
                    }
                }
            }
            if let Some((status, location)) = &rule.redirect {
                match status {
                    Some(status) => {
                        let _ = writeln!(
                            out,
                            "redirect = {{ status = {status}, location = {} }}",
                            toml_string(location)
                        );
                    }
                    None => {
                        let _ =
                            writeln!(out, "redirect = {{ location = {} }}", toml_string(location));
                    }
                }
            }
        }
    }
    out
}

fn string_list(values: &[String]) -> String {
    let rendered: Vec<String> = values.iter().map(|value| toml_string(value)).collect();
    format!("[{}]", rendered.join(", "))
}

fn header_ops(ops: &[super::HeaderOp]) -> String {
    let rendered: Vec<String> = ops
        .iter()
        .map(|op| {
            format!(
                "{{ name = {}, value = {} }}",
                toml_string(&op.name),
                toml_string(&op.value)
            )
        })
        .collect();
    format!("[{}]", rendered.join(", "))
}

/// Renders the Markdown report. `input`/`output` are placeholders that
/// [`super::report_with_paths`] replaces with the real names.
#[allow(clippy::too_many_lines)]
pub fn render(source: Source, conv: &Conv, counts: Counts) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Informe de conversión {} → EQProxy\n",
        source_label(source)
    );
    let _ = writeln!(out, "- Entrada: @input@");
    let _ = writeln!(out, "- Salida: @output@");
    let _ = writeln!(
        out,
        "- Listeners generados: **{}** · Backends generados: **{}**",
        conv.listeners.len(),
        conv.backends.len()
    );
    let degraded = conv.degraded_count();
    let _ = writeln!(
        out,
        "- Directivas leídas: **{}** · clasificadas: **{}** — convertidas **{}** · con degradación **{}** · con aviso **{}** · rechazadas **{}**",
        conv.walked,
        conv.notes.len(),
        counts.converted,
        degraded,
        counts.review - degraded,
        counts.rejected
    );
    out.push_str(
        "\n«Nunca se descarta en silencio»: cada directiva del origen aparece abajo. \
         «Con degradación» son directivas que sí están en el TOML pero con menos \
         seguridad que la que declaraba el origen; junto con «con aviso» forman la \
         columna «revisar». \
         El fichero generado se ha parseado y validado con el propio parser de EQProxy \
         (`schema_version = 1`) antes de escribirlo. La conversión es estática: no se ha \
         ejecutado nada de la configuración de origen ni se ha medido tráfico.\n",
    );

    let _ = writeln!(out, "\n## Conversión generada\n");
    out.push_str("| listener | escucha | modo | backend | TLS |\n|---|---|---|---|---|\n");
    for (name, listener) in &conv.listeners {
        let fallback = match (&listener.backend, &listener.redirect) {
            (Some(backend), None) => format!("`{backend}`"),
            (None, Some((Some(status), _))) => format!("redirect {status}"),
            (None, Some((None, _))) => "redirect 301".to_owned(),
            _ => "—".to_owned(),
        };
        let _ = writeln!(
            out,
            "| `{name}` | {} | {} | {fallback} | {} |",
            listener.listen,
            listener.mode,
            if listener.tls.is_some() { "sí" } else { "no" }
        );
    }

    out.push_str("\n| backend | servidores (peso) | balance | health check | PROXY | TLS backend |\n|---|---|---|---|---|---|\n");
    for (name, backend) in &conv.backends {
        let servers = if backend.servers.is_empty() {
            "—".to_owned()
        } else {
            backend
                .servers
                .iter()
                .map(|server| {
                    let mut cell = format!("{} ({})", server.addr, server.weight);
                    if server.backup {
                        cell.push_str(" backup");
                    }
                    cell
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let health = backend.health_check.as_ref().map_or_else(
            || "—".to_owned(),
            |health| {
                let mut text = format!(
                    "{} {}/{}/fall {}/rise {}",
                    health.kind.unwrap_or("tcp").to_uppercase(),
                    health.interval,
                    health.timeout,
                    health.fall,
                    health.rise
                );
                if let Some(path) = &health.http_path
                    && health.kind == Some("http")
                {
                    let _ = write!(text, " {path}");
                }
                text
            },
        );
        let balance = if backend.balance.is_empty() {
            "roundrobin"
        } else {
            &backend.balance
        };
        let _ = writeln!(
            out,
            "| `{name}` | {servers} | {balance} | {health} | {} | {} |",
            if backend.proxy_protocol_v1 {
                "v1"
            } else {
                "none"
            },
            if backend.tls.is_some() { "sí" } else { "no" }
        );
    }

    let _ = writeln!(out, "\n## Detalle de la clasificación\n");
    for (severity, title) in [
        (Severity::Rejected, "❌ no traducible"),
        (Severity::Gap, "⛔ rechazado (sin equivalente en EQProxy)"),
        (
            Severity::Degraded,
            "⚠️ convertido con degradación (revisar)",
        ),
        (Severity::Review, "⚠️ revisar"),
        (Severity::Info, "ℹ️ informativo"),
        (Severity::Done, "✅ convertido"),
    ] {
        let notes: Vec<_> = conv
            .notes
            .iter()
            .filter(|note| note.severity == severity)
            .collect();
        let _ = writeln!(out, "### {title} ({})\n", notes.len());
        for note in notes {
            let _ = writeln!(
                out,
                "- `{}` — *{}*: {}",
                single_line(&note.directive),
                single_line(&note.context),
                single_line(&note.message)
            );
        }
        out.push('\n');
    }

    out.push_str("## Qué no se ha traducido\n\n");
    let mut gaps: BTreeMap<&str, usize> = BTreeMap::new();
    for note in conv
        .notes
        .iter()
        .filter(|note| matches!(note.severity, Severity::Gap | Severity::Rejected))
    {
        *gaps.entry(note.message.as_str()).or_insert(0) += 1;
    }
    if gaps.is_empty() {
        out.push_str(
            "Ninguna: todas las directivas del origen tienen destino o son informativas.\n",
        );
    } else {
        for (message, count) in gaps {
            let _ = writeln!(out, "- {} ({count})", single_line(message));
        }
        out.push_str(
            "\nCada uno de estos puntos es un cambio de comportamiento frente al proxy de origen. \
             Hasta que se resuelvan (o se acepten por escrito), la configuración **no es equivalente**.\n",
        );
    }

    // N-2: a loss of security is a loss too, and it gets its own summary. The
    // detail section lists it above, but this is the paragraph an operator
    // reads before switching.
    let mut losses: BTreeMap<&str, usize> = BTreeMap::new();
    for note in conv
        .notes
        .iter()
        .filter(|note| note.severity == Severity::Degraded)
    {
        *losses.entry(note.message.as_str()).or_insert(0) += 1;
    }
    if !losses.is_empty() {
        out.push_str("\n## Convertido con degradación\n\n");
        for (message, count) in losses {
            let _ = writeln!(out, "- {} ({count})", single_line(message));
        }
        out.push_str(
            "\nEstas directivas **sí** están en el TOML, pero sin la garantía que declaraba el \
             origen. Hasta que se corrijan (o se acepten por escrito), la configuración no es \
             equivalente en seguridad.\n",
        );
    }

    let _ = writeln!(
        out,
        "\n## Siguiente paso\n\n1. `eqproxy config validate --config <salida>` — el mismo gate que usa el binario.\n\
         2. Leer las secciones «revisar» y «rechazado» de arriba y corregir a mano lo que falte.\n\
         3. `eqproxy config diff --help` para comparar con la configuración actual antes de `config apply`."
    );
    out
}

/// The degraded count declared in the report's summary header.
///
/// Only the text before the first `##` section is read: the detail heading
/// `### … (0)` is a separate number and must never become the signal.
fn declared_degraded_count(report: &str) -> Option<usize> {
    let line = report
        .lines()
        .take_while(|line| !line.starts_with("## "))
        .find(|line| line.starts_with("- Directivas leídas:"))?;
    let (_, tail) = line.split_once("con degradación **")?;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The summary's degraded count must equal the names the caller will print.
///
/// This is the only runtime consistency check: it stays in the summary, never
/// in the detail section, and runs before the caller can write any file. The
/// names remain the source of truth; the report supplies only the count it
/// promises to have listed.
pub(super) fn check_degraded_names(report: &str, names: &[String]) -> Result<(), String> {
    let declared = declared_degraded_count(report)
        .ok_or_else(|| "internal: the summary does not declare its degraded count".to_owned())?;
    if declared != names.len() {
        return Err(format!(
            "internal: the summary declares **{declared}** degraded directive(s) but {} name(s) were extracted",
            names.len()
        ));
    }
    if names.iter().any(|name| name.trim().is_empty()) {
        return Err("internal: a degraded directive was extracted without a name".to_owned());
    }
    Ok(())
}

/// The degraded directives, one line each, in classification order.
///
/// This is the machine-readable signal `config import` summarizes; it comes
/// from the same notes the report renders, so it never depends on parsing the
/// report's Markdown or on a particular report section.
pub(super) fn degraded_directives(conv: &Conv) -> Vec<String> {
    conv.notes
        .iter()
        .filter(|note| note.severity == Severity::Degraded)
        .map(|note| single_line(&note.directive))
        .collect()
}

/// Keeps one source note on one Markdown line. A directive can contain a
/// newline (quoted or malformed source text); without this, its second line
/// would be indistinguishable from another classification bullet.
fn single_line(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Nginx => "nginx",
        Source::Haproxy => "HAProxy",
        Source::Ingress => "ingress-nginx",
        Source::Envoy => "Envoy",
    }
}

/// Renders a value as a `CommonMark` inline code span.
///
/// A code span cannot contain a backtick run as long as its delimiter, so the
/// delimiter is one backtick longer than the longest run in the value. CR/LF
/// become their visible `\r`/`\n` escapes: a path cannot create a new heading
/// or list item even though the value is interpolated into Markdown. Normal
/// paths keep the previous single-backtick rendering.
fn markdown_code_span(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(c),
        }
    }

    let mut longest_run = 0;
    let mut run = 0;
    for c in escaped.chars() {
        if c == '`' {
            run += 1;
            longest_run = longest_run.max(run);
        } else {
            run = 0;
        }
    }
    let delimiter = "`".repeat(longest_run + 1);
    let needs_padding = escaped.starts_with('`')
        || escaped.ends_with('`')
        || escaped.starts_with(' ')
        || escaped.ends_with(' ');
    if needs_padding {
        format!("{delimiter} {escaped} {delimiter}")
    } else {
        format!("{delimiter}{escaped}{delimiter}")
    }
}

/// Replaces the path placeholders of a report with safe Markdown code spans.
#[must_use]
pub fn report_with_paths(report: &str, input: &str, output: &str) -> String {
    let input = markdown_code_span(input);
    let output = markdown_code_span(output);
    report
        .replacen("@input@", &input, 1)
        .replacen("@output@", &output, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Degradation;

    #[test]
    fn paths_are_substituted_once_and_untouched_when_absent() {
        let report = "- Entrada: @input@\n- Salida: @output@\n";
        let rendered = report_with_paths(report, "a/nginx.conf", "out/eqproxy.toml");
        assert_eq!(
            rendered,
            "- Entrada: `a/nginx.conf`\n- Salida: `out/eqproxy.toml`\n"
        );
        assert_eq!(
            report_with_paths("sin marcadores", "x", "y"),
            "sin marcadores"
        );
    }

    #[test]
    fn markdown_metacharacters_stay_inside_one_code_span() {
        // Backticks would close the old code span and the CR/LF would let the
        // rest of the path look like a section heading. Two backticks force a
        // three-backtick delimiter; the newline must remain one line and one
        // code span.
        let rendered = report_with_paths(
            "- Entrada: @input@\n- Salida: @output@\n",
            "in\n## SECCIÓN FALSA ``\nname.conf",
            "out\n## SECCIÓN FALSA ``\nname.toml",
        );
        assert_eq!(rendered.lines().count(), 2, "{rendered}");
        assert!(
            !rendered
                .lines()
                .any(|line| line.starts_with("## SECCIÓN FALSA")),
            "the path must not create a Markdown heading:\n{rendered}"
        );
        assert_eq!(
            rendered.lines().next(),
            Some(r"- Entrada: ```in\n## SECCIÓN FALSA ``\nname.conf```")
        );
        assert_eq!(
            rendered.lines().nth(1),
            Some(r"- Salida: ```out\n## SECCIÓN FALSA ``\nname.toml```")
        );
    }

    #[test]
    fn the_summary_names_exactly_the_degraded_directives() {
        let mut conv = Conv::new();
        conv.done(1, "proxy_pass https://app", "location /", "regla emitida");
        conv.degraded(
            1,
            "proxy_ssl_certificate /etc/client.pem",
            "location /",
            Degradation::ClientCertificate,
            "el backend no lo presenta",
        );
        conv.degraded(
            1,
            "proxy_ssl_protocols TLSv1.3",
            "location /",
            Degradation::ProtocolPolicy,
            "se negocia con otra política",
        );
        conv.review(1, "gzip on", "http", "por listener");
        let names = degraded_directives(&conv);
        assert_eq!(
            names,
            vec![
                "proxy_ssl_certificate /etc/client.pem".to_owned(),
                "proxy_ssl_protocols TLSv1.3".to_owned()
            ],
            "the summary names the degraded directives, not the reviewed ones"
        );
        let report = render(Source::Nginx, &conv, conv.classify());
        assert!(
            report.contains("con degradación **2**"),
            "the rendered summary must count both losses:\n{report}"
        );
        assert!(
            report.contains("### ⚠️ convertido con degradación (revisar) (2)"),
            "the detail header must count the same losses, not a section-local number:\n{report}"
        );
        assert!(
            check_degraded_names(&report, &names).is_ok(),
            "the summary count and the extracted names must agree:\n{report}"
        );
        let error = check_degraded_names(&report, &names[..1])
            .expect_err("one name for two summary losses must fail");
        assert!(
            error.contains("summary declares **2**") && error.contains("1 name(s)"),
            "the guard must name the summary count and the extraction: {error}"
        );

        let mut clean = Conv::new();
        clean.done(1, "proxy_pass http://app", "location /", "regla emitida");
        assert!(
            degraded_directives(&clean).is_empty(),
            "a clean conversion has no degraded directive"
        );
        let clean_report = render(Source::Nginx, &clean, clean.classify());
        assert!(
            clean_report.contains("con degradación **0**"),
            "a clean report declares zero in its summary:\n{clean_report}"
        );
        assert!(
            clean_report.contains("### ⚠️ convertido con degradación (revisar) (0)"),
            "the detail header at (0) must be harmless:\n{clean_report}"
        );
        assert!(
            check_degraded_names(&clean_report, &[]).is_ok(),
            "a clean report's detail (0) must not be read as a loss:\n{clean_report}"
        );
    }

    #[test]
    fn a_multiline_directive_does_not_create_a_second_bullet() {
        let mut conv = Conv::new();
        conv.review(1, "first\n- `second`", "ctx", "message\n- `tail`");
        let counts = conv.classify();
        let report = render(Source::Nginx, &conv, counts);
        assert_eq!(
            report
                .lines()
                .filter(|line| line.starts_with("- `"))
                .count(),
            1,
            "one classified directive must render one bullet:\n{report}"
        );
    }
}
