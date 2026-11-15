//! Reference-proxy configuration import (Phase 8, CE6/Q9/CE8).
//!
//! Converts an nginx, `HAProxy`, ingress-nginx or Envoy configuration into the
//! canonical `eqproxy.toml` and a Markdown report that classifies **every**
//! directive of the source as one of three outcomes:
//!
//! - **convertido** — the value has an equivalent field in the schema and was
//!   written to the output;
//! - **revisar** — an approximation, a cosmetic difference or a global
//!   directive: the operator must read it, nothing was invented;
//! - **rechazado** — `EQProxy` has no equivalent today (or the source value is
//!   untranslatable); the directive is named with what is lost.
//!
//! Two properties are enforced by construction rather than promised:
//!
//! 1. **No silent omission.** Each walked directive records at least one
//!    classification; [`Conv::touch`] fails the import for a directive that
//!    produced none, so a forgotten `continue` is a hard error, not a hole in
//!    the report. The report prints the counts it walked and it classified.
//! 2. **The output is a valid `EQProxy` configuration.** The generated TOML is
//!    parsed and semantically checked with this crate's own parser and
//!    validator ([`crate::parse_str`] + [`crate::validate::check`]) before it
//!    is returned: the importer can never hand back a file the product would
//!    reject. The Markdown report is honest about what the comparison is (a
//!    static reading of the source, not a traffic replay).
//!
//! Static analysis only: nothing from the source configuration is executed.

mod envoy;
mod haproxy;
mod ingress;
mod nginx;
mod report;
#[cfg(test)]
mod security;
mod yaml;

pub use report::{emit_toml, report_with_paths};

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Source configuration dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Nginx,
    Haproxy,
    Ingress,
    Envoy,
}

/// Options for [`convert_with_options`].
///
/// `include_root` enables nginx `include` expansion. Relative include paths
/// (and glob patterns) are resolved against it; absolute paths are read as
/// written. Without it, `include` remains a manual review item, exactly as in
/// [`convert`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportOptions<'a> {
    /// Base directory for relative nginx `include` paths.
    pub include_root: Option<&'a Path>,
}

impl Source {
    /// Parses a `--from` value.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "nginx" => Some(Self::Nginx),
            "haproxy" => Some(Self::Haproxy),
            "ingress" | "ingress-nginx" => Some(Self::Ingress),
            "envoy" => Some(Self::Envoy),
            _ => None,
        }
    }

    /// Stable name used in the command line and the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nginx => "nginx",
            Self::Haproxy => "haproxy",
            Self::Ingress => "ingress",
            Self::Envoy => "envoy",
        }
    }

    /// Guesses the dialect from the text. Only a guess: `--from` overrides it,
    /// and the caller reports which one was used.
    #[must_use]
    pub fn detect(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        if lower.contains("static_resources")
            || lower.contains("filter_chains")
            || lower.contains("typed_config")
        {
            return Self::Envoy;
        }
        if lower.contains("kind: ingress") || lower.contains("ingress.kubernetes.io/") {
            return Self::Ingress;
        }
        for line in text.lines() {
            let line = line.trim_start();
            for keyword in [
                "frontend ",
                "backend ",
                "defaults",
                "global",
                "default_backend",
                "default-server",
            ] {
                if line.starts_with(keyword) {
                    return Self::Haproxy;
                }
            }
        }
        Self::Nginx
    }
}

/// Severity of one classification. The outcomes of the report are
/// [`Self::Done`] (convertido), [`Self::Degraded`] (convertido, pero con menos
/// seguridad que la fuente), [`Self::Info`]/[`Self::Review`] (revisar) and
/// [`Self::Gap`]/[`Self::Rejected`] (rechazado).
///
/// `#[non_exhaustive]` because the classification is a contract that grows: a
/// future importer learns to name a new outcome and adds a variant. Readers
/// outside this crate must leave a wildcard arm; within it, matches stay
/// exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Severity {
    /// Converted into a schema field.
    Done,
    /// Walked, no translation needed (it does not affect `eqproxy.toml`).
    Info,
    /// Approximation or cosmetic difference: read it before switching.
    Review,
    /// `EQProxy` has no equivalent for this directive today.
    Gap,
    /// The source value cannot be translated as written.
    Rejected,
    /// The conversion keeps the directive's effect, but with weaker semantics
    /// than the source declared. Always carries a [`Degradation`], so a lost
    /// security property never reads as `Done`.
    ///
    /// Appended after the older variants so their discriminants and their
    /// `Ord` positions do not move (N-2).
    Degraded,
}

