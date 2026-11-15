//! Corpus gate for `eqproxy config import` (Phase 8a, CE6/Q9/CE8).
//!
//! The four reference configurations in `docs/evidence/v085-config-import/raw/`
//! are real files (a `HAProxy`, an nginx, an ingress-nginx multi-document
//! manifest and an Envoy v3 bootstrap). For each one this test asserts the
//! contract the converter promises:
//!
//! 1. the generated `eqproxy.toml` parses **and** validates with the product's
//!    own parser (`import::convert` does it, and the test re-parses the text
//!    independently so a regression in `convert` cannot hide it);
//! 2. every walked source directive produced a classification — the report's
//!    "leídas" and "clasificadas" counters agree, and the report lists all of
//!    them;
//! 3. the conversion is not empty and names what it refused: each corpus file
//!    exercises features `EQProxy` does not have (syslog, Lua, ssl-passthrough,
//!    xDS), and those must appear as rejected, never disappear;
//! 4. converting twice yields the same bytes (deterministic: `BTreeMap`s and no
//!    timestamps).
//!
//! It does not claim traffic equivalence: that would need the origin backends
//! of each file. What it proves is the shape of the artifact and the honesty of
//! the report.

use std::path::{Path, PathBuf};

use eqproxy_config::import::{self, Outcome, Source};

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/evidence/v085-config-import/raw")
        .join(name)
}

fn include_corpus(name: &str) -> PathBuf {
    corpus("nginx-includes").join(name)
}

fn convert(name: &str, source: Source) -> (String, Outcome) {
    let path = corpus(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("corpus file {} must be readable: {err}", path.display()));
    let outcome = import::convert(source, &text).unwrap_or_else(|err| {
        panic!("{name} must convert: {err}");
    });
    (text, outcome)
}

fn assert_contract(name: &str, text: &str, outcome: &Outcome) {
    // 1. The file validates with the real parser, not with a replica.
    let parsed = eqproxy_config::parse_str(&outcome.toml, Path::new(name))
        .unwrap_or_else(|err| panic!("{name}: generated TOML must parse: {err}"));
    let check = eqproxy_config::validate::check(&parsed, &outcome.toml, Path::new(name));
    assert!(
        check.is_valid(),
        "{name}: generated configuration must validate: {:?}",
        check
            .errors
            .iter()
            .map(|err| (err.field.clone(), err.cause.clone()))
            .collect::<Vec<_>>()
    );

    // 2. Every walked directive was classified and appears in the report.
    let walked = counter(&outcome.report, "Directivas leídas:");
    let classified = counter(&outcome.report, "clasificadas:");
    assert_eq!(
        walked, classified,
        "{name}: a walked directive produced no classification (silent omission)"
    );
    assert!(
        walked > 0,
        "{name}: the converter must walk the source, not skip it"
    );

    // 3. What the source has and EQProxy does not must be named.
    let rejected = outcome.counts.rejected;
    assert!(
        rejected > 0,
        "{name}: this corpus file exercises features EQProxy does not have today, \
         so at least one directive must be classified as rejected"
    );
    assert!(
        outcome.report.contains("Qué no se ha traducido"),
        "{name}: the report must summarise what was not translated"
    );
    assert!(
        !listeners_summary(&outcome.report).is_empty(),
        "{name}: at least one listener must be generated"
    );

    // 4. Deterministic output.
    let again = import::convert(source_of(name), text).expect("second pass");
    assert_eq!(
        again.toml, outcome.toml,
        "{name}: TOML must be deterministic"
    );
    assert_eq!(
        again.report, outcome.report,
        "{name}: report must be deterministic"
    );
}

/// Extracts the integer that follows `label` in the report header.
fn counter(report: &str, label: &str) -> usize {
    let start = report
        .find(label)
        .unwrap_or_else(|| panic!("report must carry «{label}»"));
    let rest = &report[start + label.len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("«{label}» must be followed by a number"))
}

fn source_of(name: &str) -> Source {
    if name.to_ascii_lowercase().ends_with(".cfg") {
        Source::Haproxy
    } else if name.to_ascii_lowercase().ends_with(".yaml") {
        Source::detect(&std::fs::read_to_string(corpus(name)).expect("read"))
    } else {
        Source::Nginx
    }
}

