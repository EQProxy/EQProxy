//! Test-only oracle for the security fields the importer promises.
//!
//! The importer report is the operator's contract: a `Done` row says the value
//! reached the generated TOML. These tables name, per dialect, the TOML field
//! that each security directive claims to fill. The test below converts a real
//! source of every dialect and fails when a `Done` security row has no matching
//! entry or when the field named by that entry is absent from the output.
//!
//! `*` in a directive pattern matches any text; in a field path it walks every
//! entry of a TOML table or array. A `context` filter distinguishes the two
//! meanings Envoy gives `transport_socket tls` (a downstream listener vs. an
//! upstream cluster).

use super::{Note, Severity, Source};

/// One security directive → one field it must leave in `eqproxy.toml`.
#[derive(Debug, Clone, Copy)]
struct SecurityField {
    /// Report-directive pattern (`*` matches any run of characters).
    directive: &'static str,
    /// Required substring of the report context, or `""` for any context.
    context: &'static str,
    /// TOML path; `*` walks a table/array, final segment names the field.
    field: &'static str,
}

const fn field(directive: &'static str, field: &'static str) -> SecurityField {
    SecurityField {
        directive,
        context: "",
        field,
    }
}

const fn scoped(
    directive: &'static str,
    context: &'static str,
    field: &'static str,
) -> SecurityField {
    SecurityField {
        directive,
        context,
        field,
    }
}

/// nginx `http`/`server` TLS, mTLS, backend TLS and access denial.
const NGINX: &[SecurityField] = &[
    field("ssl_certificate *", "listener.*.tls.cert"),
    field("ssl_certificate_key *", "listener.*.tls.key"),
    // The server TLS block folds both directives into one report row.
    field(
        "ssl_certificate / ssl_certificate_key*",
        "listener.*.tls.cert",
    ),
    field(
        "ssl_certificate / ssl_certificate_key*",
        "listener.*.tls.key",
    ),
    field("ssl_protocols *", "listener.*.tls.min_version"),
    field("ssl_client_certificate *", "listener.*.tls.client_ca"),
    field("ssl_verify_client *", "listener.*.tls.client_ca"),
    field("proxy_ssl_verify *", "backend.*.tls.verify"),
    field("proxy_ssl_trusted_certificate *", "backend.*.tls.ca_file"),
    field("proxy_ssl_server_name *", "backend.*.tls.server_name"),
    field("proxy_ssl_name *", "backend.*.tls.server_name"),
    field("deny*", "listener.*.rules.ip"),
];

/// `HAProxy` listener TLS, backend TLS and `http-request deny`.
const HAPROXY: &[SecurityField] = &[
    field("bind * ssl*", "listener.*.tls.cert"),
    field("server * ssl*", "backend.*.tls.verify"),
    field(
        "http-request deny*",
        "listener.*.rules.direct_response.status",
    ),
];

/// Envoy TLS sockets and HTTP/2-to-origin.
const ENVOY: &[SecurityField] = &[
    scoped("transport_socket tls", "listener", "listener.*.tls.cert"),
    // The cluster loop reports its map-valued key as `transport_socket: {…}`,
    // while the listener branch has the literal TLS socket label.
    scoped("transport_socket*", "cluster", "backend.*.tls.verify"),
    scoped("http2_protocol_options", "cluster", "backend.*.tls.alpn"),
];

/// ingress-nginx TLS-adjacent annotations that become fields.
const INGRESS: &[SecurityField] = &[
    field("cors-allow-origin:*", "listener.*.rules.cors.allow_origin"),
    field("whitelist-source-range:*", "listener.*.rules.ip"),
    field("allowlist-source-range:*", "listener.*.rules.ip"),
    field("enable-hsts:*", "listener.*.rules.response_headers.name"),
    field("hsts-max-age:*", "listener.*.rules.response_headers.name"),
];

fn table(source: Source) -> &'static [SecurityField] {
    match source {
        Source::Nginx => NGINX,
        Source::Haproxy => HAPROXY,
        Source::Ingress => INGRESS,
        Source::Envoy => ENVOY,
    }
}

/// Whether the directive belongs to the security surface each table describes.
///
/// This is the coverage guard: a new `Done` security row that is not in the
/// table is a test failure, not an untested promise.
fn looks_security(source: Source, directive: &str) -> bool {
    match source {
        Source::Nginx => {
            directive.starts_with("ssl_")
                || directive.starts_with("proxy_ssl_")
                || directive.starts_with("deny")
        }
        Source::Haproxy => {
            (directive.starts_with("bind ") && directive.contains(" ssl"))
                || (directive.starts_with("server ") && directive.contains(" ssl"))
                || directive.starts_with("http-request deny")
                || directive.starts_with("http-request reject")
        }
        Source::Ingress => [
            "ssl-",
            "cors-",
            "hsts",
            "whitelist-",
            "allowlist-",
            "denylist-",
            "auth-",
        ]
        .iter()
        .any(|prefix| directive.starts_with(prefix)),
        Source::Envoy => {
            directive.starts_with("transport_socket")
                || directive.starts_with("http2_protocol_options")
                || directive.starts_with("tls_params")
        }
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let mut rest = text;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        let Some(position) = rest.find(part) else {
            return false;
        };
        rest = &rest[position + part.len()..];
    }
    true
}