/// The security property a conversion kept from its source and the one it did
/// not: the mandatory reason of a [`Severity::Degraded`] classification.
///
/// The reason is a closed type rather than free text, so a degraded directive
/// cannot be filed without naming what was lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Degradation {
    /// The source authenticated itself to the origin with a client
    /// certificate; `[backend.*.tls]` has no field for one.
    ClientCertificate,
    /// The source fixed the TLS versions of the origin connection.
    ProtocolPolicy,
    /// The source fixed the cipher suites of the origin connection.
    CipherPolicy,
    /// The source bounded the length of the certificate chain it accepts.
    VerifyDepth,
    /// Two paths share one backend with different TLS identities: one of them
    /// now verifies a name its own source never declared.
    IdentityRelaxed,
}

impl std::fmt::Display for Degradation {
    /// The lost property, as the label a report row starts with.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ClientCertificate => "certificado de cliente (mTLS)",
            Self::ProtocolPolicy => "política de versiones TLS",
            Self::CipherPolicy => "política de cifrados",
            Self::VerifyDepth => "profundidad de verificación",
            Self::IdentityRelaxed => "identidad TLS compartida",
        })
    }
}

/// One classified source directive.
#[derive(Debug, Clone)]
pub struct Note {
    /// One of [`Severity`].
    pub severity: Severity,
    /// The original directive (verbatim, trimmed).
    pub directive: String,
    /// The source section it appears in (`backend bk_web`, `listener …`).
    pub context: String,
    /// What happened, in operator language.
    pub message: String,
}

/// Counts of the ternary classification printed in the report header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Directives with an equivalent field.
    pub converted: usize,
    /// Non-blocking notes: `Review` (approximation, cosmetic, global),
    /// `Info` (walked, no translation needed) **and** `Degraded` (kept with
    /// less security than the source). This is the report's «revisar» column;
    /// the header prints the `Degraded` subtotal apart from the rest.
    pub review: usize,
    /// Directives `EQProxy` cannot serve today.
    pub rejected: usize,
}

/// Result of a successful conversion.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Generated `eqproxy.toml`, already parsed and validated.
    pub toml: String,
    /// Markdown report classifying every source directive.
    pub report: String,
    /// Ternary counts.
    pub counts: Counts,
}

/// Import failure. The message names the cause; nothing is written on error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    /// Operator-facing cause.
    pub message: String,
}

impl ImportError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ImportError {}

// ---------------------------------------------------------------------------
// Intermediate representation (mirrors the v1 schema the emitter writes)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Server {
    addr: String,
    weight: u32,
    backup: bool,
    /// `HAProxy` `server … maxconn` (B-H16); `0` = no per-server cap.
    maxconn: u32,
}