/// First listener row of the report table, proving the table is not empty.
fn listeners_summary(report: &str) -> String {
    report
        .lines()
        .skip_while(|line| !line.starts_with("| listener |"))
        .nth(2)
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn haproxy_frontend_backend_and_httpchk_convert() {
    let (text, outcome) = convert("haproxy.cfg", Source::Haproxy);
    assert_contract("haproxy.cfg", &text, &outcome);
    // HTTP health check is a real field since B-H6, not a gap.
    assert!(
        outcome.toml.contains("type = \"http\""),
        "option httpchk must become an HTTP health check:\n{}",
        outcome.toml
    );
    assert!(
        outcome.toml.contains("path_prefix = \"/api\"") || outcome.toml.contains("[[listener."),
        "rules must be emitted for the frontends that need them"
    );
    // A hostname server is accepted now (B-H13), not an error.
    assert!(
        outcome.report.contains("ejemplocpuerta.local"),
        "the hostname server must appear in the report"
    );
    // N-2: this file declares no TLS policy EQProxy cannot express, so the
    // report gains no degradation it cannot justify.
    assert!(
        !outcome.report.contains("Convertido con degradación"),
        "haproxy.cfg loses no security property:\n{}",
        outcome.report
    );
}

#[test]
fn nginx_vhosts_share_one_listener_with_host_rules_and_certs() {
    let (text, outcome) = convert("nginx.conf", Source::Nginx);
    assert_contract("nginx.conf", &text, &outcome);
    assert!(
        outcome.toml.contains("path_prefix = \"/api/\""),
        "the /api/ location must become a path rule:\n{}",
        outcome.toml
    );
    // Both vhosts share one listener: the default_server's certificate is the
    // listener's own, the other vhost rides in `certs[]`, and every rule is
    // tagged with its host so the two sites stay apart.
    // A top-level table is `[listener.<name>]`: one dot (after `listener`),
    // unlike `[listener.<name>.tls]` and the other subsections.
    let top_level_listeners = outcome
        .toml
        .lines()
        .filter(|line| {
            line.starts_with("[listener.")
                && line.ends_with(']')
                && line[1..line.len() - 1].matches('.').count() == 1
        })
        .count();
    assert_eq!(
        top_level_listeners, 2,
        "one listener per IP:port, including the 80→443 redirect listener:\n{}",
        outcome.toml
    );
    // The redirect-only server becomes a real listener whose fallback is a
    // local 3xx: no backend is invented for it and no request reaches one.
    let parsed = eqproxy_config::parse_str(&outcome.toml, Path::new("nginx.conf"))
        .expect("generated TOML parses");
    let redirect = parsed
        .listener
        .values()
        .find(|listener| listener.redirect.is_some())
        .expect("the server with only `return 301` must become a redirect listener");
    assert_eq!(redirect.listen, "0.0.0.0:80");
    assert!(redirect.backend.is_none(), "{redirect:?}");
    let redirect = redirect.redirect.as_ref().expect("redirect");
    assert_eq!(redirect.status, Some(301));
    assert_eq!(redirect.location, "https://$host$request_uri");
    assert!(
        !outcome.report.contains("créelo a mano"),
        "the redirect is no longer a manual step:\n{}",
        outcome.report
    );
    for needle in [
        "host = \"ejemplo.com\"",
        "host = \"panel.ejemplo.com\"",
        ".tls.certs]]",
        "names = [\"panel.ejemplo.com\"]",
        "use_backend = \"backend_10_0_2_5_3000\"",
    ] {
        assert!(
            outcome.toml.contains(needle),
            "the shared listener must carry {needle}:\n{}",
            outcome.toml
        );
    }
    assert!(
        !outcome.report.contains("quedan sin migrar")
            && !outcome.report.contains("varios vhosts comparten"),
        "a convertible vhost must not be reported as unmigrated:\n{}",
        outcome.report
    );
    assert!(
        outcome.toml.contains("gzip = true"),
        "gzip on must enable compression:\n{}",
        outcome.toml
    );
    // N-2: `proxy_ssl_verify off` is nginx's own default and the schema can
    // hold it, so nothing is degraded here.
    assert!(
        !outcome.report.contains("Convertido con degradación"),
        "nginx.conf loses no security property:\n{}",
        outcome.report
    );
}

#[test]
fn ingress_nginx_resolves_endpoints_and_annotations() {
    let (text, outcome) = convert("ingress-nginx.yaml", Source::Ingress);
    assert_contract("ingress-nginx.yaml", &text, &outcome);
    assert!(
        outcome.toml.contains("10.0.0.10:8080") && outcome.toml.contains("10.0.1.20:8080"),
        "Endpoints must resolve to backend servers:\n{}",
        outcome.toml
    );
    assert!(
        outcome.toml.contains("rate_limit") && outcome.toml.contains("cors"),
        "limit-rps and CORS annotations must land in the rules:\n{}",
        outcome.toml
    );
    assert!(
        outcome.report.contains("ssl-passthrough"),
        "ssl-passthrough has no equivalent and must be reported"
    );
}

#[test]
fn envoy_clusters_tls_and_routes_convert() {
    let (text, outcome) = convert("envoy.yaml", Source::Envoy);
    assert_contract("envoy.yaml", &text, &outcome);
    assert!(
        outcome.toml.contains("10.0.0.10:8080"),
        "cluster endpoints must become backend servers:\n{}",
        outcome.toml
    );
    assert!(
        outcome.toml.contains("server_name = \"api.interno\""),
        "UpstreamTlsContext.sni must become server_name:\n{}",
        outcome.toml
    );
    assert!(
        outcome.report.contains("xDS"),
        "dynamic_resources must be reported as unsupported"
    );
}

#[test]
fn detection_picks_the_right_dialect_for_each_corpus_file() {
    for (name, expected) in [
        ("haproxy.cfg", Source::Haproxy),
        ("nginx.conf", Source::Nginx),
        ("ingress-nginx.yaml", Source::Ingress),
        ("envoy.yaml", Source::Envoy),
    ] {
        let text = std::fs::read_to_string(corpus(name)).expect("read");
        assert_eq!(Source::detect(&text), expected, "{name}");
    }
}

#[test]
fn nginx_include_corpus_keeps_every_vhost_in_glob_order() {
    let root = include_corpus("");
    let path = root.join("nginx.conf");
    let text = std::fs::read_to_string(&path).expect("read include corpus");
    let options = import::ImportOptions {
        include_root: Some(&root),
    };
    let outcome = import::convert_with_options(Source::Nginx, &text, options)
        .expect("the include corpus must convert");
    let parsed =
        eqproxy_config::parse_str(&outcome.toml, &path).expect("generated include TOML must parse");
    let check = eqproxy_config::validate::check(&parsed, &outcome.toml, &path);
    assert!(
        check.is_valid(),
        "generated include configuration must validate: {:?}",
        check
            .errors
            .iter()
            .map(|err| (err.field.clone(), err.cause.clone()))
            .collect::<Vec<_>>()
    );
    for needle in [
        "host = \"alpha.includes.test\"",
        "host = \"beta.includes.test\"",
        "use_backend = \"alpha\"",
        "use_backend = \"beta\"",
    ] {
        assert!(
            outcome.toml.contains(needle),
            "the include corpus must keep {needle}:\n{}",
            outcome.toml
        );
    }
    let alpha = outcome
        .toml
        .find("host = \"alpha.includes.test\"")
        .expect("alpha vhost");
    let beta = outcome
        .toml
        .find("host = \"beta.includes.test\"")
        .expect("beta vhost");
    assert!(
        alpha < beta,
        "conf.d glob must expand 10-alpha before 20-beta:\n{}",
        outcome.toml
    );
    assert!(
        outcome
            .report
            .contains("include expandido: 2 fichero(s) leído(s) en orden lexicográfico"),
        "the include itself must remain in the report:\n{}",
        outcome.report
    );
    let again = import::convert_with_options(Source::Nginx, &text, options)
        .expect("deterministic second include pass");
    assert_eq!(
        again.toml, outcome.toml,
        "include TOML must be deterministic"
    );
    assert_eq!(
        again.report, outcome.report,
        "include report must be deterministic"
    );
}

#[test]
fn nginx_tls_divergence_corpus_names_all_three_lost_policies() {
    // One listener cannot carry per-vhost TLS versions, ciphers and mTLS
    // policy. The default_server wins and the other vhost loses those three
    // properties; the report must name every one and count exactly three.
    let path = corpus("nginx-tls-divergence/nginx.conf");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("corpus file {} must be readable: {err}", path.display()));
    let (outcome, degraded) = import::convert_with_degraded(Source::Nginx, &text)
        .expect("the TLS-divergence corpus must convert");

    let mut names = degraded.clone();
    names.sort();
    let mut expected = vec![
        "ssl_protocols TLSv1.2".to_owned(),
        "ssl_ciphers ECDHE-RSA-AES128-GCM-SHA256".to_owned(),
        "ssl_client_certificate /etc/nginx/tls/clients.pem".to_owned(),
    ];
    expected.sort();
    assert_eq!(
        names, expected,
        "exactly the version, cipher and mTLS policies must be degraded"
    );

    assert!(
        outcome.report.contains("con degradación **3**"),
        "the summary must count the three losses:\n{}",
        outcome.report
    );
    assert!(
        outcome
            .report
            .contains("### ⚠️ convertido con degradación (revisar) (3)"),
        "the degraded section must list exactly three directives:\n{}",
        outcome.report
    );
    for loss in [
        "política de versiones TLS",
        "política de cifrados",
        "certificado de cliente (mTLS)",
    ] {
        assert!(
            outcome.report.contains(loss),
            "the report must name the lost {loss}:\n{}",
            outcome.report
        );
    }

    // The generated configuration is real: it parses and validates with the
    // product's own gate, and it keeps the default_server policy rather than
    // inventing per-vhost TLS that the schema cannot represent.
    let parsed = eqproxy_config::parse_str(&outcome.toml, &path)
        .expect("generated TLS-divergence TOML must parse");
    let check = eqproxy_config::validate::check(&parsed, &outcome.toml, &path);
    assert!(
        check.is_valid(),
        "generated TLS-divergence configuration must validate: {:?}",
        check
            .errors
            .iter()
            .map(|err| (err.field.clone(), err.cause.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        outcome.toml.contains("min_version = \"1.3\"")
            && outcome.toml.contains("max_version = \"1.3\"")
            && !outcome.toml.contains("TLSv1.2")
            && !outcome.toml.contains("min_version = \"1.2\""),
        "the default_server's TLS 1.3 policy must win:\n{}",
        outcome.toml
    );
    assert!(
        outcome.toml.contains("names = [\"b.example\"]")
            && outcome.toml.contains("/etc/nginx/tls/b.example.pem"),
        "the second vhost must keep its certificate:\n{}",
        outcome.toml
    );
    assert!(
        !outcome.toml.contains("client_ca")
            && !outcome.toml.contains("ECDHE-RSA-AES128-GCM-SHA256")
            && !outcome.toml.contains("HIGH:!aNULL"),
        "the listener must not claim the lost mTLS or cipher policy:\n{}",
        outcome.toml
    );

    let walked = counter(&outcome.report, "Directivas leídas:");
    let classified = counter(&outcome.report, "clasificadas:");
    assert_eq!(
        walked, classified,
        "every walked directive must be classified in the TLS-divergence corpus\n{}",
        outcome.report
    );
    let (again, degraded_again) = import::convert_with_degraded(Source::Nginx, &text)
        .expect("deterministic second TLS-divergence pass");
    assert_eq!(again.toml, outcome.toml, "TOML must be deterministic");
    assert_eq!(again.report, outcome.report, "report must be deterministic");
    assert_eq!(
        degraded_again, degraded,
        "degraded names must be deterministic"
    );
}

#[test]
fn nginx_include_corpus_cycle_fails_with_an_actionable_chain() {
    let root = include_corpus("");
    let path = root.join("cycle/nginx.conf");
    let text = std::fs::read_to_string(&path).expect("read cycle corpus");
    let error = import::convert_with_options(
        Source::Nginx,
        &text,
        import::ImportOptions {
            include_root: Some(&root),
        },
    )
    .expect_err("the cycle corpus must be rejected");
    assert!(
        error.message.contains("ciclo detectado")
            && error.message.contains("a.conf")
            && error.message.contains("b.conf"),
        "the cycle error must name the files: {}",
        error.message
    );
}