fn matches(entry: &SecurityField, note: &Note) -> bool {
    wildcard(entry.directive, &note.directive)
        && (entry.context.is_empty() || note.context.contains(entry.context))
}

fn field_at<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let segments: Vec<&str> = path.split('.').collect();
    field_at_segments(value, &segments)
}

fn field_at_segments<'a>(value: &'a toml::Value, segments: &[&str]) -> Option<&'a toml::Value> {
    let Some((segment, rest)) = segments.split_first() else {
        return Some(value);
    };
    if *segment == "*" {
        return match value {
            toml::Value::Table(table) => table
                .values()
                .find_map(|item| field_at_segments(item, rest)),
            toml::Value::Array(items) => {
                items.iter().find_map(|item| field_at_segments(item, rest))
            }
            _ => None,
        };
    }
    match value {
        toml::Value::Table(table) => field_at_segments(table.get(*segment)?, rest),
        toml::Value::Array(items) => items
            .iter()
            .find_map(|item| field_at_segments(item, segments)),
        _ => None,
    }
}

fn field_present(toml_text: &str, path: &str) -> bool {
    let value: toml::Value = toml::from_str(toml_text).expect("generated TOML must parse");
    field_at(&value, path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{Conv, report};

    fn convert(source: Source, text: &str) -> (String, Vec<Note>) {
        let mut conv = Conv::new();
        match source {
            Source::Nginx => super::super::nginx::convert(&mut conv, text, None),
            Source::Haproxy => super::super::haproxy::convert(&mut conv, text),
            Source::Ingress => super::super::ingress::convert(&mut conv, text),
            Source::Envoy => super::super::envoy::convert(&mut conv, text),
        }
        .expect("fixture must convert");
        let toml = report::emit_toml(&conv);
        (toml, conv.notes.clone())
    }

    const NGINX_FIXTURE: &str = "http {\n  proxy_ssl_unknown_http on;\n  ssl_certificate /etc/http.pem;\n  ssl_certificate_key /etc/http.key;\n  ssl_client_certificate /etc/clients.pem;\n  ssl_verify_client on;\n  ssl_protocols TLSv1.2 TLSv1.3;\n  upstream app { server 10.0.0.10:8443; }\n  server {\n    listen 443 ssl default_server;\n    server_name app.example;\n    deny 10.0.0.0/8;\n    location / {\n      proxy_pass https://backend.internal:8443;\n      proxy_ssl_unknown_location on;\n      proxy_ssl_verify on;\n      proxy_ssl_trusted_certificate /etc/ca.pem;\n      proxy_ssl_server_name on;\n      proxy_ssl_name app.example;\n    }\n    location /off {\n      proxy_pass https://off.internal:8443;\n      proxy_ssl_server_name off;\n    }\n    location /ip {\n      proxy_pass https://10.0.0.9:8443;\n      proxy_ssl_server_name on;\n    }\n  }\n  server {\n    listen 8081;\n    ssl_protocols TLSv1.2;\n    location / { proxy_pass http://127.0.0.1:8080; }\n  }\n}\n";

    fn fixtures() -> [(Source, &'static str); 4] {
        [
            (Source::Nginx, NGINX_FIXTURE),
            (
                Source::Haproxy,
                include_str!("../../../docs/evidence/v085-config-import/raw/haproxy.cfg"),
            ),
            (
                Source::Ingress,
                include_str!("../../../docs/evidence/v085-config-import/raw/ingress-nginx.yaml"),
            ),
            (
                Source::Envoy,
                include_str!("../../../docs/evidence/v085-config-import/raw/envoy.yaml"),
            ),
        ]
    }

    #[test]
    fn the_field_oracle_rejects_a_missing_nested_field() {
        let toml = "[listener.web.tls]\ncert = \"/etc/site.pem\"\n\n[[listener.web.rules]]\nip = [\"10.0.0.0/8\"]\nresponse_headers = [{ name = \"Strict-Transport-Security\", value = \"max-age=1\" }]\n\n[backend.app.tls]\nverify = false\n";
        assert!(field_present(toml, "listener.*.tls.cert"));
        assert!(field_present(toml, "listener.*.rules.ip"));
        assert!(field_present(
            toml,
            "listener.*.rules.response_headers.name"
        ));
        assert!(field_present(toml, "backend.*.tls.verify"));
        assert!(
            !field_present(toml, "listener.*.tls.key"),
            "the oracle must fail when the promised field is absent"
        );
        assert!(!field_present(toml, "backend.*.tls.server_name"));
    }

    #[test]
    fn every_done_security_directive_has_its_toml_field() {
        for (source, text) in fixtures() {
            let (toml, notes) = convert(source, text);
            for note in notes.iter().filter(|note| note.severity == Severity::Done) {
                if !looks_security(source, &note.directive) {
                    continue;
                }
                let entries: Vec<&SecurityField> = table(source)
                    .iter()
                    .filter(|entry| matches(entry, note))
                    .collect();
                assert!(
                    !entries.is_empty(),
                    "{}: `{}` is classified Done but has no security-field entry; \
                     add it to the table or stop promising a conversion\n{}",
                    source.as_str(),
                    note.directive,
                    note.message
                );
                for entry in entries {
                    assert!(
                        field_present(&toml, entry.field),
                        "{}: `{}` is classified Done but `{}` is absent from the TOML\n{}",
                        source.as_str(),
                        note.directive,
                        entry.field,
                        toml
                    );
                }
            }
        }
    }
}