#[derive(Debug, Clone, Default)]
struct HealthCheck {
    interval: String,
    timeout: String,
    fall: u32,
    rise: u32,
    /// `Some("http")` for an HTTP check (B-H6).
    kind: Option<&'static str>,
    http_path: Option<String>,
    expect_status: Option<String>,
    http_host: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct BackendTls {
    verify: bool,
    ca_file: Option<String>,
    server_name: Option<String>,
    alpn: Vec<String>,
    min_version: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
struct Affinity {
    mode: String,
    cookie: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Backend {
    servers: Vec<Server>,
    /// Empty = the schema default (`roundrobin`).
    balance: String,
    proxy_protocol_v1: bool,
    health_check: Option<HealthCheck>,
    tls: Option<BackendTls>,
    affinity: Option<Affinity>,
    retries: u32,
    maxqueue: u32,
    slow_start: Option<String>,
    /// Source timeouts (`client`, `server`, `connect`, `check`, …).
    timeouts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
struct ListenerTls {
    cert: String,
    key: String,
    alpn: Vec<String>,
    min_version: Option<&'static str>,
    /// Highest accepted version (B-N16/B-I16); `None` = TLS 1.3.
    max_version: Option<&'static str>,
    client_ca: Option<String>,
    /// Additional certificates selected by SNI, one per vhost
    /// (`[listener.*.tls].certs[]`).
    certs: Vec<SniCertIr>,
}

/// One `[[listener.*.tls.certs]]` entry: a certificate plus the SNI names
/// (exact or `*.domain`) from the vhost that declared it.
#[derive(Debug, Clone, Default)]
struct SniCertIr {
    cert: String,
    key: String,
    names: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct HeaderOp {
    name: String,
    value: String,
}

/// One request-header condition in the import IR. The fields mirror
/// `eqproxy_core::rules::HeaderSpec`: exactly one of `exact`, `regex` or
/// `present` is set.
#[derive(Debug, Clone, Default)]
struct HeaderCondition {
    name: String,
    exact: Option<String>,
    regex: Option<String>,
    present: bool,
}

#[derive(Debug, Clone, Default)]
struct Rule {
    name: String,
    use_backend: Option<String>,
    path_prefix: Option<String>,
    path: Option<String>,
    host: Option<String>,
    host_prefix: Option<String>,
    host_regex: Option<String>,
    /// nginx `server_name` precedence (exact, leading wildcard, trailing
    /// wildcard, regex); not emitted. Other converters leave the default.
    host_rank: HostRank,
    headers: Vec<HeaderCondition>,
    methods: Vec<String>,
    ip: Vec<String>,
    set_headers: Vec<HeaderOp>,
    add_headers: Vec<HeaderOp>,
    response_headers: Vec<HeaderOp>,
    direct_response: Option<(u16, Option<String>)>,
    redirect: Option<(Option<u16>, String)>,
    /// `CorsConfig.allow_origin` (ingress-nginx `enable-cors`).
    cors_origin: Option<String>,
    /// `RateLimitConfig`: (rate, per, burst).
    rate_limit: Option<(u32, String, Option<u32>)>,
    /// `RewriteConfig.path_prefix_rewrite`.
    rewrite_prefix: Option<String>,
}

/// Matching precedence of a `server_name` pattern. `Reverse` makes the
/// longest wildcard sort first, as nginx does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum HostRank {
    Exact,
    LeadingWildcard(std::cmp::Reverse<usize>),
    TrailingWildcard(std::cmp::Reverse<usize>),
    Regex(usize),
    #[default]
    CatchAll,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ErrorPageIr {
    codes: Vec<u16>,
    file: Option<String>,
    body: Option<String>,
    /// `HAProxy` `errorloc`: a 3xx redirect instead of a body.
    location: Option<String>,
    raw: bool,
    status: Option<u16>,
    intercept: bool,
}

// The IR mirrors the TOML surface one field per key; grouping the switches
// would break that correspondence.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
struct Listener {
    listen: String,
    mode: String,
    backend: Option<String>,
    /// Default 3xx for a redirect-only listener (nginx server-level
    /// `return`): status and `Location` template.
    redirect: Option<(Option<u16>, String)>,
    maxconn: u32,
    connect_timeout: Option<String>,
    idle_timeout: Option<String>,
    client_header_timeout: Option<String>,
    server_header_timeout: Option<String>,
    tls: Option<ListenerTls>,
    quic: bool,
    forwarded_headers: Option<&'static str>,
    compression_gzip: bool,
    compression_level: Option<u32>,
    compression_min_size: Option<u64>,
    max_body_bytes: Option<u64>,
    cache: bool,
    proxy_receive: bool,
    rules: Vec<Rule>,
    error_pages: Vec<ErrorPageIr>,
}

impl Listener {
    fn new(listen: impl Into<String>, mode: &str, backend: impl Into<String>) -> Self {
        Self {
            listen: listen.into(),
            mode: mode.to_owned(),
            backend: Some(backend.into()),
            // The schema default; the emitter omits it while it stays 1024.
            maxconn: 1024,
            ..Self::default()
        }
    }

    /// A listener whose fallback is a local redirect, so it needs no
    /// backend at all (nginx `server { return 301 …; }`).
    fn redirect(
        listen: impl Into<String>,
        mode: &str,
        status: Option<u16>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            listen: listen.into(),
            mode: mode.to_owned(),
            backend: None,
            redirect: Some((status, location.into())),
            // The schema default; the emitter omits it while it stays 1024.
            maxconn: 1024,
            ..Self::default()
        }
    }
}

/// Accumulator shared by the four converters: the IR being built plus the
/// classification log the report is rendered from.
#[derive(Debug, Default)]
pub struct Conv {
    listeners: BTreeMap<String, Listener>,
    backends: BTreeMap<String, Backend>,
    notes: Vec<Note>,
    /// Source units walked (directives, lines, nodes). Compared with the
    /// number of units that produced a classification.
    walked: usize,
}

impl Conv {
    fn new() -> Self {
        Self::default()
    }

    /// Records one classification.
    fn note(
        &mut self,
        severity: Severity,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.notes.push(Note {
            severity,
            directive: directive.into(),
            context: context.to_owned(),
            message: message.into(),
        });
    }

    /// Counts one walked source unit and records its classification.
    ///
    /// `units` is 1 for a directive and >1 when one source line expands to
    /// several dispositions (`server … check weight 5`).
    fn done(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.walked += units;
        self.note(Severity::Done, directive, context, message);
    }

    fn info(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.walked += units;
        self.note(Severity::Info, directive, context, message);
    }

    fn review(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.walked += units;
        self.note(Severity::Review, directive, context, message);
    }

    /// Records a conversion that is weaker than its source.
    ///
    /// `lost` names the security property that did not survive, so the note
    /// always carries a reason; `detail` says which source values were lost.
    fn degraded(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        lost: Degradation,
        detail: impl Into<String>,
    ) {
        self.walked += units;
        let detail = detail.into();
        self.note(
            Severity::Degraded,
            directive,
            context,
            format!("{lost}: {detail}"),
        );
    }

    /// Directives emitted with less security than their source declared.
    fn degraded_count(&self) -> usize {
        self.notes
            .iter()
            .filter(|note| note.severity == Severity::Degraded)
            .count()
    }

    fn gap(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.walked += units;
        self.note(Severity::Gap, directive, context, message);
    }

    fn reject(
        &mut self,
        units: usize,
        directive: impl Into<String>,
        context: &str,
        message: impl Into<String>,
    ) {
        self.walked += units;
        self.note(Severity::Rejected, directive, context, message);
    }

    /// Fails when a walked directive produced no classification.
    ///
    /// `before` is `self.notes.len()` taken before the directive was handled;
    /// a handler that returns early without a note is a silent omission, which
    /// this turns into an error naming the directive.
    fn touch(&self, directive: &str, context: &str, before: usize) -> Result<(), ImportError> {
        if self.notes.len() == before {
            return Err(ImportError::new(format!(
                "{context}: directive «{directive}» was walked but not classified — the importer never drops a directive silently"
            )));
        }
        Ok(())
    }

    fn classify(&self) -> Counts {
        let mut counts = Counts::default();
        for note in &self.notes {
            match note.severity {
                Severity::Done => counts.converted += 1,
                Severity::Info | Severity::Review | Severity::Degraded => counts.review += 1,
                Severity::Gap | Severity::Rejected => counts.rejected += 1,
            }
        }
        counts
    }

    fn free_backend_name(&self, base: &str) -> String {
        unique_name(base, self.backends.keys().map(String::as_str))
    }

    fn free_listener_name(&self, base: &str) -> String {
        unique_name(base, self.listeners.keys().map(String::as_str))
    }

    fn has_backend(&self, name: &str) -> bool {
        self.backends.contains_key(name)
    }

    /// Number of listeners generated so far.
    fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// The only backend defined so far, when there is exactly one.
    fn only_backend(&self) -> Option<String> {
        if self.backends.len() == 1 {
            self.backends.keys().next().cloned()
        } else {
            None
        }
    }

    fn add_backend(&mut self, name: String, backend: Backend) {
        self.backends.insert(name, backend);
    }

    /// Mutable access to a backend the importer already created, so a later
    /// directive (`proxy_ssl_*`, EXT-H-04) can complete what `proxy_pass`
    /// started without building a second copy of the backend.
    fn backend_mut(&mut self, name: &str) -> Option<&mut Backend> {
        self.backends.get_mut(name)
    }

    fn add_listener(&mut self, name: String, listener: Listener) {
        self.listeners.insert(name, listener);
    }

    /// Default health check (`interval 2s`, `timeout 1s`, `fall 3`, `rise 2`).
    fn default_health_check() -> HealthCheck {
        HealthCheck {
            interval: "2s".to_owned(),
            timeout: "1s".to_owned(),
            fall: 3,
            rise: 2,
            ..HealthCheck::default()
        }
    }
}

/// `base` sanitized, with `_2`, `_3`… appended when the name is taken.
fn unique_name<'a>(base: &str, taken: impl Iterator<Item = &'a str>) -> String {
    let base = {
        let cleaned = sanitize_name(base);
        if cleaned.is_empty() {
            "sin_nombre".to_owned()
        } else {
            cleaned
        }
    };
    let used: Vec<&str> = taken.collect();
    if !used.contains(&base.as_str()) {
        return base;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}_{index}");
        if !used.contains(&candidate.as_str()) {
            return candidate;
        }
        index += 1;
    }
}

/// Restricts a name to `A-Za-z0-9_-` (TOML bare-key and section names).
#[must_use]
pub fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

/// Converts a reference duration to the `eqproxy.toml` spelling
/// (`<integer><ms|s|m|h>`).
///
/// nginx: a bare number is seconds. `HAProxy`: a bare number is milliseconds.
/// Anything else (`10m30s`, `1d`) is summed when every part is recognised and
/// reviewed with a note reading it, never guessed.
fn convert_duration(
    conv: &mut Conv,
    raw: &str,
    default_unit: &str,
    directive: &str,
    context: &str,
) -> Option<String> {
    let raw = raw.trim();
    let mut total_ms: u64 = 0;
    let mut rest = raw;
    let mut parts = 0usize;
    while !rest.is_empty() {
        let digits: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if digits.is_empty() || digits == "." {
            break;
        }
        let after = &rest[digits.len()..];
        let (unit, consumed) = if let Some(tail) = after.strip_prefix("ms") {
            ("ms", after.len() - tail.len())
        } else if let Some(tail) = after.strip_prefix('s') {
            ("s", after.len() - tail.len())
        } else if let Some(tail) = after.strip_prefix('m') {
            ("m", after.len() - tail.len())
        } else if let Some(tail) = after.strip_prefix('h') {
            ("h", after.len() - tail.len())
        } else if let Some(tail) = after.strip_prefix('d') {
            ("d", after.len() - tail.len())
        } else {
            (default_unit, 0)
        };
        let Ok(value) = digits.parse::<f64>() else {
            break;
        };
        let factor: u32 = match unit {
            "s" => 1_000,
            "m" => 60_000,
            "h" => 3_600_000,
            "d" => 86_400_000,
            _ => 1,
        };
        // `value` is a non-negative figure parsed from the directive and the
        // caller range-checks the total; truncating to whole milliseconds is
        // the intended rounding.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            total_ms += (value * f64::from(factor)) as u64;
        }
        rest = &after[consumed..];
        parts += 1;
    }
    if parts == 0 || !rest.is_empty() || total_ms == 0 {
        conv.review(
            1,
            directive,
            context,
            format!("duración no reconocida «{raw}»: se usa el valor por defecto de EQProxy"),
        );
        return None;
    }
    if total_ms.is_multiple_of(3_600_000) {
        Some(format!("{}h", total_ms / 3_600_000))
    } else if total_ms.is_multiple_of(60_000) {
        Some(format!("{}m", total_ms / 60_000))
    } else if total_ms.is_multiple_of(1_000) {
        Some(format!("{}s", total_ms / 1_000))
    } else {
        Some(format!("{total_ms}ms"))
    }
}

/// Parses a size with an optional unit suffix (`8m`, `1k`, `1024`).
///
/// nginx and ingress-nginx spell sizes with `k`/`m`/`g`; anything else is
/// `None` and becomes a classified review note, never a guess.
#[must_use]
pub fn parse_size(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (digits, factor) = match raw.chars().last()? {
        'k' | 'K' => (&raw[..raw.len() - 1], 1_024u64),
        'm' | 'M' => (&raw[..raw.len() - 1], 1_024 * 1_024),
        'g' | 'G' => (&raw[..raw.len() - 1], 1_024 * 1_024 * 1_024),
        _ => (raw, 1),
    };
    let value: u64 = digits.trim().parse().ok()?;
    value.checked_mul(factor)
}

/// True when `value` is `IP:PORT` or `HOSTNAME:PORT` — what the schema
/// accepts (`parse_server_target`).
#[must_use]
pub fn is_addr(value: &str) -> bool {
    crate::schema::parse_server_target(value).is_ok()
}

/// Normalises a bind/server address to `IP:PORT`, `HOSTNAME:PORT` or
/// `unix:/absolute/path` (ADR-0043).
///
/// `EQProxy` accepts literal IPs and hostnames (resolved in the background,
/// B-H13) and, as a **backend** target, a filesystem Unix socket. A listener
/// still cannot be a Unix socket (`allow_unix = false`), and `fd@` has no
/// equivalent: both are rejected with their cause. A missing port is filled
/// from `default_port` when the source dialect allows it.
fn normalize_addr(
    conv: &mut Conv,
    raw: &str,
    default_port: Option<u16>,
    directive: &str,
    context: &str,
    allow_unix: bool,
) -> Option<String> {
    let mut addr = raw.trim().trim_matches(['"', '\'']).to_owned();
    // `unix:/path` (nginx), `unix@/path` and a bare absolute path (HAProxy)
    // all name the same local socket. The normalised spelling is the one the
    // schema accepts.
    if let Some(path) = addr
        .strip_prefix("unix:")
        .or_else(|| addr.strip_prefix("unix@"))
    {
        return unix_target(conv, raw, path, directive, context, allow_unix);
    }
    if addr.starts_with('/') {
        return unix_target(conv, raw, &addr, directive, context, allow_unix);
    }
    if addr.starts_with("fd@") {
        conv.reject(
            1,
            directive,
            context,
            format!("«{addr}»: descriptores heredados (fd@) sin soporte en EQProxy"),
        );
        return None;
    }
    if let Some(port) = addr.strip_prefix("*:") {
        addr = format!("0.0.0.0:{port}");
    } else if let Some(port) = addr.strip_prefix(':') {
        addr = format!("0.0.0.0:{port}");
    }
    if !addr.contains(':') {
        if addr.chars().all(|c| c.is_ascii_digit()) {
            addr = format!("0.0.0.0:{addr}");
        } else if let Some(port) = default_port {
            addr = format!("{addr}:{port}");
        } else {
            conv.reject(
                1,
                directive,
                context,
                format!("«{addr}»: falta el puerto y no hay uno por defecto en este contexto"),
            );
            return None;
        }
    }
    // A bare `HOST:` gains the default port; a bracketed IPv6 keeps its shape.
    if addr.ends_with(':')
        && let Some(port) = default_port
    {
        addr = format!("{addr}{port}");
    }
    if is_addr(&addr) {
        return Some(addr);
    }
    conv.reject(
        1,
        directive,
        context,
        format!("«{raw}»: no es IP:PORT ni HOSTNAME:PORT válido para EQProxy"),
    );
    None
}

/// Accepts a Unix socket path as a backend target, or rejects it with its
/// cause (ADR-0043).
fn unix_target(
    conv: &mut Conv,
    raw: &str,
    path: &str,
    directive: &str,
    context: &str,
    allow_unix: bool,
) -> Option<String> {
    if !allow_unix {
        conv.reject(
            1,
            directive,
            context,
            format!(
                "«{raw}»: EQProxy no escucha en sockets Unix (solo IP:PORT o HOSTNAME:PORT); un backend sí puede ser unix:/ruta"
            ),
        );
        return None;
    }
    let target = format!("unix:{path}");
    if crate::schema::parse_server_target(&target).is_ok() {
        return Some(target);
    }
    conv.reject(
        1,
        directive,
        context,
        format!("«{raw}»: ruta de socket Unix inválida (debe ser absoluta y medir ≤100 bytes)"),
    );
    None
}

/// Escapes a TOML basic string.
fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_key(name: &str) -> String {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        name.to_owned()
    } else {
        toml_string(name)
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Converts `text` (a configuration in dialect `source`) into `eqproxy.toml`
/// plus its Markdown report.
///
/// Fails when the source cannot be read as that dialect, when it yields no
/// listener, or when the generated document does not pass this crate's own
/// parser and validator. Nothing partial is ever returned.
///
///# Errors
///
///Returns `ImportError` naming the directive and the reason when the source
///cannot be read or converted.
pub fn convert(source: Source, text: &str) -> Result<Outcome, ImportError> {
    convert_with_degraded(source, text).map(|(outcome, _)| outcome)
}

/// Like [`convert`], with explicit options (currently nginx `include`
/// expansion).
///
///# Errors
///
///Returns `ImportError` naming the directive and the reason when the source
///cannot be read or converted; the options are applied as documented.
pub fn convert_with_options(
    source: Source,
    text: &str,
    options: ImportOptions<'_>,
) -> Result<Outcome, ImportError> {
    convert_with_options_and_degraded(source, text, options).map(|(outcome, _)| outcome)
}

/// Like [`convert`], plus the directives classified as degraded.
///
/// The names come from the same notes the report is rendered from, so a caller
/// that summarizes the conversion never has to parse its Markdown or depend on
/// a report section. They are one line each, in report order. The report's
/// summary count is checked against this list before the conversion returns.
///
///# Errors
///
///Returns `ImportError` naming the directive and the reason when the source
///cannot be read or converted.
pub fn convert_with_degraded(
    source: Source,
    text: &str,
) -> Result<(Outcome, Vec<String>), ImportError> {
    convert_with_options_and_degraded(source, text, ImportOptions::default())
}

/// Like [`convert_with_degraded`], with explicit options.
///
/// `include_root` is only meaningful for nginx; passing it for another
/// dialect is an error rather than silently reading files it would never use.
///
///# Errors
///
///Returns `ImportError` naming the directive and the reason when the source
///cannot be read or converted.
pub fn convert_with_options_and_degraded(
    source: Source,
    text: &str,
    options: ImportOptions<'_>,
) -> Result<(Outcome, Vec<String>), ImportError> {
    if text.len() > crate::MAX_CONFIG_BYTES {
        return Err(ImportError::new(format!(
            "source configuration too large: {} bytes (maximum {})",
            text.len(),
            crate::MAX_CONFIG_BYTES
        )));
    }
    if options.include_root.is_some() && source != Source::Nginx {
        return Err(ImportError::new(
            "--include-root only applies to nginx configurations".to_owned(),
        ));
    }
    let mut conv = Conv::new();
    match source {
        Source::Nginx => nginx::convert(&mut conv, text, options.include_root)?,
        Source::Haproxy => haproxy::convert(&mut conv, text)?,
        Source::Ingress => ingress::convert(&mut conv, text)?,
        Source::Envoy => envoy::convert(&mut conv, text)?,
    }
    if conv.listeners.is_empty() {
        return Err(ImportError::new(
            "the source configuration produced no listener: nothing to write. Check that the file declares a proxy_section/listen (nginx), frontend or listen (HAProxy), an Ingress rule or an Envoy listener with a route"
                .to_owned(),
        ));
    }
    let toml = emit_toml(&conv);
    let file = Path::new("<import>");
    let config = crate::parse_str(&toml, file).map_err(|err| {
        ImportError::new(format!(
            "internal: the generated configuration does not parse ({}) — please report it",
            err.cause
        ))
    })?;
    let check = crate::validate::check(&config, &toml, file);
    if !check.is_valid() {
        let details: Vec<String> = check
            .errors
            .iter()
            .map(|err| {
                format!(
                    "{}: {}",
                    err.field.clone().unwrap_or_else(|| "-".to_owned()),
                    err.cause
                )
            })
            .collect();
        return Err(ImportError::new(format!(
            "internal: the generated configuration does not validate: {}",
            details.join("; ")
        )));
    }
    let counts = conv.classify();
    let degraded = report::degraded_directives(&conv);
    let report = report::render(source, &conv, counts);
    report::check_degraded_names(&report, &degraded).map_err(ImportError::new)?;
    Ok((
        Outcome {
            toml,
            report,
            counts,
        },
        degraded,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_follow_each_dialect_default() {
        let mut conv = Conv::new();
        assert_eq!(
            convert_duration(&mut conv, "4", "ms", "timeout", "x"),
            Some("4ms".to_owned())
        );
        assert_eq!(
            convert_duration(&mut conv, "30s", "ms", "timeout", "x"),
            Some("30s".to_owned())
        );
        assert_eq!(
            convert_duration(&mut conv, "10", "s", "timeout", "x"),
            Some("10s".to_owned())
        );
        assert_eq!(
            convert_duration(&mut conv, "10m", "ms", "timeout", "x"),
            Some("10m".to_owned())
        );
        assert_eq!(
            convert_duration(&mut conv, "1h", "ms", "timeout", "x"),
            Some("1h".to_owned())
        );
        assert_eq!(
            convert_duration(&mut conv, "1500ms", "ms", "timeout", "x"),
            Some("1500ms".to_owned())
        );
        assert!(conv.notes.is_empty(), "recognised durations are not noted");
    }

    #[test]
    fn a_compound_duration_is_summed_and_nonsense_is_reviewed() {
        let mut conv = Conv::new();
        assert_eq!(
            convert_duration(&mut conv, "10m30s", "ms", "timeout", "x"),
            Some("630s".to_owned()),
            "both parts are recognised: the sum is exact"
        );
        assert!(conv.notes.is_empty());
        assert_eq!(
            convert_duration(&mut conv, "soon", "ms", "timeout", "x"),
            None
        );
        assert_eq!(conv.notes.len(), 1);
        assert!(matches!(conv.notes[0].severity, Severity::Review));
        assert!(conv.notes[0].message.contains("no reconocida"));
    }

    #[test]
    fn addresses_accept_ips_hostnames_and_backend_unix_sockets() {
        let mut conv = Conv::new();
        assert_eq!(
            normalize_addr(&mut conv, "10.0.0.1:80", None, "bind", "x", false).as_deref(),
            Some("10.0.0.1:80")
        );
        assert_eq!(
            normalize_addr(&mut conv, ":8080", None, "bind", "x", false).as_deref(),
            Some("0.0.0.0:8080")
        );
        assert_eq!(
            normalize_addr(&mut conv, "app.local:8080", None, "server", "x", true).as_deref(),
            Some("app.local:8080"),
            "B-H13: hostnames are accepted and resolved in the background"
        );
        assert_eq!(
            normalize_addr(&mut conv, "example.com", Some(443), "server", "x", true).as_deref(),
            Some("example.com:443")
        );
        // ADR-0043: both reference spellings name the same backend socket,
        // and the schema form is what the conversion emits.
        assert_eq!(
            normalize_addr(&mut conv, "unix@/run/app.sock", None, "server", "x", true).as_deref(),
            Some("unix:/run/app.sock")
        );
        assert_eq!(
            normalize_addr(&mut conv, "/run/app.sock", None, "server", "x", true).as_deref(),
            Some("unix:/run/app.sock"),
            "HAProxy's bare absolute path is the same target"
        );
        assert_eq!(
            normalize_addr(&mut conv, "unix:/run/app.sock", None, "bind", "x", false),
            None,
            "a listener still cannot be a Unix socket"
        );
        assert_eq!(
            normalize_addr(&mut conv, "unix:relative.sock", None, "server", "x", true),
            None,
            "the path must be absolute"
        );
        assert_eq!(
            normalize_addr(&mut conv, "fd@12", None, "server", "x", true),
            None,
            "inherited descriptors have no equivalent"
        );
        assert_eq!(
            normalize_addr(&mut conv, "[::1]:8443", None, "bind", "x", false).as_deref(),
            Some("[::1]:8443")
        );
        assert!(conv.notes.iter().any(|n| n.severity == Severity::Rejected));
    }

    #[test]
    fn unix_backends_convert_in_both_reference_dialects() {
        // ADR-0043: the conversion emits the schema spelling and the result
        // parses and validates (`Outcome.toml` is already checked).
        let nginx = "http {\n  upstream app {\n    server unix:/run/app.sock;\n  }\n  server {\n    listen 80;\n    location / { proxy_pass http://app; }\n  }\n}\n";
        let out = convert(Source::Nginx, nginx).expect("nginx upstream unix");
        assert!(out.toml.contains("\"unix:/run/app.sock\""), "{}", out.toml);
        assert!(
            !out.report.contains("Rejected"),
            "a supported backend is not a rejection: {}",
            out.report
        );

        let inline = "http {\n  server {\n    listen 80;\n    location / { proxy_pass http://unix:/run/app.sock; }\n  }\n}\n";
        let out = convert(Source::Nginx, inline).expect("nginx inline unix");
        assert!(out.toml.contains("\"unix:/run/app.sock\""), "{}", out.toml);

        let haproxy = "global\n    maxconn 100\nbackend app\n    server local unix@/run/app.sock\nfrontend fe\n    bind :80\n    default_backend app\n";
        let out = convert(Source::Haproxy, haproxy).expect("haproxy unix@");
        assert!(
            out.toml.contains("\"unix:/run/app.sock\""),
            "the `unix@` spelling normalises to the schema form: {}",
            out.toml
        );
    }

    #[test]
    fn every_walked_directive_must_be_classified() {
        let mut conv = Conv::new();
        let before = conv.notes.len();
        conv.info(1, "worker_processes", "global", "sin traducción");
        conv.touch("worker_processes", "global", before)
            .expect("classified");

        let before = conv.notes.len();
        conv.touch("forgotten", "http", before)
            .expect_err("an unclassified directive must fail the import");
    }

    #[test]
    fn the_classifier_counts_every_severity_and_degraded_stays_in_review() {
        let mut conv = Conv::new();
        conv.done(1, "a", "x", "convertido");
        conv.info(1, "b", "x", "informativo");
        conv.review(1, "c", "x", "revisar");
        conv.degraded(
            1,
            "proxy_ssl_protocols TLSv1.3",
            "x",
            Degradation::ProtocolPolicy,
            "sin min_version en el TOML",
        );
        conv.gap(1, "e", "x", "sin equivalente");
        conv.reject(1, "f", "x", "no traducible");

        let counts = conv.classify();
        assert_eq!(counts.converted, 1, "only `Done` is a conversion");
        assert_eq!(
            counts.review, 3,
            "`Degraded` belongs to the «revisar» column with `Info`/`Review`"
        );
        assert_eq!(counts.rejected, 2, "`Gap` and `Rejected` are refusals");
        assert_eq!(conv.walked, 6);
        assert_eq!(
            conv.notes.len(),
            conv.walked,
            "every walked directive produced one classification"
        );
        assert_eq!(conv.degraded_count(), 1);

        let degraded = conv
            .notes
            .iter()
            .find(|note| note.severity == Severity::Degraded)
            .expect("the degraded note is kept");
        assert!(
            degraded.message.contains("TLS") && degraded.message.contains("sin min_version"),
            "a degraded note names the lost property and the detail: {}",
            degraded.message
        );
    }

    #[test]
    fn name_sanitising_and_uniqueness_are_deterministic() {
        assert_eq!(sanitize_name("fe_https.1"), "fe_https_1");
        assert_eq!(sanitize_name("__x__"), "x");
        assert_eq!(unique_name("api", ["api"].into_iter()), "api_2");
        assert_eq!(unique_name("api", ["api", "api_2"].into_iter()), "api_3");
        assert_eq!(unique_name("", [].into_iter()), "sin_nombre");
    }

    #[test]
    fn dialect_detection_reads_the_shape_of_the_file() {
        assert_eq!(
            Source::detect("static_resources:\n  clusters: []\n"),
            Source::Envoy
        );
        assert_eq!(Source::detect("kind: Ingress\n"), Source::Ingress);
        assert_eq!(Source::detect("global\n    maxconn 9\n"), Source::Haproxy);
        assert_eq!(Source::detect("http {\n server { }\n}\n"), Source::Nginx);
        assert_eq!(Source::parse("ingress-nginx"), Some(Source::Ingress));
        assert_eq!(Source::parse("apache"), None);
    }
}
