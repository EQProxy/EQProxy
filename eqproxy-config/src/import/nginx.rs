//! nginx → `EQProxy`.
//!
//! Parses the `directive args;` / `block { … }` grammar (comments, quoting and
//! nesting included) and maps what the v1 schema can express today: listeners,
//! upstreams, TLS (including mTLS and SNI-less certificates), timeouts,
//! `client_max_body_size`, gzip, path-based routing with
//! `[[listener.*.rules]]` (nginx `location`), request/response header edits,
//! `deny` and redirects. Everything else is classified, never dropped.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use eqproxy_core::expectation::RegexMatcher;
use eqproxy_core::rules::validate_location;

use super::{
    Conv, Degradation, HeaderOp, HostRank, ImportError, Listener, ListenerTls, Rule, Severity,
    SniCertIr, convert_duration, normalize_addr, parse_size, sanitize_name,
};

/// Global directives that do not affect `eqproxy.toml`.
const GLOBAL_INFO: &[&str] = &[
    "worker_processes",
    "worker_rlimit_nofile",
    "worker_shutdown_timeout",
    "pid",
    "user",
    "load_module",
    "working_directory",
    "thread_pool",
    "timer_resolution",
    "env",
];

/// Directives `EQProxy` **does** support in a field the converter does not fill
/// automatically: the operator gets the exact place to configure it.
fn supported_manually(name: &str) -> Option<&'static str> {
    Some(match name {
        "limit_req" | "limit_req_zone" => "[[listener.*.rules.rate_limit]] (rate/per/burst)",
        "limit_conn" | "limit_conn_zone" => "listener.maxconn (límite global, no por cliente)",
        "access_log" | "log_format" => "[access_log] (format = \"json\" | \"combined\")",
        "error_log" => "logs del proceso (fuera de eqproxy.toml)",
        "resolver" => "[backend.*].resolve_interval (re-resolución en caliente)",
        "proxy_cache" | "proxy_cache_path" | "proxy_cache_valid" | "proxy_cache_bypass" => {
            "[listener.*.cache] (default_ttl, max_memory_bytes, disk_path)"
        }
        "map" | "geo" | "split_clients" => "[maps.*] + [[listener.*.rules]].map",
        "limit_rate" | "limit_rate_after" => "no implementado (limitado a la CLI)",
        "auth_request" => "[[listener.*.rules.auth]].url (subpetición de autenticación)",
        "satisfy" => "combinación de auth.url + auth.users",
        "proxy_next_upstream" | "proxy_next_upstream_tries" => "[backend.*].retries",
        "proxy_intercept_errors" | "error_page" => {
            "[[listener.*.error_pages]] (el conversor solo traduce los declarados en `server`)"
        }
        "real_ip_header" | "real_ip_recursive" | "set_real_ip_from" => {
            "[listener.*].proxy_receive + proxy_allow (PROXY v1 desde orígenes de confianza)"
        }
        "http2" => "[listener.*.tls].alpn (\"h2\")",
        "http3" | "quic" => "[listener.*].quic = true (HTTP/3 sobre QUIC)",
        "ssl_stapling" | "ssl_stapling_verify" => "OCSP stapling: fuera por decisión (ADR-0035)",
        "ssl_session_cache" | "ssl_session_timeout" | "ssl_session_tickets" => {
            "gestión de sesiones TLS: sin mando (la fija el proveedor criptográfico)"
        }
        "ssl_ciphers" | "ssl_prefer_server_ciphers" => {
            "selección de cipher suites: fuera por decisión (ADR-0035)"
        }
        "http3_hq" | "quic_retry" | "quic_gso" => "[listener.*].quic",
        _ => return None,
    })
}

/// Directives with no equivalent in `EQProxy` today.
fn unsupported(name: &str) -> Option<&'static str> {
    Some(match name {
        "root" | "alias" | "index" | "autoindex" | "try_files" | "types" | "default_type"
        | "charset" | "etag" | "expires" | "open_file_cache" => {
            "EQProxy no sirve ficheros estáticos: hace falta un backend de origen"
        }
        "rewrite" => {
            "reescritura de URI arbitraria: solo [[listener.*.rules]].rewrite.path_prefix_rewrite"
        }
        "if" => "condicionales: [[listener.*.rules]] cubre host/path/method/header/ip/cookie",
        "set" => "variables: sin motor de expresiones",
        "fastcgi_pass" | "uwsgi_pass" | "scgi_pass" | "memcached_pass" => {
            "protocolo sin soporte: EQProxy proxifica HTTP y TCP/gRPC"
        }
        "proxy_method" => "método fijo al backend: sin soporte",
        "proxy_http_version" => {
            "la versión al backend la decide el ALPN de [backend.*.tls] (h2 o HTTP/1.1)"
        }
        "proxy_buffering"
        | "proxy_buffers"
        | "client_body_buffer_size"
        | "large_client_header_buffers" => {
            "buffers: [listener.*.limits] (max_head_bytes, stream_buffer_bytes)"
        }
        "sendfile" | "tcp_nopush" | "tcp_nodelay" => "rendimiento interno, sin mando expuesto",
        "server_tokens" => "cosmético: EQProxy no añade cabecera Server propia",
        "apk_cache" | "spdy" => "obsoleto en nginx y sin equivalente",
        "allow" => "orden allow/deny: use [[listener.*.rules]] con ip + direct_response",
        "underscores_in_headers" | "ignore_invalid_headers" => {
            "framing estricto: EQProxy no relaja las cabeceras"
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Word(String),
    Semi,
    Open,
    Close,
}

#[derive(Debug)]
struct Node {
    name: String,
    args: Vec<String>,
    children: Vec<Node>,
}

fn tokenize(text: &str) -> Result<Vec<Tok>, ImportError> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ';' => {
                tokens.push(Tok::Semi);
                i += 1;
            }
            '{' => {
                tokens.push(Tok::Open);
                i += 1;
            }
            '}' => {
                tokens.push(Tok::Close);
                i += 1;
            }
            quote @ ('"' | '\'') => {
                let start = i;
                i += 1;
                let mut word = String::new();
                loop {
                    if i >= chars.len() {
                        return Err(ImportError::new(format!(
                            "cadena sin cerrar (posición {start}): falta la comilla {quote}"
                        )));
                    }
                    let c = chars[i];
                    if c == '\\' && i + 1 < chars.len() {
                        // nginx keeps the escaped byte as written.
                        word.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    if c == quote {
                        i += 1;
                        break;
                    }
                    word.push(c);
                    i += 1;
                }
                tokens.push(Tok::Word(word));
            }
            _ => {
                let mut word = String::new();
                while i < chars.len() {
                    let c = chars[i];
                    if c.is_whitespace() || matches!(c, ';' | '{' | '}' | '#' | '"' | '\'') {
                        break;
                    }
                    word.push(c);
                    i += 1;
                }
                tokens.push(Tok::Word(word));
            }
        }
    }
    Ok(tokens)
}

fn parse(tokens: &[Tok]) -> Result<Vec<Node>, ImportError> {
    let mut pos = 0;
    let nodes = parse_block(tokens, &mut pos, false)?;
    if pos != tokens.len() {
        return Err(ImportError::new(
            "sintaxis nginx: tokens sobrantes tras el último bloque".to_owned(),
        ));
    }
    Ok(nodes)
}

fn parse_block(tokens: &[Tok], pos: &mut usize, nested: bool) -> Result<Vec<Node>, ImportError> {
    let mut nodes = Vec::new();
    loop {
        match tokens.get(*pos) {
            None => {
                if nested {
                    return Err(ImportError::new(
                        "sintaxis nginx: bloque sin cerrar (falta «}»)".to_owned(),
                    ));
                }
                return Ok(nodes);
            }
            Some(Tok::Close) => {
                if nested {
                    *pos += 1;
                    return Ok(nodes);
                }
                return Err(ImportError::new(
                    "sintaxis nginx: «}» sin bloque abierto".to_owned(),
                ));
            }
            Some(Tok::Semi) => {
                return Err(ImportError::new(
                    "sintaxis nginx: «;» sin directiva delante".to_owned(),
                ));
            }
            Some(Tok::Open) => {
                return Err(ImportError::new(
                    "sintaxis nginx: «{» sin nombre de bloque delante".to_owned(),
                ));
            }
            Some(Tok::Word(name)) => {
                let name = name.clone();
                *pos += 1;
                let mut args = Vec::new();
                loop {
                    match tokens.get(*pos) {
                        Some(Tok::Word(arg)) => {
                            args.push(arg.clone());
                            *pos += 1;
                        }
                        Some(Tok::Semi) => {
                            *pos += 1;
                            nodes.push(Node {
                                name,
                                args,
                                children: Vec::new(),
                            });
                            break;
                        }
                        Some(Tok::Open) => {
                            *pos += 1;
                            let children = parse_block(tokens, pos, true)?;
                            nodes.push(Node {
                                name,
                                args,
                                children,
                            });
                            break;
                        }
                        Some(Tok::Close) => {
                            return Err(ImportError::new(format!(
                                "sintaxis nginx: la directiva «{name}» no termina en «;» antes de «}}»"
                            )));
                        }
                        None => {
                            return Err(ImportError::new(format!(
                                "sintaxis nginx: la directiva «{name}» no termina en «;» ni en «{{»"
                            )));
                        }
                    }
                }
            }
        }
    }
}

fn children<'a>(node: &'a Node, name: &str) -> Vec<&'a Node> {
    node.children.iter().filter(|c| c.name == name).collect()
}

fn first<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children.iter().find(|c| c.name == name)
}

fn directive(node: &Node) -> String {
    if node.args.is_empty() {
        node.name.clone()
    } else {
        format!("{} {}", node.name, node.args.join(" "))
    }
}

/// First argument, or the empty string when the directive has none.
fn first_arg(node: &Node) -> &str {
    node.args.first().map_or("", String::as_str)
}

// ---------------------------------------------------------------------------
// include expansion
// ---------------------------------------------------------------------------

/// Hard caps for one import. They bound the work an `include` graph can make
/// the converter do before it fails with an actionable error.
const INCLUDE_MAX_DEPTH: usize = 16;
const INCLUDE_MAX_FILES: usize = 256;
const INCLUDE_MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct IncludeLimits {
    depth: usize,
    files: usize,
    bytes: usize,
}

const INCLUDE_LIMITS: IncludeLimits = IncludeLimits {
    depth: INCLUDE_MAX_DEPTH,
    files: INCLUDE_MAX_FILES,
    bytes: INCLUDE_MAX_BYTES,
};

/// One successful (or empty) `include` directive, kept apart from the AST so
/// the converter can classify it even though the directive itself is replaced
/// by the nodes it included.
#[derive(Debug)]
struct IncludeEvent {
    directive: String,
    context: String,
    files: usize,
    empty_glob: bool,
}

/// Reads `include` directives from disk and splices their parsed nodes at the
/// directive's position. Paths are canonicalised for cycle detection; glob
/// matches are sorted before they are read, so the same tree always yields
/// the same AST.
struct IncludeExpander {
    root: PathBuf,
    canonical_root: PathBuf,
    limits: IncludeLimits,
    files_read: usize,
    bytes_read: usize,
    stack: Vec<PathBuf>,
    events: Vec<IncludeEvent>,
}

impl IncludeExpander {
    fn new(root: &Path, root_bytes: usize, limits: IncludeLimits) -> Result<Self, ImportError> {
        let canonical_root = std::fs::canonicalize(root).map_err(|err| {
            ImportError::new(format!(
                "--include-root {} is not readable: {err}",
                root.display()
            ))
        })?;
        if !canonical_root.is_dir() {
            return Err(ImportError::new(format!(
                "--include-root {} is not a directory",
                root.display()
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
            canonical_root,
            limits,
            files_read: 0,
            bytes_read: root_bytes,
            stack: Vec::new(),
            events: Vec::new(),
        })
    }

    fn expand(
        mut self,
        nodes: Vec<Node>,
        context: &str,
    ) -> Result<(Vec<Node>, Vec<IncludeEvent>), ImportError> {
        let nodes = self.expand_nodes(nodes, 0, context)?;
        Ok((nodes, self.events))
    }

    fn expand_nodes(
        &mut self,
        nodes: Vec<Node>,
        depth: usize,
        context: &str,
    ) -> Result<Vec<Node>, ImportError> {
        let mut expanded = Vec::new();
        for mut node in nodes {
            if node.name == "include" {
                self.include(&node, depth, context, &mut expanded)?;
                continue;
            }
            let child_context = match node.name.as_str() {
                "http" => "http".to_owned(),
                "stream" => "stream".to_owned(),
                "server" => format!("server {}", server_label(&node)),
                _ => context.to_owned(),
            };
            node.children = self.expand_nodes(node.children, depth, &child_context)?;
            expanded.push(node);
        }
        Ok(expanded)
    }

    fn include(
        &mut self,
        node: &Node,
        depth: usize,
        context: &str,
        out: &mut Vec<Node>,
    ) -> Result<(), ImportError> {
        if node.args.len() != 1 {
            return Err(ImportError::new(format!(
                "include en «{context}» debe tener exactamente una ruta o patrón (encontrados {})",
                node.args.len()
            )));
        }
        if !node.children.is_empty() {
            return Err(ImportError::new(format!(
                "include «{}» no puede ser un bloque",
                node.args[0]
            )));
        }
        let pattern = &node.args[0];
        if pattern.trim().is_empty() {
            return Err(ImportError::new("include con ruta vacía".to_owned()));
        }
        if pattern.contains('$') {
            return Err(ImportError::new(format!(
                "include «{pattern}» usa variables de nginx; el importador solo expande rutas estáticas"
            )));
        }
        let (files, is_glob) = self.resolve(pattern)?;
        if is_glob && files.is_empty() {
            self.events.push(IncludeEvent {
                directive: directive(node),
                context: context.to_owned(),
                files: 0,
                empty_glob: true,
            });
            return Ok(());
        }
        self.events.push(IncludeEvent {
            directive: directive(node),
            context: context.to_owned(),
            files: files.len(),
            empty_glob: false,
        });
        for path in &files {
            out.extend(self.read_file(path, depth, context)?);
        }
        Ok(())
    }

    fn read_file(
        &mut self,
        path: &Path,
        depth: usize,
        context: &str,
    ) -> Result<Vec<Node>, ImportError> {
        if depth + 1 > self.limits.depth {
            return Err(ImportError::new(format!(
                "include «{}»: se supera la profundidad máxima de {} niveles ({} -> {})",
                path.display(),
                self.limits.depth,
                self.chain(),
                path.display()
            )));
        }
        let canonical = std::fs::canonicalize(path).map_err(|err| {
            ImportError::new(format!(
                "include «{}» no se puede leer: {err}",
                path.display()
            ))
        })?;
        if let Some(start) = self.stack.iter().position(|open| *open == canonical) {
            let mut cycle: Vec<String> = self.stack[start..]
                .iter()
                .map(|open| open.display().to_string())
                .collect();
            cycle.push(canonical.display().to_string());
            return Err(ImportError::new(format!(
                "include «{}»: ciclo detectado: {}",
                path.display(),
                cycle.join(" -> ")
            )));
        }
        let metadata = std::fs::metadata(&canonical).map_err(|err| {
            ImportError::new(format!(
                "include «{}» no se puede leer: {err}",
                path.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(ImportError::new(format!(
                "include «{}» no es un fichero regular",
                path.display()
            )));
        }
        if self.files_read + 1 > self.limits.files {
            return Err(ImportError::new(format!(
                "include «{}»: se supera el máximo de {} ficheros incluidos",
                path.display(),
                self.limits.files
            )));
        }
        let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if self.bytes_read.saturating_add(size) > self.limits.bytes {
            return Err(ImportError::new(format!(
                "include «{}»: el contenido expandido supera el máximo de {} bytes",
                path.display(),
                self.limits.bytes
            )));
        }
        let text = std::fs::read_to_string(&canonical).map_err(|err| {
            ImportError::new(format!(
                "include «{}» no es UTF-8 legible: {err}",
                path.display()
            ))
        })?;
        self.files_read += 1;
        self.bytes_read = self.bytes_read.saturating_add(text.len());
        if self.bytes_read > self.limits.bytes {
            return Err(ImportError::new(format!(
                "include «{}»: el contenido expandido supera el máximo de {} bytes",
                path.display(),
                self.limits.bytes
            )));
        }
        let tokens = tokenize(&text).map_err(|err| {
            ImportError::new(format!("include «{}»: {}", path.display(), err.message))
        })?;
        let parsed = parse(&tokens).map_err(|err| {
            ImportError::new(format!("include «{}»: {}", path.display(), err.message))
        })?;
        self.stack.push(canonical);
        let expanded = self.expand_nodes(parsed, depth + 1, context)?;
        self.stack.pop();
        Ok(expanded)
    }

    fn chain(&self) -> String {
        if self.stack.is_empty() {
            "configuración principal".to_owned()
        } else {
            self.stack
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ")
        }
    }

    /// Resolves one nginx include argument. Literal paths must exist; a glob
    /// with no matches is valid in nginx and is reported, not treated as an
    /// error. The boolean says whether the pattern contains glob syntax.
    #[allow(clippy::too_many_lines)]
    fn resolve(&self, pattern: &str) -> Result<(Vec<PathBuf>, bool), ImportError> {
        let raw = Path::new(pattern);
        let is_glob = pattern.chars().any(|c| matches!(c, '*' | '?' | '['));
        let mut candidates = vec![if raw.is_absolute() {
            PathBuf::new()
        } else {
            self.root.clone()
        }];
        let components: Vec<Component<'_>> = raw.components().collect();
        for (idx, component) in components.iter().enumerate() {
            let last = idx + 1 == components.len();
            let mut next = Vec::new();
            for base in std::mem::take(&mut candidates) {
                match component {
                    Component::Prefix(prefix) => {
                        next.push(PathBuf::from(prefix.as_os_str()));
                    }
                    Component::RootDir => {
                        let mut path = base;
                        path.push(Component::RootDir.as_os_str());
                        next.push(path);
                    }
                    Component::CurDir => next.push(base),
                    Component::ParentDir => {
                        if raw.is_absolute() {
                            let mut path = base;
                            path.pop();
                            next.push(path);
                        } else {
                            let mut path = base;
                            if !path.pop() || !path.starts_with(&self.root) {
                                return Err(ImportError::new(format!(
                                    "include «{pattern}» sale de --include-root con «..»"
                                )));
                            }
                            next.push(path);
                        }
                    }
                    Component::Normal(name) => {
                        let name = name.to_str().ok_or_else(|| {
                            ImportError::new(format!(
                                "include «{pattern}» contiene un nombre de fichero no UTF-8"
                            ))
                        })?;
                        if has_glob_meta(name) {
                            match std::fs::read_dir(&base) {
                                Ok(entries) => {
                                    let mut matched = Vec::new();
                                    for entry in entries {
                                        let entry = entry.map_err(|err| {
                                            ImportError::new(format!(
                                                "include «{pattern}»: no se puede listar {}: {err}",
                                                base.display()
                                            ))
                                        })?;
                                        let Some(entry_name) =
                                            entry.file_name().to_str().map(str::to_owned)
                                        else {
                                            continue;
                                        };
                                        if glob_match(name, &entry_name) {
                                            matched.push(entry.path());
                                        }
                                    }
                                    matched.sort();
                                    next.extend(matched);
                                }
                                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                                Err(err) => {
                                    return Err(ImportError::new(format!(
                                        "include «{pattern}»: no se puede listar {}: {err}",
                                        base.display()
                                    )));
                                }
                            }
                        } else {
                            next.push(base.join(name));
                        }
                    }
                }
            }
            next.sort();
            next.dedup();
            if next.is_empty() {
                break;
            }
            // Intermediate glob matches must be directories; a file would
            // make the next `read_dir` fail and hide the real reason.
            if !last {
                next.retain(|path| path.is_dir());
            }
            candidates = next;
        }

        let mut files = Vec::new();
        for candidate in candidates {
            match std::fs::metadata(&candidate) {
                Ok(metadata) if metadata.is_file() => {
                    if !raw.is_absolute() {
                        let canonical = std::fs::canonicalize(&candidate).map_err(|err| {
                            ImportError::new(format!(
                                "include «{}» no se puede leer: {err}",
                                candidate.display()
                            ))
                        })?;
                        if !canonical.starts_with(&self.canonical_root) {
                            return Err(ImportError::new(format!(
                                "include «{pattern}» resuelve fuera de --include-root: {}",
                                canonical.display()
                            )));
                        }
                    }
                    files.push(candidate);
                }
                Ok(_) if is_glob => {}
                Ok(_) => {
                    return Err(ImportError::new(format!(
                        "include «{pattern}» apunta a un directorio, no a un fichero"
                    )));
                }
                Err(err) if is_glob && err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(ImportError::new(format!(
                        "include «{pattern}» no se puede leer: {err}"
                    )));
                }
            }
        }
        files.sort();
        files.dedup();
        Ok((files, is_glob))
    }
}

fn has_glob_meta(component: &str) -> bool {
    component.chars().any(|c| matches!(c, '*' | '?' | '['))
}

/// POSIX-glob subset used by nginx paths: `*`, `?`, `[...]` and backslash
/// escaping. `*` does not match a leading dot unless the pattern says so.
fn glob_match(pattern: &str, name: &str) -> bool {
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut p = 0;
    let mut n = 0;
    let mut star: Option<usize> = None;
    let mut star_name = 0;
    while n < name.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_name = n;
            p += 1;
            continue;
        }
        if p < pattern.len() {
            let (matches, after) = match pattern[p] {
                '?' => (true, p + 1),
                '[' => match bracket_match(&pattern, p, name[n]) {
                    Some((matches, after)) => (matches, after),
                    None => (name[n] == '[', p + 1),
                },
                '\\' if p + 1 < pattern.len() => (name[n] == pattern[p + 1], p + 2),
                literal => (name[n] == literal, p + 1),
            };
            if matches {
                p = after;
                n += 1;
                continue;
            }
        }
        if let Some(star_pos) = star {
            star_name += 1;
            n = star_name;
            p = star_pos + 1;
            continue;
        }
        return false;
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// Matches one `[...]` class. Returns the match and the index after `]`, or
/// `None` when the class is not terminated (POSIX then treats `[` literally).
fn bracket_match(pattern: &[char], start: usize, candidate: char) -> Option<(bool, usize)> {
    let mut i = start + 1;
    let negated = matches!(pattern.get(i), Some('!' | '^'));
    if negated {
        i += 1;
    }
    let mut matched = false;
    let mut any = false;
    while i < pattern.len() {
        if pattern[i] == ']' && any {
            return Some((matched != negated, i + 1));
        }
        any = true;
        let first = if pattern[i] == '\\' && i + 1 < pattern.len() {
            i += 1;
            pattern[i]
        } else {
            pattern[i]
        };
        if i + 2 < pattern.len() && pattern[i + 1] == '-' && pattern[i + 2] != ']' {
            let last = pattern[i + 2];
            if first <= candidate && candidate <= last {
                matched = true;
            }
            i += 3;
        } else {
            if first == candidate {
                matched = true;
            }
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

/// Backend TLS values nginx inherits independently per directive from `http`
/// to `server` to `location`. `None` means "not declared at this level", which
/// is what lets a narrower level inherit only the fields it did not set; it is
/// not the same as nginx's default (`proxy_ssl_verify`/`proxy_ssl_server_name`
/// default to off).
#[derive(Debug, Clone, Default)]
struct ProxySslDefaults {
    verify: Option<bool>,
    ca_file: Option<String>,
    sni: Option<bool>,
    server_name: Option<String>,
}

/// Listener TLS values nginx inherits independently per directive from `http`
/// to `server`. `None` means "not declared at this level"; the server keeps
/// its own value for that directive and inherits only the others.
#[derive(Debug, Clone, Default)]
struct HttpTlsDefaults {
    cert: Option<String>,
    key: Option<String>,
    protocols: Option<Vec<String>>,
    client_ca: Option<String>,
    verify_client: Option<String>,
}

/// Defaults inherited by every listener generated from the `http` block.
#[derive(Default)]
struct HttpDefaults {
    gzip: bool,
    gzip_level: Option<u32>,
    gzip_min_size: Option<u64>,
    max_body_bytes: Option<u64>,
    idle_timeout: Option<String>,
    connect_timeout: Option<String>,
    server_header_timeout: Option<String>,
    client_header_timeout: Option<String>,
    cache: bool,
    /// `proxy_ssl_*` at http scope (N-2): inherited by every server and
    /// location below it, unless a narrower level declares the same directive.
    proxy_ssl: ProxySslDefaults,
    /// Listener TLS at http scope: inherited by every `server` that does not
    /// declare the same directive, then emitted on the listener.
    tls: HttpTlsDefaults,
}

pub(super) fn convert(
    conv: &mut Conv,
    text: &str,
    include_root: Option<&Path>,
) -> Result<(), ImportError> {
    let mut nodes = parse(&tokenize(text)?)?;
    if let Some(root) = include_root {
        let (expanded, events) =
            IncludeExpander::new(root, text.len(), INCLUDE_LIMITS)?.expand(nodes, "global")?;
        nodes = expanded;
        for event in events {
            if event.empty_glob {
                conv.review(
                    1,
                    &event.directive,
                    &event.context,
                    "include sin coincidencias: el patrón no incluye ningún fichero",
                );
            } else {
                conv.info(
                    1,
                    &event.directive,
                    &event.context,
                    format!(
                        "include expandido: {} fichero(s) leído(s) en orden lexicográfico",
                        event.files
                    ),
                );
            }
        }
    }
    for node in &nodes {
        let before = conv.notes.len();
        match node.name.as_str() {
            "http" => http(conv, node)?,
            "stream" => stream(conv, node)?,
            "events" | "mail" => conv.info(
                1,
                node.name.clone(),
                "global",
                "bloque de infraestructura sin traducción (no afecta a eqproxy.toml)",
            ),
            name if GLOBAL_INFO.contains(&name) => conv.info(
                1,
                directive(node),
                "global",
                "directiva global sin traducción (no afecta a eqproxy.toml)",
            ),
            name if name.starts_with("proxy_ssl_") => {
                unknown_proxy_ssl(conv, &directive(node), "global", name);
            }
            name => conv.gap(
                1,
                directive(node),
                "global",
                format!("directiva global «{name}» sin equivalente conocido: revísela a mano"),
            ),
        }
        conv.touch(&directive(node), "global", before)?;
    }
    downgrade_unfulfilled_proxy_ssl_server_name(conv);
    Ok(())
}

/// `proxy_ssl_server_name on` fills `server_name` only when some HTTPS target
/// has a DNS name (or an explicit `proxy_ssl_name`). With only IP literals the
/// directive has no field of its own: turn every still-`Done` row into `Info`
/// rather than promising a field the TOML does not contain.
fn downgrade_unfulfilled_proxy_ssl_server_name(conv: &mut Conv) {
    let has_server_name = conv.backends.values().any(|backend| {
        backend
            .tls
            .as_ref()
            .and_then(|tls| tls.server_name.as_ref())
            .is_some()
    });
    if has_server_name {
        return;
    }
    for note in &mut conv.notes {
        if note.severity == Severity::Done && note.directive.starts_with("proxy_ssl_server_name") {
            note.severity = Severity::Info;
            "SNI solicitado sin nombre DNS (destino IP o sin proxy_pass https): \
                            no se emite server_name"
                .clone_into(&mut note.message);
        }
    }
}

fn http(conv: &mut Conv, http: &Node) -> Result<(), ImportError> {
    // The directives can appear before or after the server blocks, so the
    // effective http-scope TLS defaults are collected first. The main loop
    // classifies each directive separately.
    let mut defaults = HttpDefaults {
        proxy_ssl: collect_proxy_ssl(http, &ProxySslDefaults::default()),
        tls: collect_http_tls(http),
        ..HttpDefaults::default()
    };
    let has_https_targets = has_https_target(http);
    let has_tls_servers = has_tls_server(http, &defaults.tls);

    for up in children(http, "upstream") {
        upstream(conv, up)?;
    }

    for child in &http.children {
        if matches!(child.name.as_str(), "upstream" | "server") {
            continue;
        }
        let before = conv.notes.len();
        http_directive(
            conv,
            child,
            &mut defaults,
            has_https_targets,
            has_tls_servers,
        );
        conv.touch(&directive(child), "http", before)?;
    }

    // Group server blocks by listen address. EQProxy has one listener per
    // IP:port, so the vhosts of one address share it: the `default_server`
    // (or the first block) defines the listener and every other vhost
    // contributes its rules (`host`) and its certificate (`certs[]`).
    let mut by_addr: BTreeMap<String, Vec<&Node>> = BTreeMap::new();
    for block in children(http, "server") {
        let listens = children(block, "listen");
        if listens.is_empty() {
            by_addr
                .entry("0.0.0.0:80".to_owned())
                .or_default()
                .push(block);
            continue;
        }
        for listen in listens {
            let raw = listen
                .args
                .first()
                .cloned()
                .unwrap_or_else(|| "0.0.0.0:80".to_owned());
            by_addr.entry(raw).or_default().push(block);
        }
    }

    for (raw_listen, servers) in by_addr {
        multi_vhost(conv, &raw_listen, &servers, &defaults)?;
    }
    Ok(())
}

/// One listener per IP:port, shared by the vhosts of that address.
///
/// The `default_server` (or the first block) owns the listener: its fallback
/// backend, its certificate as the default (no-SNI) one and its TLS policy,
/// the only one a listener can hold. Every other vhost keeps its own rules,
/// tagged with `host`, and its certificate joins `[listener.*.tls].certs[]`; a
/// TLS policy the listener cannot carry per vhost (mTLS, versions or ciphers)
/// is `Degraded`, never silently dropped.
fn multi_vhost(
    conv: &mut Conv,
    raw_listen: &str,
    servers: &[&Node],
    defaults: &HttpDefaults,
) -> Result<(), ImportError> {
    let chosen_idx = servers
        .iter()
        .position(|node| is_default_for(node, raw_listen))
        .unwrap_or(0);
    let primary_label = server_label(servers[chosen_idx]);
    let primary_ciphers = ciphers_of(servers[chosen_idx]);
    // Parse every vhost in file order before choosing the default one, so a
    // regex in a non-default vhost keeps its nginx position even when the
    // default_server appears later.
    let mut regex_order = 0;
    let host_names: Vec<ServerNames> = servers
        .iter()
        .map(|node| server_names(node, &mut regex_order))
        .collect();
    let Some((name, mut listener)) = server(
        conv,
        servers[chosen_idx],
        raw_listen,
        defaults,
        &VhostRole::Primary,
        &host_names[chosen_idx],
    )?
    else {
        if servers.len() > 1 {
            conv.gap(
                1,
                "server (varios en la misma dirección)",
                &format!("listen {raw_listen}"),
                format!(
                    "varios vhosts comparten «{raw_listen}» y el default_server («{primary_label}») \
                     no tiene ninguna location con proxy_pass: el listener necesita un backend o un redirect; \
                     migre los vhosts a mano"
                ),
            );
        }
        return Ok(());
    };

    let role = VhostRole::Secondary {
        primary_tls: listener.tls.as_ref(),
        primary_ciphers: primary_ciphers.as_deref(),
        primary_label: &primary_label,
        primary_listener: &listener,
    };
    let mut extra_rules: Vec<Rule> = Vec::new();
    let mut certs: Vec<SniCertIr> = Vec::new();
    for (idx, node) in servers.iter().enumerate() {
        if idx == chosen_idx {
            continue;
        }
        let context = format!("server {}", server_label(node));
        let names = &host_names[idx];
        if names.patterns.is_empty() {
            // A vhost without a host condition would become a catch-all:
            // appended after the named rules it would shadow them, which
            // nginx never does (it picks a server first). Unknown Host belongs to the
            // default_server, never to this secondary vhost.
            conv.review(
                1,
                "server (varios en la misma dirección)",
                &context,
                "vhost sin server_name traducible: EQProxy enruta por host, así que sus reglas no \
                 se emiten y no se convierten en catch-all; los hosts sin coincidencia siguen al \
                 default_server. Declare server_name o migre el vhost a mano",
            );
            // Walk the block for the per-directive report and for the backends
            // it declares, but never retain the listener: its rules have no
            // host and would become a catch-all on the shared listener.
            drop(server(conv, node, raw_listen, defaults, &role, names)?);
            continue;
        }
        let Some((_, vhost)) = server(conv, node, raw_listen, defaults, &role, names)? else {
            continue;
        };
        if let Some(tls) = vhost.tls
            && listener
                .tls
                .as_ref()
                .is_some_and(|primary| primary.cert != tls.cert || primary.key != tls.key)
            && !names.cert_names.is_empty()
        {
            certs.push(SniCertIr {
                cert: tls.cert,
                key: tls.key,
                names: names.cert_names.clone(),
            });
        }
        // Belt and braces: only the default vhost may contribute a hostless
        // rule. A secondary must carry a host condition on every rule;
        // filtering at the merge boundary keeps that invariant even if
        // `server()` changes later.
        extra_rules.extend(vhost.rules.into_iter().filter(|rule| {
            rule.host.is_some() || rule.host_prefix.is_some() || rule.host_regex.is_some()
        }));
    }
    if let Some(tls) = listener.tls.as_mut() {
        tls.certs.extend(certs);
    }
    listener.rules.extend(extra_rules);
    // nginx resolves a virtual server by exact name, then the longest leading
    // wildcard, then the longest trailing wildcard, then regexes in file
    // order; the catch-all (the default vhost with no names) stays last.
    listener.rules.sort_by_key(|rule| rule.host_rank);
    conv.add_listener(name, listener);
    Ok(())
}

/// Whether this block is the `default_server` of `raw_listen`. The flag is
/// per address: one block can be the default of 443 and a secondary vhost of
/// 8443. Looking at every `listen` of the block would let it steal the primary
/// role (and its hostless rules) from the real default of another address.
fn is_default_for(node: &Node, raw_listen: &str) -> bool {
    children(node, "listen").iter().any(|listen| {
        listen.args.first().map(String::as_str) == Some(raw_listen)
            && listen
                .args
                .iter()
                .skip(1)
                .any(|arg| arg == "default_server")
    })
}

/// What `server()` is building: the listener's default vhost, or another vhost
/// of the same address, which only contributes rules, a certificate and
/// policy comparisons.
enum VhostRole<'a> {
    Primary,
    Secondary {
        primary_tls: Option<&'a ListenerTls>,
        primary_ciphers: Option<&'a str>,
        primary_label: &'a str,
        primary_listener: &'a Listener,
    },
}

impl VhostRole<'_> {
    fn is_primary(&self) -> bool {
        matches!(self, Self::Primary)
    }

    fn primary_listener(&self) -> Option<&Listener> {
        match self {
            Self::Secondary {
                primary_listener, ..
            } => Some(*primary_listener),
            Self::Primary => None,
        }
    }
}

/// Reports a listener-scope value declared by a secondary vhost.
///
/// ADR-0051: the listener keeps the `default_server`'s value. A secondary
/// declaration is not a degradation (timeouts, body limits, compression,
/// maxconn and cache are not per-vhost security policies), but it must never
/// read as applied. A matching value is `Info`; a divergent one is `Review`
/// and names the listener value that wins.
fn report_secondary_listener_knob(
    conv: &mut Conv,
    text: &str,
    context: &str,
    role: &VhostRole<'_>,
    description: &str,
    mine: &str,
    primary: Option<&str>,
) {
    let VhostRole::Secondary { primary_label, .. } = role else {
        return;
    };
    if primary == Some(mine) {
        conv.info(
            1,
            text,
            context,
            format!(
                "{description} «{mine}» coincide con el default_server «{primary_label}»: el \
                 listener lo aplica a todos los nombres"
            ),
        );
    } else if let Some(primary) = primary {
        conv.review(
            1,
            text,
            context,
            format!(
                "este vhost declara {description} «{mine}»; el listener conserva el valor del \
                 default_server «{primary_label}» («{primary}») y no aplica el de este vhost"
            ),
        );
    } else {
        conv.review(
            1,
            text,
            context,
            format!(
                "este vhost declara {description} «{mine}»; el default_server no fija ese \
                 valor y el listener no aplica el de este vhost: configúrelo en [listener.*] a mano"
            ),
        );
    }
}

/// One name condition emitted for a vhost, plus the nginx precedence that
/// decides its position on a shared listener.
#[derive(Debug, Clone)]
struct HostPattern {
    exact: Option<String>,
    prefix: Option<String>,
    regex: Option<String>,
    rank: HostRank,
}

impl HostPattern {
    fn same_condition(&self, other: &Self) -> bool {
        self.exact == other.exact && self.prefix == other.prefix && self.regex == other.regex
    }

    fn description(&self) -> String {
        if let Some(host) = &self.exact {
            format!("host = «{host}»")
        } else if let Some(prefix) = &self.prefix {
            format!("host_prefix = «{prefix}»")
        } else if let Some(regex) = &self.regex {
            format!("host_regex = «{regex}»")
        } else {
            "sin condición de host".to_owned()
        }
    }

    fn apply(&self, rule: &mut Rule) {
        rule.host.clone_from(&self.exact);
        rule.host_prefix.clone_from(&self.prefix);
        rule.host_regex.clone_from(&self.regex);
        rule.host_rank = self.rank;
    }
}

/// `server_name` values of one vhost, already translated to `EQProxy` host
/// conditions and to the names an SNI certificate can announce.
#[derive(Debug, Clone, Default)]
struct ServerNames {
    patterns: Vec<HostPattern>,
    cert_names: Vec<String>,
    /// Names that route correctly but cannot be represented in
    /// `[listener.*.tls].certs[].names`.
    unrepresented: Vec<String>,
    /// Invalid or unsupported names, named in the report.
    issues: Vec<String>,
    raw: Vec<String>,
}

impl ServerNames {
    fn add_pattern(&mut self, pattern: HostPattern) {
        if !self
            .patterns
            .iter()
            .any(|known| known.same_condition(&pattern))
        {
            self.patterns.push(pattern);
        }
    }

    fn add_cert_name(&mut self, name: &str, source: &str) {
        if let Some(why) = crate::validate::sni_name_problem(name) {
            self.issues.push(format!(
                "server_name «{source}»: {why}; se usa para enrutar pero no como nombre SNI"
            ));
            self.unrepresented.push(source.to_owned());
            return;
        }
        if !self.cert_names.iter().any(|known| known == name) {
            self.cert_names.push(name.to_owned());
        }
    }
}

/// Parses every `server_name` directive of a vhost with nginx's matching
/// grammar: exact names, a leading `*.` wildcard (one or more labels), the
/// leading-dot shorthand (apex plus that wildcard), the trailing `mail.*`
/// wildcard and `~` regular expressions. `regex_order` is shared across the
/// listener so regexes keep the order in which they appear in the file.
#[allow(clippy::too_many_lines)]
fn server_names(node: &Node, regex_order: &mut usize) -> ServerNames {
    let mut names = ServerNames::default();
    // nginx accumulates names across repeated `server_name` directives.
    for directive in children(node, "server_name") {
        for raw in &directive.args {
            let raw = raw.trim();
            names.raw.push(raw.to_owned());
            if raw.is_empty() {
                names.issues.push("server_name vacío: se ignora".to_owned());
                continue;
            }
            let lower = raw.to_ascii_lowercase();
            if lower == "$hostname" {
                names.issues.push(
                    "server_name «$hostname»: el nombre de la máquina se resuelve al arrancar \
                     nginx y no está en el fichero; declare el nombre real"
                        .to_owned(),
                );
                continue;
            }
            let regex_source = raw
                .strip_prefix("~*")
                .map(|pattern| (pattern, true))
                .or_else(|| raw.strip_prefix('~').map(|pattern| (pattern, false)));
            if let Some((pattern, explicit_case_insensitive)) = regex_source {
                let order = *regex_order;
                *regex_order += 1;
                if pattern.is_empty() {
                    names.issues.push(format!(
                        "server_name «{raw}»: expresión regular vacía; se ignora"
                    ));
                    continue;
                }
                // nginx compiles a server_name regex case-insensitively when
                // it contains an uppercase ASCII letter, and matches it
                // against the lowercased host. EQProxy does the same except
                // for the automatic flag, so add it explicitly.
                let regex = if explicit_case_insensitive
                    || pattern.bytes().any(|byte| byte.is_ascii_uppercase())
                {
                    format!("(?i){pattern}")
                } else {
                    pattern.to_owned()
                };
                if let Err(why) = RegexMatcher::parse(&regex) {
                    names.issues.push(format!(
                        "server_name «{raw}»: la expresión no se puede evaluar en EQProxy ({why}); \
                         se ignora"
                    ));
                    continue;
                }
                names.unrepresented.push(raw.to_owned());
                names.add_pattern(HostPattern {
                    exact: None,
                    prefix: None,
                    regex: Some(regex),
                    rank: HostRank::Regex(order),
                });
                continue;
            }
            if let Some(suffix) = lower.strip_prefix("*.") {
                if suffix.is_empty() || suffix.contains('*') || suffix.starts_with('.') {
                    names.issues.push(format!(
                        "server_name «{raw}»: comodín inicial no válido (debe ser *.dominio); \
                         se ignora"
                    ));
                    continue;
                }
                names.add_pattern(HostPattern {
                    exact: None,
                    prefix: None,
                    regex: Some(leading_wildcard_regex(suffix)),
                    rank: HostRank::LeadingWildcard(std::cmp::Reverse(suffix.len())),
                });
                let cert = format!("*.{suffix}");
                names.add_cert_name(&cert, raw);
                continue;
            }
            if let Some(suffix) = lower.strip_prefix('.') {
                if suffix.is_empty() || suffix.contains('*') || suffix.starts_with('.') {
                    names.issues.push(format!(
                        "server_name «{raw}»: forma de comodín inicial no válida (.dominio); \
                         se ignora"
                    ));
                    continue;
                }
                // The leading dot is shorthand for the exact apex plus the
                // leading wildcard; both are emitted, in that order.
                names.add_pattern(HostPattern {
                    exact: Some(suffix.to_owned()),
                    prefix: None,
                    regex: None,
                    rank: HostRank::Exact,
                });
                names.add_cert_name(suffix, raw);
                names.add_pattern(HostPattern {
                    exact: None,
                    prefix: None,
                    regex: Some(leading_wildcard_regex(suffix)),
                    rank: HostRank::LeadingWildcard(std::cmp::Reverse(suffix.len())),
                });
                let cert = format!("*.{suffix}");
                names.add_cert_name(&cert, raw);
                continue;
            }
            if let Some(prefix) = lower.strip_suffix('*') {
                if prefix.is_empty() || prefix.contains('*') || !prefix.ends_with('.') {
                    names.issues.push(format!(
                        "server_name «{raw}»: comodín final no válido (debe ser dominio.*); \
                         se ignora"
                    ));
                    continue;
                }
                names.unrepresented.push(raw.to_owned());
                names.add_pattern(HostPattern {
                    exact: None,
                    prefix: Some(prefix.to_owned()),
                    regex: None,
                    rank: HostRank::TrailingWildcard(std::cmp::Reverse(prefix.len())),
                });
                continue;
            }
            if lower.contains('*') {
                names.issues.push(format!(
                    "server_name «{raw}»: comodín no válido (sólo al inicio o al final, en un \
                     límite de etiqueta); se ignora"
                ));
                continue;
            }
            names.add_pattern(HostPattern {
                exact: Some(lower.clone()),
                prefix: None,
                regex: None,
                rank: HostRank::Exact,
            });
            names.add_cert_name(&lower, raw);
        }
    }
    names
}

/// nginx's leading wildcard (`*.example.org`) matches one or more labels
/// before the suffix, but not the bare suffix.
fn leading_wildcard_regex(suffix: &str) -> String {
    format!(r"^[^.]+(?:\.[^.]+)*\.{}$", regex_literal(suffix))
}

/// Escapes a literal so it can be embedded in a `regex` condition.
fn regex_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        ) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

fn ciphers_of(node: &Node) -> Option<String> {
    first(node, "ssl_ciphers").map(|n| n.args.join(" "))
}

fn server_label(node: &Node) -> String {
    first(node, "server_name")
        .and_then(|n| n.args.first().cloned())
        .unwrap_or_else(|| "(sin server_name)".to_owned())
}

fn http_directive(
    conv: &mut Conv,
    node: &Node,
    defaults: &mut HttpDefaults,
    has_https_targets: bool,
    has_tls_servers: bool,
) {
    let name = node.name.as_str();
    let arg = node.args.first().map_or_default(String::as_str);
    let text = directive(node);
    match name {
        "gzip" => {
            defaults.gzip = arg == "on";
            conv.done(
                1,
                &text,
                "http",
                format!(
                    "[listener.*.compression].gzip = {} en cada listener generado",
                    defaults.gzip
                ),
            );
        }
        "gzip_comp_level" => {
            if let Ok(level) = arg.parse::<u32>() {
                defaults.gzip_level = Some(level.clamp(1, 9));
                conv.done(1, &text, "http", "compression.level (1..9)");
            } else {
                conv.review(1, &text, "http", "nivel de gzip no numérico: se usa el de EQProxy (6)");
            }
        }
        "gzip_min_length" => match parse_size(arg) {
            Some(size) => {
                defaults.gzip_min_size = Some(size);
                conv.done(1, &text, "http", "compression.min_size");
            }
            None => conv.review(1, &text, "http", "tamaño mínimo de gzip no reconocido: se usa 1024"),
        },
        "gzip_types" | "gzip_vary" | "gzip_proxied" | "gzip_static" => conv.review(
            1,
            &text,
            "http",
            "tipos de gzip: EQProxy comprime text/*, application/json y xml por defecto (compression.types)",
        ),
        "client_max_body_size" => match parse_size(arg) {
            Some(size) => {
                defaults.max_body_bytes = Some(size);
                conv.done(
                    1,
                    &text,
                    "http",
                    "listener.limits.max_body_bytes (413 antes de abrir el backend si el cliente lo declara)",
                );
            }
            None => conv.review(1, &text, "http", "tamaño de cuerpo no reconocido: se deja ilimitado"),
        },
        "keepalive_timeout" => {
            if let Some(value) = convert_duration(conv, arg, "s", &text, "http") {
                defaults.idle_timeout = Some(value);
                conv.review(
                    1,
                    &text,
                    "http",
                    "listener.idle_timeout (aproximado: en EQProxy la inactividad es bidireccional)",
                );
            }
        }
        "client_header_timeout" => {
            if let Some(value) = convert_duration(conv, arg, "s", &text, "http") {
                defaults.client_header_timeout = Some(value);
                conv.done(1, &text, "http", "listener.client_header_timeout");
            }
        }
        "proxy_connect_timeout" => {
            if let Some(value) = convert_duration(conv, arg, "s", &text, "http") {
                defaults.connect_timeout = Some(value);
                conv.done(1, &text, "http", "listener.connect_timeout");
            }
        }
        "proxy_read_timeout" | "send_timeout" => {
            if let Some(value) = convert_duration(conv, arg, "s", &text, "http") {
                defaults.server_header_timeout = Some(value);
                conv.review(
                    1,
                    &text,
                    "http",
                    "listener.server_header_timeout (aproximado: EQProxy solo acota la cabecera de respuesta)",
                );
            }
        }
        "proxy_send_timeout" => conv.review(
            1,
            &text,
            "http",
            "sin equivalente: en EQProxy idle_timeout es el único plazo de inactividad",
        ),
        "proxy_cache_path" => conv.review(
            1,
            &text,
            "http",
            "[listener.*.cache].disk_path (la persistencia existe; la zona de nginx no se traduce)",
        ),
        "proxy_temp_path" | "client_body_temp_path" => conv.info(
            1,
            &text,
            "http",
            "rutas temporales de nginx: EQProxy no escribe cuerpos en disco",
        ),
        "server_names_hash_bucket_size" | "server_names_hash_max_size" | "variables_hash_max_size"
        | "types_hash_max_size" | "map_hash_bucket_size" | "map_hash_max_size" => conv.info(
            1,
            &text,
            "http",
            "ajuste de tablas hash de nginx: sin efecto en EQProxy",
        ),
        "include" => conv.review(
            1,
            &text,
            "http",
            format!("include no se expande: revise «{arg}» a mano y vuelva a convertir"),
        ),
        "default_type" | "types" | "charset" | "charset_types" | "source_charset" => {
            conv.info(1, &text, "http", "cosmético de nginx (tipos MIME): EQProxy solo proxifica");
        }
        "log_format" => conv.review(
            1,
            &text,
            "http",
            "[access_log] con format = \"json\" (campos estables) o \"combined\" (forma nginx)",
        ),
        "access_log" => conv.review(
            1,
            &text,
            "http",
            "[access_log].path (una línea por petición; la ruta y el formato se eligen en el TOML)",
        ),
        "error_log" => conv.info(
            1,
            &text,
            "http",
            "log de errores del proceso: el dataplane escribe a stderr",
        ),
        "sendfile" | "tcp_nopush" | "tcp_nodelay" | "aio" | "directio" => conv.info(
            1,
            &text,
            "http",
            "rendimiento interno de nginx sin mando expuesto",
        ),
        "limit_req_zone" => conv.review(
            1,
            &text,
            "http",
            "[[listener.*.rules.rate_limit]] por IP de cliente (rate/per/burst), no una zona compartida",
        ),
        "map" | "geo" => conv.review(
            1,
            &text,
            "http",
            "[maps.*] cubre mapas clave→valor; el conversor no traduce sus entradas",
        ),
        "split_clients" => conv.review(
            1,
            &text,
            "http",
            "[[listener.*.rules]].split (reparto ponderado entre backends)",
        ),
        "ssl_protocols" => {
            let (wanted, has_legacy) = tls_protocol_range(&node.args);
            if wanted.is_none() {
                conv.review(
                    1,
                    &text,
                    "http",
                    "ssl_protocols no incluye TLSv1.2/1.3: EQProxy no puede desactivar ambos",
                );
            } else if has_tls_servers {
                conv.done(
                    1,
                    &text,
                    "http",
                    "[listener.*.tls].min_version/max_version por defecto (cada server puede \
                     sobrescribirlo)",
                );
            } else {
                conv.info(
                    1,
                    &text,
                    "http",
                    "sin server TLS: la política no llega a ningún listener",
                );
            }
            if has_legacy {
                conv.info(
                    1,
                    &text,
                    "http",
                    "TLS 1.0/1.1 y SSL nunca se aceptan en EQProxy (se ignoran)",
                );
            }
        }
        "ssl_certificate" | "ssl_certificate_key" => {
            if node.args.first().is_some_and(|path| !path.is_empty()) {
                if has_tls_servers {
                    conv.done(
                        1,
                        &text,
                        "http",
                        "valor por defecto de [listener.*.tls] (cada server puede sobrescribirlo)",
                    );
                } else {
                    conv.info(
                        1,
                        &text,
                        "http",
                        "sin server TLS: el certificado no llega a ningún listener",
                    );
                }
            } else {
                conv.review(1, &text, "http", "ssl_certificate sin ruta: se ignora");
            }
        }
        "ssl_client_certificate" => {
            if node.args.first().is_some_and(|path| !path.is_empty()) {
                if has_tls_servers && http_client_verify_active(node, &defaults.tls) {
                    conv.done(
                        1,
                        &text,
                        "http",
                        "valor por defecto de [listener.*.tls].client_ca (mTLS heredado por los \
                         servers que lo activen)",
                    );
                } else {
                    conv.info(
                        1,
                        &text,
                        "http",
                        "CA de clientes sin ssl_verify_client activo: no hace nada",
                    );
                }
            } else {
                conv.review(
                    1,
                    &text,
                    "http",
                    "ssl_client_certificate sin ruta: se ignora",
                );
            }
        }
        "ssl_verify_client" => match client_verify_mode(node.args.first().map(String::as_str)) {
            ClientVerifyMode::Required
                if has_tls_servers && http_has_client_ca(node, &defaults.tls) =>
            {
                conv.done(
                    1,
                    &text,
                    "http",
                    "activa [listener.*.tls].client_ca con el CA heredado o declarado",
                );
            }
            ClientVerifyMode::Required | ClientVerifyMode::Off => conv.info(
                1,
                &text,
                "http",
                "sin ssl_client_certificate activo: no exige certificado de cliente",
            ),
            ClientVerifyMode::Unsupported(mode) => {
                reject_optional_client_verify(conv, &text, "http", mode);
            }
        },
        "proxy_set_header" => conv.review(
            1,
            &text,
            "http",
            "nivel http: se traduce donde aparece (location/server); aquí se resume",
        ),
        "proxy_http_version" => conv.gap(
            1,
            &text,
            "http",
            "versión al backend: la decide el ALPN de [backend.*.tls] (h2 o HTTP/1.1)",
        ),
        "proxy_buffering" | "proxy_buffers" | "proxy_busy_buffers_size" | "proxy_max_temp_file_size" => {
            conv.gap(
                1,
                &text,
                "http",
                "buffers: [listener.*.limits] (max_head_bytes, stream_buffer_bytes)",
            );
        }
        "client_body_buffer_size" | "large_client_header_buffers" | "client_header_buffer_size" => {
            conv.gap(
                1,
                &text,
                "http",
                "buffers: [listener.*.limits].max_head_bytes (16 KiB por defecto)",
            );
        }
        "resolver" => conv.review(
            1,
            &text,
            "http",
            "[backend.*].resolve_interval (re-resolución en caliente sin bloqueo)",
        ),
        "resolver_timeout" => conv.info(1, &text, "http", "timeout del resolver: sin mando en EQProxy"),
        "reset_timedout_connection" | "lingering_close" | "lingering_time" | "lingering_timeout" => {
            conv.info(1, &text, "http", "cierre de conexiones: lo gestiona el dataplane");
        }
        // N-2: proxy_ssl_* at http scope is inherited by every server/location
        // below it. The values were collected before the loop; this row
        // classifies the directive itself.
        name if name.starts_with("proxy_ssl_") => {
            classify_proxy_ssl(conv, node, "http", has_https_targets);
        }
        name => match supported_manually(name) {
            Some(place) => conv.review(
                1,
                &text,
                "http",
                format!("EQProxy sí lo soporta en {place}: transcríbalo a mano"),
            ),
            None => match unsupported(name) {
                Some(reason) => conv.gap(1, &text, "http", reason),
                None => conv.review(
                    1,
                    &text,
                    "http",
                    "directiva http sin traducción automática: revísela a mano",
                ),
            },
        },
    }
}

/// `upstream { … }` → `[backend.*]`.
#[allow(clippy::too_many_lines)]
fn upstream(conv: &mut Conv, up: &Node) -> Result<(), ImportError> {
    let source_name = up
        .args
        .first()
        .cloned()
        .unwrap_or_else(|| "upstream".to_owned());
    let context = format!("upstream {source_name}");
    let name = conv.free_backend_name(&source_name);
    let mut backend = super::Backend::default();
    let mut balance: Option<String> = None;

    for child in &up.children {
        let before = conv.notes.len();
        let text = directive(child);
        match child.name.as_str() {
            "server" => {
                let Some(raw) = child.args.first() else {
                    conv.review(1, &text, &context, "línea server sin dirección: se omite");
                    continue;
                };
                let mut weight = 1;
                let mut backup = false;
                let mut maxconn = 0;
                for arg in child.args.iter().skip(1) {
                    if let Some(value) = arg.strip_prefix("weight=") {
                        match value.parse::<u32>() {
                            Ok(parsed) if parsed > 256 => {
                                weight = 256;
                                conv.review(
                                    1,
                                    arg.clone(),
                                    &context,
                                    "weight > 256 (máximo de EQProxy): recortado a 256",
                                );
                            }
                            Ok(parsed) => weight = parsed,
                            Err(_) => conv.review(
                                1,
                                arg.clone(),
                                &context,
                                "weight no numérico: se usa 1",
                            ),
                        }
                    } else if arg == "down" {
                        weight = 0;
                        conv.review(
                            1,
                            arg.clone(),
                            &context,
                            "«down» ≈ weight = 0 (drenado: nunca seleccionado, sí chequeado)",
                        );
                    } else if arg == "backup" {
                        backup = true;
                        conv.done(
                            1,
                            arg.clone(),
                            &context,
                            "backup = true (B8: solo se usa cuando ningún primario es elegible)",
                        );
                    } else if arg.starts_with("max_fails=") || arg.starts_with("fail_timeout=") {
                        conv.review(
                            1,
                            arg.clone(),
                            &context,
                            "chequeo pasivo de nginx: EQProxy tiene check TCP activo y detección pasiva (health_check.passive)",
                        );
                    } else if let Some(value) = arg.strip_prefix("max_conns=") {
                        // nginx `max_conns=N` (B-H16): the same per-server cap
                        // as HAProxy `server … maxconn`. `0` means unlimited in
                        // nginx, which is also the schema default.
                        match value.parse::<u32>() {
                            Ok(parsed) if parsed > crate::schema::MAX_SERVER_MAXCONN => {
                                maxconn = crate::schema::MAX_SERVER_MAXCONN;
                                conv.review(
                                    1,
                                    arg.clone(),
                                    &context,
                                    format!(
                                        "max_conns > {} (máximo de EQProxy): recortado a {}",
                                        crate::schema::MAX_SERVER_MAXCONN,
                                        crate::schema::MAX_SERVER_MAXCONN
                                    ),
                                );
                            }
                            Ok(parsed) => {
                                maxconn = parsed;
                                conv.done(
                                    1,
                                    arg.clone(),
                                    &context,
                                    format!(
                                        "maxconn = {parsed} (tope de conexiones de este servidor)"
                                    ),
                                );
                            }
                            Err(_) => conv.review(
                                1,
                                arg.clone(),
                                &context,
                                "max_conns no numérico: se omite el tope por servidor",
                            ),
                        }
                    } else if arg.starts_with("slow_start=") {
                        conv.review(
                            1,
                            arg.clone(),
                            &context,
                            "[backend.*].slow_start (ventana entera, no por servidor)",
                        );
                    } else if arg == "resolve"
                        || arg.starts_with("route=")
                        || arg.starts_with("id=")
                    {
                        conv.info(
                            1,
                            arg.clone(),
                            &context,
                            "metadato del servidor sin equivalente: se ignora",
                        );
                    } else {
                        conv.gap(
                            1,
                            arg.clone(),
                            &context,
                            "parámetro de servidor sin equivalente conocido: revíselo a mano",
                        );
                    }
                }
                if let Some(addr) = normalize_addr(conv, raw, Some(80), &text, &context, true) {
                    let rendered = addr.clone();
                    backend.servers.push(super::Server {
                        addr,
                        weight,
                        backup,
                        maxconn,
                    });
                    conv.done(
                        1,
                        &text,
                        &context,
                        format!("servidor {rendered} (peso {weight}) en el upstream"),
                    );
                }
            }
            "least_conn" => {
                balance = Some("leastconn".to_owned());
                conv.done(
                    1,
                    &text,
                    &context,
                    "balance = \"leastconn\" (least-connections ponderado)",
                );
            }
            "ip_hash" => {
                backend.affinity = Some(super::Affinity {
                    mode: "source".to_owned(),
                    cookie: None,
                });
                conv.done(
                    1,
                    &text,
                    &context,
                    "[backend.*.affinity] mode = \"source\" (hash de IP del cliente)",
                );
            }
            "keepalive" => conv.review(
                1,
                &text,
                &context,
                "pool de EQProxy: 64 conexiones idle por servidor, expiración 60 s",
            ),
            "random" | "hash" => {
                if balance.is_none() {
                    balance = Some("roundrobin".to_owned());
                }
                conv.gap(
                    1,
                    &text,
                    &context,
                    "algoritmo de balance sin soporte: EQProxy emite roundrobin ponderado",
                );
            }
            "zone" | "queue" | "ntlm" | "sticky" | "state" => conv.gap(
                1,
                &text,
                &context,
                "directiva de upstream sin equivalente en EQProxy",
            ),
            name if name.starts_with("proxy_ssl_") => {
                unknown_proxy_ssl(conv, &text, &context, name);
            }
            other => conv.gap(
                1,
                &text,
                &context,
                format!(
                    "directiva de upstream «{other}» sin equivalente conocido: revísela a mano"
                ),
            ),
        }
        conv.touch(&directive(child), &context, before)?;
    }

    if backend.servers.is_empty() {
        conv.reject(
            1,
            format!("upstream {source_name}"),
            &context,
            "upstream sin servidores traducibles: se omite (y los listeners que lo usen fallarán)",
        );
        return Ok(());
    }
    if let Some(balance) = balance {
        backend.balance = balance;
    }
    if name != source_name {
        conv.review(
            1,
            format!("upstream {source_name}"),
            &context,
            format!("renombrado a «{name}» (el nombre original no es válido o ya estaba usado)"),
        );
    }
    conv.add_backend(name, backend);
    Ok(())
}

/// Parses `return 301 URL;` (and the URL-only 302 form) into the listener
/// redirect model. Anything that is not a local 3xx with a supported
/// `Location` template is returned as a human-readable reason.
fn server_return(node: &Node) -> Result<(Option<u16>, String), String> {
    let (status, location) = match node.args.as_slice() {
        [code, location] => {
            let code = code
                .parse::<u16>()
                .map_err(|_| format!("return con código no numérico «{code}»: se ignora"))?;
            (Some(code), location.clone())
        }
        [location] if location.contains("://") => (Some(302), location.clone()),
        [code] => {
            return Err(format!(
                "return {code} sin URL: EQProxy necesita un destino para la redirección"
            ));
        }
        _ => return Err("return con argumentos no reconocidos: se ignora".to_owned()),
    };
    if !matches!(status, Some(301 | 302 | 303 | 307 | 308)) {
        return Err(format!(
            "return {} no es una redirección (301, 302, 303, 307, 308): se ignora",
            status.unwrap_or_default()
        ));
    }
    validate_location(&location)
        .map_err(|why| format!("destino «{location}» de return no válido para EQProxy: {why}"))?;
    Ok((status, location))
}

/// One compiled location of a `server` block: the rule plus the prefix
/// length that orders the list (nginx's longest match).
struct Planned {
    rule: Rule,
    /// Length of the path prefix, for nginx's longest-match ordering.
    length: usize,
}

/// One `server { … }` block on one address.
#[allow(clippy::too_many_lines)]
fn server(
    conv: &mut Conv,
    node: &Node,
    raw_listen: &str,
    defaults: &HttpDefaults,
    role: &VhostRole<'_>,
    hosts: &ServerNames,
) -> Result<Option<(String, Listener)>, ImportError> {
    let label = server_label(node);
    let context = format!("server {label}");
    // nginx inherits proxy_ssl_* independently per directive: start from the
    // http-scope values, then apply this server's own declarations. Locations
    // below receive the result and override only the fields they declare.
    let has_https_targets = has_https_target(node);
    let server_proxy_ssl = collect_proxy_ssl(node, &defaults.proxy_ssl);

    let listen_args = children(node, "listen")
        .into_iter()
        .find(|l| l.args.first().map(String::as_str) == Some(raw_listen))
        .map_or_else(|| vec![raw_listen.to_owned()], |l| l.args.clone());
    let flags = &listen_args[1..];
    let has = |needle: &str| flags.iter().any(|flag| flag == needle);
    let ssl = has("ssl");
    let http2 = flags
        .iter()
        .any(|flag| flag == "http2" || flag.starts_with("http2"))
        || first(node, "http2").is_some_and(|n| n.args.first().map(String::as_str) == Some("on"));
    let http3 = flags
        .iter()
        .any(|flag| flag == "quic" || flag.starts_with("http3") || flag.starts_with("quic"));
    let proxy_protocol = has("proxy_protocol");
    if let VhostRole::Secondary { primary_label, .. } = role {
        conv.info(
            1,
            format!("listen {raw_listen}"),
            &context,
            format!(
                "comparte el listener del default_server «{primary_label}»: EQProxy tiene un \
                 listener por IP:puerto"
            ),
        );
    } else {
        conv.done(
            1,
            format!("listen {raw_listen}"),
            &context,
            format!(
                "listener de EQProxy en {raw_listen}{}{}",
                if ssl { " con TLS" } else { "" },
                if http2 { " y ALPN h2" } else { "" }
            ),
        );
    }

    let Some(addr) = normalize_addr(
        conv,
        raw_listen,
        Some(if ssl { 443 } else { 80 }),
        &format!("listen {raw_listen}"),
        &context,
        false,
    ) else {
        conv.reject(
            1,
            format!("listen {raw_listen}"),
            &context,
            "listener omitido: la dirección no es IP:PORT ni HOSTNAME:PORT",
        );
        return Ok(None);
    };

    if proxy_protocol {
        conv.review(
            1,
            "proxy_protocol",
            &context,
            "PROXY protocol de entrada: EQProxy lo soporta con [listener.*].proxy_receive + proxy_allow \
             (allowlist obligatoria), pero no se activa a ciegas sin conocer los orígenes de confianza",
        );
    }

    // TLS termination. nginx resolves ssl_* independently per directive: a
    // server declaration wins for its directive, otherwise the http-scope
    // value is inherited. `local_*` keeps what this server declared for the
    // report and for the extra certificate entry of a secondary vhost.
    let local_cert = tls_arg(node, "ssl_certificate");
    let local_key = tls_arg(node, "ssl_certificate_key");
    let cert = local_cert.as_deref().or(defaults.tls.cert.as_deref());
    let key = local_key.as_deref().or(defaults.tls.key.as_deref());
    let local_protocols = first(node, "ssl_protocols");
    let protocols = local_protocols
        .map(|directive| directive.args.as_slice())
        .or(defaults.tls.protocols.as_deref());
    let local_ca = tls_arg(node, "ssl_client_certificate");
    let ca = local_ca.as_deref().or(defaults.tls.client_ca.as_deref());
    let local_verify = tls_arg(node, "ssl_verify_client");
    let verify = local_verify
        .as_deref()
        .or(defaults.tls.verify_client.as_deref());
    let verify_mode = client_verify_mode(verify);
    let client_auth = matches!(verify_mode, ClientVerifyMode::Required);

    // A secondary vhost without translatable names only exists to be
    // reported: its location rules are dropped after the walk, so do not tell
    // the reader that a `proxy_pass` became a rule.
    let emit_rules = !hosts.patterns.is_empty() || (role.is_primary() && hosts.raw.is_empty());

    let mut tls: Option<ListenerTls> = None;
    if ssl || cert.is_some() || key.is_some() {
        match (cert, key) {
            (Some(cert), Some(key)) => {
                tls = Some(ListenerTls {
                    cert: cert.to_owned(),
                    key: key.to_owned(),
                    alpn: if http2 || http3 {
                        let mut alpn = Vec::new();
                        if http2 {
                            alpn.push("h2".to_owned());
                        }
                        alpn.push("http/1.1".to_owned());
                        alpn
                    } else {
                        Vec::new()
                    },
                    min_version: None,
                    max_version: None,
                    client_ca: if client_auth {
                        ca.map(str::to_owned)
                    } else {
                        None
                    },
                    certs: Vec::new(),
                });
                match role {
                    VhostRole::Primary => {
                        // The http-scope row already reports an inherited
                        // certificate; do not credit the server with a
                        // directive it did not declare.
                        if local_cert.is_some() || local_key.is_some() {
                            conv.done(
                                1,
                                "ssl_certificate / ssl_certificate_key",
                                &context,
                                format!("[listener.*.tls] cert = «{cert}», key = «{key}»"),
                            );
                        }
                    }
                    VhostRole::Secondary {
                        primary_tls,
                        primary_label,
                        ..
                    } => {
                        let same_as_primary = primary_tls.is_some_and(|primary| {
                            primary.cert.as_str() == cert && primary.key.as_str() == key
                        });
                        if primary_tls.is_none() {
                            conv.review(
                                1,
                                "ssl_certificate / ssl_certificate_key",
                                &context,
                                format!(
                                    "este vhost declara TLS y el default_server «{primary_label}» \
                                     emite el listener sin TLS: no hay [listener.*.tls] donde poner \
                                     su certificado; actívelo a mano"
                                ),
                            );
                        } else if same_as_primary {
                            if local_cert.is_some() || local_key.is_some() {
                                conv.info(
                                    1,
                                    "ssl_certificate / ssl_certificate_key",
                                    &context,
                                    format!(
                                        "mismo certificado que el default_server «{primary_label}»: \
                                         ya es el certificado por defecto y no se duplica en \
                                         [listener.*.tls].certs[]"
                                    ),
                                );
                            }
                        } else if hosts.cert_names.is_empty() {
                            conv.review(
                                1,
                                "ssl_certificate / ssl_certificate_key",
                                &context,
                                format!(
                                    "vhost sin nombre expresable en [listener.*.tls].certs[]: \
                                     certs[].names solo admite nombres exactos o *.dominio y \
                                     server_name usa {}; el certificado no se emite",
                                    if hosts.unrepresented.is_empty() {
                                        "una forma no representable".to_owned()
                                    } else {
                                        hosts.unrepresented.join(", ")
                                    }
                                ),
                            );
                        } else {
                            if !hosts.unrepresented.is_empty() {
                                conv.review(
                                    1,
                                    "ssl_certificate / ssl_certificate_key",
                                    &context,
                                    format!(
                                        "[listener.*.tls].certs[] solo anuncia {}; server_name \
                                         también usa {} y esos nombres recibirán el certificado \
                                         por defecto del listener",
                                        hosts.cert_names.join(", "),
                                        hosts.unrepresented.join(", ")
                                    ),
                                );
                            }
                            conv.done(
                                1,
                                "ssl_certificate / ssl_certificate_key",
                                &context,
                                format!(
                                    "[listener.*.tls].certs[] cert = «{cert}», key = «{key}», \
                                     names = [{}]",
                                    hosts.cert_names.join(", ")
                                ),
                            );
                        }
                    }
                }
            }
            _ => conv.reject(
                1,
                "ssl_certificate",
                &context,
                "TLS sin certificado y clave completos: el listener se emite en claro y hay que corregirlo a mano",
            ),
        }
    }
    if let Some(protocols) = protocols {
        let (wanted, has_legacy) = tls_protocol_range(protocols);
        if let Some(tls) = tls.as_mut()
            && let Some((min, max)) = wanted
        {
            tls.min_version = min;
            tls.max_version = max;
        }
        if let Some(local) = local_protocols {
            let text = directive(local);
            match role {
                VhostRole::Primary => {
                    if wanted.is_none() {
                        conv.review(
                            1,
                            &text,
                            &context,
                            "ssl_protocols no incluye TLSv1.2/1.3: EQProxy no puede desactivar ambos",
                        );
                    }
                    if has_legacy {
                        conv.info(
                            1,
                            &text,
                            &context,
                            "TLS 1.0/1.1 y SSL nunca se aceptan en EQProxy (se ignoran)",
                        );
                    }
                    if wanted.is_some() {
                        if tls.is_none() {
                            conv.info(
                                1,
                                &text,
                                &context,
                                "sin [listener.*.tls] no hay política TLS que fijar (listener en claro)",
                            );
                        } else {
                            conv.done(
                                1,
                                &text,
                                &context,
                                "[listener.*.tls].min_version/max_version (EQProxy nunca acepta TLS 1.0/1.1)",
                            );
                        }
                    }
                }
                VhostRole::Secondary {
                    primary_tls,
                    primary_label,
                    ..
                } => {
                    let primary = primary_tls.map(|tls| (tls.min_version, tls.max_version));
                    if primary == wanted {
                        conv.info(
                            1,
                            &text,
                            &context,
                            format!(
                                "misma política TLS que el default_server «{primary_label}»: el \
                                 listener la aplica a todos los nombres"
                            ),
                        );
                    } else {
                        conv.degraded(
                            1,
                            &text,
                            &context,
                            Degradation::ProtocolPolicy,
                            format!(
                                "este vhost declara otra política TLS; el listener aplica la del \
                                 default_server «{primary_label}» a todos los nombres"
                            ),
                        );
                    }
                }
            }
        } else if let VhostRole::Secondary {
            primary_tls,
            primary_label,
            ..
        } = role
        {
            // The http-scope policy was inherited by this vhost. It is only
            // safe when it matches the listener policy selected by the
            // default_server; otherwise nginx would apply a different range
            // to this name and EQProxy cannot.
            let primary = primary_tls.map(|tls| (tls.min_version, tls.max_version));
            if primary != wanted {
                conv.degraded(
                    1,
                    format!("ssl_protocols {}", protocols.join(" ")),
                    &context,
                    Degradation::ProtocolPolicy,
                    format!(
                        "este vhost hereda de http otra política TLS; el listener aplica la del \
                         default_server «{primary_label}» a todos los nombres"
                    ),
                );
            }
        }
    }
    if let Some(ca) = ca {
        let ca_text = local_ca.as_deref().map_or_else(
            || format!("ssl_client_certificate {ca} (heredado de http)"),
            |value| format!("ssl_client_certificate {value}"),
        );
        if client_auth {
            match role {
                VhostRole::Primary => {
                    if local_ca.is_some() {
                        if tls.is_some() {
                            conv.done(
                                1,
                                &ca_text,
                                &context,
                                "mTLS: [listener.*.tls].client_ca (los certificados de cliente se \
                                 exigen y verifican)",
                            );
                        } else {
                            conv.review(
                                1,
                                &ca_text,
                                &context,
                                "sin [listener.*.tls] no hay dónde configurar el CA de clientes: se \
                                 ignora",
                            );
                        }
                    }
                }
                VhostRole::Secondary {
                    primary_tls,
                    primary_label,
                    ..
                } => {
                    let mine = tls.as_ref().and_then(|tls| tls.client_ca.as_deref());
                    let primary = primary_tls.and_then(|tls| tls.client_ca.as_deref());
                    let should_report = local_ca.is_some() || mine != primary;
                    if should_report && mine == primary {
                        conv.info(
                            1,
                            &ca_text,
                            &context,
                            format!("mismo CA de clientes que el default_server «{primary_label}»"),
                        );
                    } else if should_report && primary.is_none() {
                        conv.degraded(
                            1,
                            &ca_text,
                            &context,
                            Degradation::ClientCertificate,
                            format!(
                                "este vhost exige certificado de cliente y el listener no: el \
                                 default_server «{primary_label}» no declara mTLS"
                            ),
                        );
                    } else if should_report {
                        conv.degraded(
                            1,
                            &ca_text,
                            &context,
                            Degradation::ClientCertificate,
                            format!(
                                "el listener exige el CA del default_server «{primary_label}» a \
                                 todos los nombres; este vhost declara otro (o ninguno)"
                            ),
                        );
                    }
                }
            }
        } else if local_ca.is_some() {
            conv.info(
                1,
                &ca_text,
                &context,
                "CA de clientes sin ssl_verify_client activo: no hace nada",
            );
        }
    }
    if let Some(verify_node) = first(node, "ssl_verify_client") {
        let text = directive(verify_node);
        match verify_mode {
            ClientVerifyMode::Required if ca.is_some() && tls.is_some() => conv.done(
                1,
                &text,
                &context,
                "activa [listener.*.tls].client_ca con el CA local o heredado",
            ),
            ClientVerifyMode::Required => conv.info(
                1,
                &text,
                &context,
                "sin ssl_client_certificate disponible: no hay CA que verificar",
            ),
            ClientVerifyMode::Off => conv.info(
                1,
                &text,
                &context,
                "ssl_verify_client desactivado: no exige certificado de cliente",
            ),
            ClientVerifyMode::Unsupported(mode) => {
                reject_optional_client_verify(conv, &text, &context, mode);
            }
        }
    }

    // The listener has one client-auth policy for every name. A secondary
    // vhost that disables mTLS while the default_server enforces it loses that
    // policy on EQProxy's side, so it must be degraded even when the inherited
    // CA is still present in the http defaults.
    if let VhostRole::Secondary {
        primary_tls,
        primary_label,
        ..
    } = role
    {
        let mine = tls.as_ref().and_then(|tls| tls.client_ca.as_deref());
        let primary = primary_tls.and_then(|tls| tls.client_ca.as_deref());
        if mine.is_none()
            && primary.is_some()
            && !matches!(verify_mode, ClientVerifyMode::Unsupported(_))
        {
            let text = first(node, "ssl_verify_client").map_or_else(
                || "ssl_verify_client (heredado de http)".to_owned(),
                directive,
            );
            conv.degraded(
                1,
                &text,
                &context,
                Degradation::ClientCertificate,
                format!(
                    "este vhost no exige certificado de cliente y el default_server «{primary_label}» \
                     sí: el listener aplica la política mTLS del default_server a todos los nombres"
                ),
            );
        }
    }

    // Locations: one routing rule each, longest prefix first (nginx semantics).
    let mut planned: Vec<Planned> = Vec::new();
    // A redirect-only server (`return 301 …;` and no locations) becomes the
    // listener's fallback action, so it needs no backend.
    let mut server_redirect: Option<(Option<u16>, String)> = None;
    let mut server_headers: Vec<HeaderOp> = Vec::new();
    let mut forwarded = false;
    let mut cache = defaults.cache;
    // Custom error pages (B-N14 · B-E18): server-level `error_page` covers
    // every location of the block, which is EQProxy's listener scope.
    let mut error_pages: Vec<super::ErrorPageIr> = Vec::new();
    let mut intercept_errors = false;
    // Listener-scope values declared by this server. They only reach the
    // generated listener when this is the default_server; a secondary vhost's
    // values are compared with the primary's and reported as discarded.
    let mut server_client_header_timeout: Option<String> = None;
    let mut server_idle_timeout: Option<String> = None;
    let mut server_connect_timeout: Option<String> = None;
    let mut server_server_header_timeout: Option<String> = None;
    let mut server_max_body_bytes: Option<u64> = None;
    let mut server_gzip: Option<bool> = None;
    let mut server_gzip_level: Option<u32> = None;
    let mut server_gzip_min_size: Option<u64> = None;

    for child in &node.children {
        let before = conv.notes.len();
        let text = directive(child);
        match child.name.as_str() {
            "listen" | "http2" | "ssl_certificate" | "ssl_certificate_key" | "ssl_protocols"
            | "ssl_client_certificate" => conv.info(
                1,
                &text,
                &context,
                if role.is_primary() {
                    "binding y TLS del listener: aplicados arriba en [listener.*.tls]"
                } else {
                    "TLS del vhost: certificado a [listener.*.tls].certs[] y política comparada \
                     con el default_server"
                },
            ),
            // The specific TLS block above already classified this directive;
            // falling through to the generic server note would duplicate it.
            "ssl_verify_client" => continue,
            "server_name" => {
                let declared: Vec<&str> = child.args.iter().map(String::as_str).collect();
                if declared.is_empty() {
                    conv.review(1, &text, &context, "server_name vacío: se ignora");
                } else if hosts.patterns.is_empty() {
                    conv.review(
                        1,
                        &text,
                        &context,
                        format!(
                            "server_name sin forma traducible: {}",
                            if hosts.issues.is_empty() {
                                "no hay nombres válidos".to_owned()
                            } else {
                                hosts.issues.join("; ")
                            }
                        ),
                    );
                } else if !hosts.issues.is_empty() {
                    conv.review(
                        1,
                        &text,
                        &context,
                        format!(
                            "server_name traducido parcialmente ({}); {}",
                            hosts
                                .patterns
                                .iter()
                                .map(HostPattern::description)
                                .collect::<Vec<_>>()
                                .join(", "),
                            hosts.issues.join("; ")
                        ),
                    );
                } else {
                    conv.done(
                        1,
                        &text,
                        &context,
                        format!(
                            "{} en cada regla del vhost ({} nombre(s))",
                            hosts
                                .patterns
                                .iter()
                                .map(HostPattern::description)
                                .collect::<Vec<_>>()
                                .join(", "),
                            hosts.raw.len()
                        ),
                    );
                }
            }
            "add_header" => {
                let Some(name) = child.args.first() else {
                    conv.review(1, &text, &context, "add_header sin nombre: se ignora");
                    continue;
                };
                let value = child.args.get(1).cloned().unwrap_or_default();
                server_headers.push(HeaderOp {
                    name: name.clone(),
                    value,
                });
                conv.done(
                    1,
                    &text,
                    &context,
                    "[[listener.*.rules]].response_headers (se emite en la regla que cubre el listener)",
                );
            }
            "location" => {
                let (compiled, path_len) = location(
                    conv,
                    child,
                    &context,
                    &mut forwarded,
                    &server_proxy_ssl,
                    emit_rules,
                )?;
                if let Some(compiled_rule) = compiled {
                    planned.push(Planned {
                        rule: compiled_rule,
                        length: path_len,
                    });
                }
            }
            "proxy_cache" => {
                let on = child.args.first().map(String::as_str) != Some("off");
                if role.is_primary() {
                    cache = on;
                    conv.review(
                        1,
                        &text,
                        &context,
                        "[listener.*.cache] con valores por defecto (60 s, 64 MiB, tope 1 MiB): revise TTL y presupuesto",
                    );
                } else {
                    let primary = role
                        .primary_listener()
                        .map(|listener| listener.cache);
                    let mine = if on { "on" } else { "off" };
                    let primary = primary.map(|value| if value { "on" } else { "off" });
                    report_secondary_listener_knob(
                        conv,
                        &text,
                        &context,
                        role,
                        "proxy_cache (listener.cache)",
                        mine,
                        primary,
                    );
                }
            }
            "client_header_timeout" => {
                if let Some(value) = convert_duration(
                    conv,
                    first_arg(child),
                    "s",
                    &text,
                    &context,
                ) {
                    if role.is_primary() {
                        server_client_header_timeout = Some(value.clone());
                        conv.done(
                            1,
                            &text,
                            &context,
                            format!("listener.client_header_timeout = «{value}»"),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.client_header_timeout.as_deref());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "client_header_timeout (listener.client_header_timeout)",
                            &value,
                            primary,
                        );
                    }
                }
            }
            "keepalive_timeout" => {
                if let Some(value) = convert_duration(
                    conv,
                    first_arg(child),
                    "s",
                    &text,
                    &context,
                ) {
                    if role.is_primary() {
                        server_idle_timeout = Some(value.clone());
                        conv.review(
                            1,
                            &text,
                            &context,
                            format!(
                                "listener.idle_timeout = «{value}» (aproximado: en EQProxy la \
                                 inactividad es bidireccional)"
                            ),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.idle_timeout.as_deref());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "keepalive_timeout (listener.idle_timeout, aproximado)",
                            &value,
                            primary,
                        );
                    }
                }
            }
            "proxy_connect_timeout" => {
                if let Some(value) = convert_duration(
                    conv,
                    first_arg(child),
                    "s",
                    &text,
                    &context,
                ) {
                    if role.is_primary() {
                        server_connect_timeout = Some(value.clone());
                        conv.done(
                            1,
                            &text,
                            &context,
                            format!("listener.connect_timeout = «{value}»"),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.connect_timeout.as_deref());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "proxy_connect_timeout (listener.connect_timeout)",
                            &value,
                            primary,
                        );
                    }
                }
            }
            "proxy_read_timeout" => {
                if let Some(value) = convert_duration(
                    conv,
                    first_arg(child),
                    "s",
                    &text,
                    &context,
                ) {
                    if role.is_primary() {
                        server_server_header_timeout = Some(value.clone());
                        conv.review(
                            1,
                            &text,
                            &context,
                            format!(
                                "listener.server_header_timeout = «{value}» (aproximado: EQProxy \
                                 solo acota la cabecera de respuesta)"
                            ),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.server_header_timeout.as_deref());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "proxy_read_timeout (listener.server_header_timeout, aproximado)",
                            &value,
                            primary,
                        );
                    }
                }
            }
            "proxy_send_timeout" | "send_timeout" => conv.review(
                1,
                &text,
                &context,
                "sin equivalente: en EQProxy idle_timeout es el único plazo de inactividad",
            ),
            "client_max_body_size" => match parse_size(
                first_arg(child),
            ) {
                Some(size) => {
                    if role.is_primary() {
                        server_max_body_bytes = Some(size);
                        conv.done(
                            1,
                            &text,
                            &context,
                            format!("listener.limits.max_body_bytes = {size}"),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.max_body_bytes)
                            .map(|value| value.to_string());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "client_max_body_size (listener.limits.max_body_bytes)",
                            &size.to_string(),
                            primary.as_deref(),
                        );
                    }
                }
                None => conv.review(
                    1,
                    &text,
                    &context,
                    "tamaño de cuerpo no reconocido: se deja el valor del listener",
                ),
            },
            "gzip" => {
                let on = child.args.first().map(String::as_str) == Some("on");
                if role.is_primary() {
                    server_gzip = Some(on);
                    conv.done(
                        1,
                        &text,
                        &context,
                        format!("listener.compression.gzip = {on}"),
                    );
                } else {
                    let primary = role
                        .primary_listener()
                        .map(|listener| listener.compression_gzip);
                    let mine = if on { "on" } else { "off" };
                    let primary = primary.map(|value| if value { "on" } else { "off" });
                    report_secondary_listener_knob(
                        conv,
                        &text,
                        &context,
                        role,
                        "gzip (listener.compression.gzip)",
                        mine,
                        primary,
                    );
                }
            }
            "gzip_comp_level" => {
                let arg = first_arg(child);
                match arg.parse::<u32>() {
                    Ok(level) => {
                        let level = level.clamp(1, 9);
                        if role.is_primary() {
                            server_gzip_level = Some(level);
                            conv.done(
                                1,
                                &text,
                                &context,
                                format!("listener.compression.level = {level}"),
                            );
                        } else {
                            let primary = role
                                .primary_listener()
                                .and_then(|listener| listener.compression_level)
                                .map(|value| value.to_string());
                            report_secondary_listener_knob(
                                conv,
                                &text,
                                &context,
                                role,
                                "gzip_comp_level (listener.compression.level)",
                                &level.to_string(),
                                primary.as_deref(),
                            );
                        }
                    }
                    Err(_) => conv.review(
                        1,
                        &text,
                        &context,
                        "nivel de gzip no numérico: se usa el del listener",
                    ),
                }
            }
            "gzip_min_length" => match parse_size(
                first_arg(child),
            ) {
                Some(size) => {
                    if role.is_primary() {
                        server_gzip_min_size = Some(size);
                        conv.done(
                            1,
                            &text,
                            &context,
                            format!("listener.compression.min_size = {size}"),
                        );
                    } else {
                        let primary = role
                            .primary_listener()
                            .and_then(|listener| listener.compression_min_size)
                            .map(|value| value.to_string());
                        report_secondary_listener_knob(
                            conv,
                            &text,
                            &context,
                            role,
                            "gzip_min_length (listener.compression.min_size)",
                            &size.to_string(),
                            primary.as_deref(),
                        );
                    }
                }
                None => conv.review(
                    1,
                    &text,
                    &context,
                    "tamaño mínimo de gzip no reconocido: se usa el del listener",
                ),
            },
            "deny" => {
                let cidr_text = child
                    .args
                    .first().map_or_else(|| "0.0.0.0/0".to_owned(), |raw| cidr_of(raw));
                let mut deny_rule = Rule {
                    name: "deny".to_owned(),
                    direct_response: Some((403, None)),
                    ip: vec![cidr_text.clone()],
                    ..Rule::default()
                };
                // A deny rule has no backend: it answers 403 locally.
                deny_rule.use_backend = None;
                planned.push(Planned {
                    rule: deny_rule,
                    length: usize::MAX / 2,
                });
                conv.done(
                    1,
                    &text,
                    &context,
                    format!("regla [[listener.*.rules]] ip = [\"{cidr_text}\"] → 403 local"),
                );
            }
            "allow" => conv.review(
                1,
                &text,
                &context,
                "orden allow/deny de nginx: EQProxy evalúa reglas en orden, pero el conversor no construye \
                 la secuencia completa; transcríbala con ip + direct_response",
            ),
            "return"
                if role.is_primary()
                    && !node.children.iter().any(|child| {
                        matches!(child.name.as_str(), "location" | "deny")
                    }) =>
            {
                if server_redirect.is_some() {
                    conv.review(
                        1,
                        &text,
                        &context,
                        "hay más de un `return` a nivel de server: solo se convierte el primero",
                    );
                } else {
                    match server_return(child) {
                        Ok((status, location)) => {
                            conv.done(
                                1,
                                &text,
                                &context,
                                format!(
                                    "[listener.*.redirect] status = {}, location = «{location}» \
                                     (sin backend)",
                                    status.unwrap_or(301)
                                ),
                            );
                            server_redirect = Some((status, location));
                        }
                        Err(reason) => conv.review(1, &text, &context, reason),
                    }
                }
            }
            "return" => conv.review(
                1,
                &text,
                &context,
                "redirección a nivel de server con locations u otros destinos: nginx la aplica \
                 antes que las locations y EQProxy solo puede convertirla en el fallback del \
                 listener cuando el bloque entero redirige; revísela a mano",
            ),
            "auth_basic" | "auth_basic_user_file" => conv.review(
                1,
                &text,
                &context,
                "[[listener.*.rules]].auth.users (htpasswd inline): el fichero no se lee, copie las líneas",
            ),
            "error_page" => {
                let Some((target, modifiers)) = child.args.split_last() else {
                    conv.review(1, &text, &context, "error_page sin argumentos: se ignora");
                    continue;
                };
                let mut codes: Vec<u16> = Vec::new();
                let mut status = None;
                let mut invalid = None;
                for arg in modifiers {
                    if let Some(raw) = arg.strip_prefix('=') {
                        match raw.parse::<u16>() {
                            Ok(code) => status = Some(code),
                            Err(_) if raw.is_empty() => invalid = Some(
                                "error_page «=» sin código: nginx usa el estado del destino interno \
                                 y el conversor no lo resuelve; fije status a mano"
                                    .to_owned(),
                            ),
                            Err(_) => invalid = Some(format!("«{arg}» no es «=NNN»")),
                        }
                    } else if let Ok(code) = arg.parse::<u16>() {
                        codes.push(code);
                    } else {
                        invalid = Some(format!("«{arg}» no es un código de estado"));
                    }
                }
                if let Some(reason) = invalid {
                    conv.review(1, &text, &context, reason);
                    continue;
                }
                if codes.is_empty() {
                    conv.review(1, &text, &context, "error_page sin código: se ignora");
                    continue;
                }
                if target.starts_with('@') {
                    conv.gap(
                        1,
                        &text,
                        &context,
                        format!(
                            "destino «{target}»: EQProxy no tiene named locations; declare la página \
                             con [[listener.*.error_pages]]"
                        ),
                    );
                    continue;
                }
                if target.contains("://") {
                    conv.review(
                        1,
                        &text,
                        &context,
                        format!(
                            "destino externo «{target}»: nginx responde 302; use \
                             [[listener.*.rules]].redirect"
                        ),
                    );
                    continue;
                }
                // nginx resolves this URI against `root`/`alias`; EQProxy
                // resolves `file` against the TOML's directory. The leading
                // slash becomes a relative path and the note tells the
                // operator to copy the file there (a leading slash would be
                // read as an absolute filesystem path that nginx never used).
                let file = target.trim_start_matches('/');
                if file.is_empty() {
                    conv.review(
                        1,
                        &text,
                        &context,
                        "error_page apunta a «/»: no hay fichero que servir",
                    );
                    continue;
                }
                // nginx replaces a code's page when a later `error_page`
                // names it, but the other codes of the earlier directive must
                // keep their page: remove only the overlapping codes, then
                // drop pages left with none.
                for page in &mut error_pages {
                    page.codes.retain(|code| !codes.contains(code));
                }
                error_pages.retain(|page| !page.codes.is_empty());
                let page = super::ErrorPageIr {
                    codes: codes.clone(),
                    file: Some(file.to_owned()),
                    body: None,
                    location: None,
                    raw: false,
                    status,
                    intercept: false,
                };
                let list = codes
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let detail = format!(
                    "[[listener.*.error_pages]] codes = [{list}], file = \"{file}\"{}: la ruta se \
                     resuelve contra el directorio del TOML, no contra el root de nginx",
                    status.map_or_default(|s| format!(", status = {s}"))
                );
                let same_as_primary = role
                    .primary_listener()
                    .is_some_and(|primary| primary.error_pages.contains(&page));
                if role.is_primary() {
                    conv.done(1, &text, &context, detail);
                } else if same_as_primary {
                    conv.info(
                        1,
                        &text,
                        &context,
                        "misma página de error que el default_server: el listener ya la aplica \
                         a todos los nombres y no se duplica",
                    );
                } else {
                    conv.review(
                        1,
                        &text,
                        &context,
                        format!(
                            "este vhost declara una página de error; las páginas son del listener \
                             y el default_server fija las suyas, así que esta no se aplica: {detail}"
                        ),
                    );
                }
                error_pages.push(page);
            }
            "proxy_intercept_errors" => match child.args.first().map(String::as_str) {
                Some("on") => {
                    intercept_errors = true;
                    if role.is_primary() {
                        conv.done(
                            1,
                            &text,
                            &context,
                            "intercept = true en las páginas error_page de este server (también \
                             sustituyen respuestas del backend)",
                        );
                    } else {
                        conv.review(
                            1,
                            &text,
                            &context,
                            "el flag es del listener: el default_server decide si sus páginas \
                             interceptan respuestas del backend; el on de este vhost no se aplica",
                        );
                    }
                }
                Some("off") => conv.info(
                    1,
                    &text,
                    &context,
                    "errores del backend pasan tal cual: las páginas solo cubren códigos locales",
                ),
                _ => conv.review(
                    1,
                    &text,
                    &context,
                    "proxy_intercept_errors sin on/off: se trata como off",
                ),
            },
            "gzip_types" | "gzip_vary" | "gzip_proxied" | "gzip_static" => conv.review(
                1,
                &text,
                &context,
                "la compresión se configura por listener: mueva la directiva al bloque http o a [listener.*.compression]",
            ),
            "ssl_ciphers" => {
                let value = child.args.join(" ");
                match role {
                    VhostRole::Primary => conv.review(
                        1,
                        &text,
                        &context,
                        "selección de cipher suites: fuera por decisión (ADR-0035)",
                    ),
                    VhostRole::Secondary {
                        primary_ciphers,
                        primary_label,
                        ..
                    } if *primary_ciphers == Some(value.as_str()) => conv.info(
                        1,
                        &text,
                        &context,
                        format!(
                            "mismos cifrados que el default_server «{primary_label}» (EQProxy usa \
                             su propia política)"
                        ),
                    ),
                    VhostRole::Secondary { primary_label, .. } => conv.degraded(
                        1,
                        &text,
                        &context,
                        Degradation::CipherPolicy,
                        format!(
                            "este vhost declara otros cifrados que el default_server \
                             «{primary_label}»; el listener no puede llevar una política por vhost \
                             y EQProxy usa la suya (ADR-0035)"
                        ),
                    ),
                }
            }
            // N-2: server-scope proxy_ssl_* is inherited by every location
            // below it; the values were collected before the loop and the
            // location applies them. This row classifies the source directive.
            name if name.starts_with("proxy_ssl_") => {
                classify_proxy_ssl(conv, child, &context, has_https_targets);
            }
            name => match supported_manually(name) {
                Some(place) => conv.review(
                    1,
                    &text,
                    &context,
                    format!("EQProxy sí lo soporta en {place}: transcríbalo a mano"),
                ),
                None => match unsupported(name) {
                    Some(reason) => conv.gap(1, &text, &context, reason),
                    None => conv.review(
                        1,
                        &text,
                        &context,
                        "directiva de server sin traducción automática: revísela a mano",
                    ),
                },
            },
        }
        conv.touch(&directive(child), &context, before)?;
    }

    if planned.is_empty() && server_redirect.is_none() {
        conv.reject(
            1,
            "location",
            &context,
            "el bloque no tiene ninguna location con proxy_pass ni deny ni un return de \
             redirección: EQProxy no sirve contenido estático",
        );
        return Ok(None);
    }

    // Longest prefix first, denies (huge length) before anything else.
    planned.sort_by_key(|p| std::cmp::Reverse(p.length));

    // The fallback backend is the one covering "/" (or the first rule).
    let fallback = planned
        .iter()
        .find(|p| p.rule.path_prefix.as_deref() == Some("/"))
        .or_else(|| planned.first())
        .and_then(|p| p.rule.use_backend.clone())
        .or_else(|| planned.iter().find_map(|p| p.rule.use_backend.clone()));

    let mut rules: Vec<Rule> = planned.into_iter().map(|p| p.rule).collect();

    if !server_headers.is_empty() {
        let target = rules.iter_mut().find(|rule| rule.use_backend.is_some());
        match target {
            Some(found) => {
                found.response_headers.extend(server_headers);
                conv.review(
                    1,
                    "add_header",
                    &context,
                    "las cabeceras se emiten en una regla: las peticiones que no casen con ninguna regla no las llevan",
                );
            }
            None => conv.review(
                1,
                "add_header",
                &context,
                "sin ninguna regla reenviada no hay dónde colgar la cabecera: confígurela a mano",
            ),
        }
    }

    // Every rule carries one condition per vhost name, so a shared listener
    // keeps the vhosts distinguishable. The copies of a single vhost would
    // otherwise repeat the same rule name; qualify it with the pattern so
    // names stay unique for logs, metrics and `explain`.
    if !hosts.patterns.is_empty() {
        let qualify_names = hosts.patterns.len() > 1;
        rules = hosts
            .patterns
            .iter()
            .flat_map(|pattern| {
                rules.iter().cloned().map(move |mut rule| {
                    if qualify_names {
                        rule.name = format!("{} ({})", rule.name, pattern.description());
                    }
                    pattern.apply(&mut rule);
                    rule
                })
            })
            .collect();
    } else if !role.is_primary() || !hosts.raw.is_empty() {
        // A vhost that declared names but none is representable must not
        // become a catch-all. The default vhost without any server_name is
        // the one case where a hostless rule is the intended fallback.
        rules.clear();
    }

    if fallback.is_none() && server_redirect.is_none() {
        conv.reject(
            1,
            &context,
            &context,
            "ninguna location apunta a un backend traducible y el bloque no redirige: listener omitido",
        );
        return Ok(None);
    }

    let name = conv.free_listener_name(&format!("web_{label}"));
    let mut listener = if let Some(backend) = fallback {
        Listener::new(addr, "http", backend)
    } else {
        let (status, location) = server_redirect
            .clone()
            .expect("validated above: no backend means a redirect");
        Listener::redirect(addr, "http", status, location)
    };
    listener.maxconn = 1024;
    listener.tls = tls;
    listener.quic = http3 && listener.tls.is_some();
    listener.compression_gzip = defaults.gzip;
    listener.compression_level = defaults.gzip_level;
    listener.compression_min_size = defaults.gzip_min_size;
    listener.max_body_bytes = defaults.max_body_bytes;
    listener.idle_timeout.clone_from(&defaults.idle_timeout);
    listener
        .connect_timeout
        .clone_from(&defaults.connect_timeout);
    listener
        .server_header_timeout
        .clone_from(&defaults.server_header_timeout);
    listener
        .client_header_timeout
        .clone_from(&defaults.client_header_timeout);
    listener.cache = cache;
    if intercept_errors {
        // nginx applies the flag to every error_page of the server, whatever
        // the order of the directives.
        for page in &mut error_pages {
            page.intercept = true;
        }
    }
    listener.error_pages = error_pages;
    if forwarded {
        listener.forwarded_headers = Some("append");
    }
    // Per-server listener-scope values win over the http-level defaults, but
    // only for the default_server. A secondary vhost's values were compared
    // with this listener above and are deliberately not copied here: ADR-0051
    // gives the listener one value, the default_server's.
    if role.is_primary() {
        if let Some(value) = server_client_header_timeout {
            listener.client_header_timeout = Some(value);
        }
        if let Some(value) = server_idle_timeout {
            listener.idle_timeout = Some(value);
        }
        if let Some(value) = server_connect_timeout {
            listener.connect_timeout = Some(value);
        }
        if let Some(value) = server_server_header_timeout {
            listener.server_header_timeout = Some(value);
        }
        if let Some(value) = server_max_body_bytes {
            listener.max_body_bytes = Some(value);
        }
        if let Some(value) = server_gzip {
            listener.compression_gzip = value;
        }
        if let Some(value) = server_gzip_level {
            listener.compression_level = Some(value);
        }
        if let Some(value) = server_gzip_min_size {
            listener.compression_min_size = Some(value);
        }
    }
    listener.rules = rules;
    if http2 {
        conv.info(
            1,
            "http2",
            &context,
            "ALPN h2 en [listener.*.tls] (HTTP/2 nativo)",
        );
    }
    if http3 {
        conv.info(
            1,
            "http3/quic",
            &context,
            "[listener.*].quic = true (HTTP/3 sobre QUIC en el mismo puerto)",
        );
    }
    Ok(Some((name, listener)))
}

/// One `location { … }`.
#[allow(clippy::too_many_lines)]
fn location(
    conv: &mut Conv,
    node: &Node,
    server_context: &str,
    forwarded: &mut bool,
    inherited_ssl: &ProxySslDefaults,
    emit_rules: bool,
) -> Result<(Option<Rule>, usize), ImportError> {
    let raw_path = node.args.first().cloned().unwrap_or_else(|| "/".to_owned());
    let context = format!("{server_context} location {raw_path}");
    let (path, exact) = if let Some(rest) = raw_path.strip_prefix('=') {
        (rest.trim().to_owned(), true)
    } else if raw_path.starts_with('~') || raw_path.starts_with("^~") {
        conv.gap(
            1,
            format!("location {raw_path}"),
            &context,
            "location por expresión regular: EQProxy solo tiene path y path_prefix literales \
             ([[listener.*.rules]].path_regex existe, pero el conversor no traslada la expresión)",
        );
        return Ok((None, 0));
    } else {
        (raw_path.clone(), false)
    };

    let mut rule = Rule {
        name: context.clone(),
        ..Rule::default()
    };
    if exact {
        rule.path = Some(path.clone());
    } else {
        rule.path_prefix = Some(path.clone());
    }
    let mut length = path.len();
    let mut has_target = false;
    // N-2: whether the backend connection is TLS decides what a `proxy_ssl_*`
    // directive means, and nginx accepts those before or after `proxy_pass`,
    // so the target is read first. `false` covers a plaintext target, a
    // variable and no `proxy_pass`: there is no TLS block to qualify.
    let tls_target = node
        .children
        .iter()
        .find(|child| child.name == "proxy_pass")
        .and_then(|child| child.args.first())
        .filter(|target| !target.contains('$'))
        .and_then(|target| target.split_once("://"))
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"));

    // EXT-H-04: an `https://` target and the `proxy_ssl_*` directives that
    // qualify it are collected here and applied after the loop, because nginx
    // accepts them before or after `proxy_pass`. The tuple keeps the host (the
    // SNI source) and the directive text (the report row) of that target.
    let mut https_target: Option<(String, String)> = None;
    // Nginx inheritance is per directive: start with the http/server values
    // and let a location declaration replace only its own field.
    let mut ssl_verify = inherited_ssl.verify;
    let mut ssl_ca = inherited_ssl.ca_file.clone();
    let mut ssl_sni = inherited_ssl.sni;
    let mut ssl_name = inherited_ssl.server_name.clone();
    // `proxy_ssl_server_name on` only reaches the TOML when a DNS name is
    // available; an IP literal never becomes `server_name`. Keep the note so
    // the final backend TLS can turn the provisional `Done` back into `Info`
    // instead of promising a field that is not there.
    let mut proxy_ssl_sni_note = None;

    for child in &node.children {
        let before = conv.notes.len();
        let text = directive(child);
        match child.name.as_str() {
            "proxy_pass" => {
                let Some(target) = child.args.first() else {
                    conv.review(1, &text, &context, "proxy_pass vacío: se ignora");
                    continue;
                };
                if target.contains('$') {
                    conv.review(
                        1,
                        &text,
                        &context,
                        "proxy_pass con variables: sin soporte (EQProxy no evalúa variables)",
                    );
                    continue;
                }
                // The scheme is a security property, not decoration: dropping
                // it turned nginx's TLS to the origin into plaintext with no
                // word in the report (EXT-H-04).
                let (scheme, without_scheme) = match target.split_once("://") {
                    Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
                    None => (None, target.as_str()),
                };
                let https = match scheme.as_deref() {
                    Some("https") => true,
                    Some("http") | None => false,
                    Some(other) => {
                        conv.review(
                            1,
                            &text,
                            &context,
                            format!(
                                "esquema «{other}://» sin equivalente: la regla no se emite \
                                 (EQProxy habla HTTP/1.1, HTTP/2 y HTTP/3 hacia el backend)"
                            ),
                        );
                        continue;
                    }
                };
                // ADR-0043: `http://unix:/run/app.sock` (optionally followed
                // by `:/uri`) is one target, not an authority plus a path.
                let (authority, uri) = match without_scheme.strip_prefix("unix:") {
                    Some(rest) => match rest.find(":/") {
                        Some(pos) => (
                            format!("unix:{}", &rest[..pos]),
                            Some(rest[pos + 1..].to_owned()),
                        ),
                        None => (format!("unix:{rest}"), None),
                    },
                    None => match without_scheme.split_once('/') {
                        Some((authority, uri)) => (authority.to_owned(), Some(uri.to_owned())),
                        None => (without_scheme.to_owned(), None),
                    },
                };
                if let Some(uri) = uri.as_deref() {
                    conv.review(
                        1,
                        text.clone(),
                        &context,
                        format!(
                            "proxy_pass con URI («/{uri}»): use [[listener.*.rules]].rewrite.path_prefix_rewrite"
                        ),
                    );
                }
                let backend_name =
                    resolve_backend(conv, &authority, if https { 443 } else { 80 }, &context);
                if let Some(backend_name) = backend_name {
                    if https {
                        https_target = Some((authority.clone(), text.clone()));
                    }
                    rule.use_backend = Some(backend_name.clone());
                    has_target = true;
                    if emit_rules {
                        conv.done(
                            1,
                            &text,
                            &context,
                            format!("regla con use_backend = «{backend_name}»"),
                        );
                    } else {
                        conv.review(
                            1,
                            &text,
                            &context,
                            format!(
                                "la regla con use_backend = «{backend_name}» no se emite: vhost sin \
                                 server_name exacto; los hosts desconocidos siguen al default_server"
                            ),
                        );
                    }
                }
            }
            // EXT-H-04/N-2: the TLS qualifiers of `proxy_pass https://…`.
            "proxy_ssl_verify" => match proxy_ssl_toggle(child) {
                Some(on) => {
                    ssl_verify = Some(on);
                    tls_mapped(conv, &text, &context, tls_target, format!("verify = {on}"));
                }
                None => conv.review(
                    1,
                    &text,
                    &context,
                    "proxy_ssl_verify debe ser on u off: se ignora",
                ),
            },
            "proxy_ssl_trusted_certificate" => match proxy_ssl_path(child) {
                Some(path) => {
                    tls_mapped(
                        conv,
                        &text,
                        &context,
                        tls_target,
                        format!("ca_file = «{path}»"),
                    );
                    ssl_ca = Some(path);
                }
                None => conv.review(
                    1,
                    &text,
                    &context,
                    "proxy_ssl_trusted_certificate sin ruta: se ignora",
                ),
            },
            "proxy_ssl_server_name" => match proxy_ssl_toggle(child) {
                Some(true) => {
                    ssl_sni = Some(true);
                    tls_mapped(
                        conv,
                        &text,
                        &context,
                        tls_target,
                        "SNI hacia el origen: true".to_owned(),
                    );
                    proxy_ssl_sni_note = Some(conv.notes.len().saturating_sub(1));
                }
                Some(false) => {
                    ssl_sni = Some(false);
                    conv.info(
                        1,
                        &text,
                        &context,
                        "SNI desactivado: EQProxy no emite server_name (sin SNI es el valor por defecto)",
                    );
                }
                None => conv.review(
                    1,
                    &text,
                    &context,
                    "proxy_ssl_server_name debe ser on u off: se ignora",
                ),
            },
            "proxy_ssl_name" => match proxy_ssl_path(child) {
                Some(name) => {
                    tls_mapped(
                        conv,
                        &text,
                        &context,
                        tls_target,
                        format!("server_name = «{name}»"),
                    );
                    ssl_name = Some(name);
                }
                None => conv.review(1, &text, &context, "proxy_ssl_name vacío: se ignora"),
            },
            // N-2: TLS policy the schema cannot express. On a TLS target the
            // backend still gets `[backend.*.tls]`, so losing the client
            // identity, the version policy, the ciphers or the chain depth is
            // a degradation — never a conversion.
            "proxy_ssl_certificate" | "proxy_ssl_certificate_key" => tls_lost(
                conv,
                &text,
                &context,
                tls_target,
                Degradation::ClientCertificate,
            ),
            "proxy_ssl_protocols" => tls_lost(
                conv,
                &text,
                &context,
                tls_target,
                Degradation::ProtocolPolicy,
            ),
            "proxy_ssl_ciphers" => tls_lost(
                conv,
                &text,
                &context,
                tls_target,
                Degradation::CipherPolicy,
            ),
            "proxy_ssl_verify_depth" => tls_lost(
                conv,
                &text,
                &context,
                tls_target,
                Degradation::VerifyDepth,
            ),
            "error_page" | "proxy_intercept_errors" => conv.review(
                1,
                &text,
                &context,
                "página de error por location: EQProxy las declara por listener y cubren todas las \
                 rutas; muévala al bloque server o a [[listener.*.error_pages]] a mano",
            ),
            "proxy_set_header" => {
                let (Some(name), Some(value)) = (child.args.first(), child.args.get(1)) else {
                    conv.review(1, &text, &context, "proxy_set_header incompleto: se ignora");
                    continue;
                };
                if value.contains("$proxy_add_x_forwarded_for") {
                    *forwarded = true;
                    conv.review(
                        1,
                        &text,
                        &context,
                        "«$proxy_add_x_forwarded_for» → [listener.*].forwarded_headers = \"append\" \
                         (EQProxy añade la IP de cliente a X-Forwarded-For)",
                    );
                    continue;
                }
                if let Some(bad) = nginx_variable_unsupported(value) {
                    conv.review(
                        1,
                        &text,
                        &context,
                        format!("variable «{bad}» sin soporte en plantillas de cabecera: no se emite"),
                    );
                    continue;
                }
                rule.set_headers.push(HeaderOp {
                    name: name.clone(),
                    value: value.clone(),
                });
                conv.done(
                    1,
                    &text,
                    &context,
                    format!("set_headers += {{ {name}, {value} }}"),
                );
            }
            "proxy_hide_header" | "proxy_pass_header" => conv.review(
                1,
                &text,
                &context,
                "[[listener.*.rules]].del_headers cubre quitar cabeceras de la respuesta; el conversor no la emite",
            ),
            "proxy_connect_timeout" | "proxy_read_timeout" | "proxy_send_timeout"
            | "client_header_timeout" | "keepalive_timeout" => conv.review(
                1,
                &text,
                &context,
                "el timeout es de ámbito listener: el valor de esta location no se aplica al \
                 listener generado; configúrelo en el bloque server o en [listener.*] a mano",
            ),
            "deny" => {
                let cidr_text = child
                    .args
                    .first().map_or_else(|| "0.0.0.0/0".to_owned(), |raw| cidr_of(raw));
                conv.review(
                    1,
                    &text,
                    &context,
                    format!(
                        "deny dentro de location: emita una regla ip = [\"{cidr_text}\"] con direct_response 403 \
                         antes que esta"
                    ),
                );
            }
            "add_header" => {
                if let (Some(name), Some(value)) = (child.args.first(), child.args.get(1)) {
                    rule.response_headers.push(HeaderOp {
                        name: name.clone(),
                        value: value.clone(),
                    });
                    conv.done(
                        1,
                        &text,
                        &context,
                        format!("response_headers += {{ {name}, {value} }}"),
                    );
                } else {
                    conv.review(1, &text, &context, "add_header incompleto: se ignora");
                }
            }
            "limit_req" | "limit_conn" => conv.review(
                1,
                &text,
                &context,
                "[[listener.*.rules]].rate_limit (rate/per/burst) por IP: transcríbalo a mano",
            ),
            "proxy_cache" => conv.review(
                1,
                &text,
                &context,
                "[listener.*.cache] con valores por defecto: revise TTL y presupuesto",
            ),
            // Any unknown `proxy_ssl_*` gets its own row here too: the generic
            // location fallback would hide that it is a TLS policy the
            // converter cannot map, and a later reader might mistake it for a
            // harmless nginx directive.
            name if name.starts_with("proxy_ssl_") => {
                unknown_proxy_ssl(conv, &text, &context, name);
            }
            name => match supported_manually(name) {
                Some(place) => conv.review(
                    1,
                    &text,
                    &context,
                    format!("EQProxy sí lo soporta en {place}: transcríbalo a mano"),
                ),
                None => match unsupported(name) {
                    Some(reason) => conv.gap(1, &text, &context, reason),
                    None => conv.review(
                        1,
                        &text,
                        &context,
                        "directiva de location sin traducción automática: revísela a mano",
                    ),
                },
            },
        }
        conv.touch(&directive(child), &context, before)?;
    }

    if let (Some((authority, text)), Some(name)) = (&https_target, &rule.use_backend) {
        apply_backend_tls(
            conv,
            name,
            &context,
            text,
            BackendTlsOptions {
                authority: Some(authority),
                verify: ssl_verify,
                ca_file: ssl_ca,
                sni: ssl_sni,
                server_name: ssl_name,
            },
        );
    }

    // The provisional `proxy_ssl_server_name on` row is `Done` only when the
    // final backend TLS carries a `server_name`. `proxy_ssl_name` overrides the
    // authority; an IP authority has no DNS name, so the directive has no
    // field to fill and must read as `Info` (nginx itself has no effect there).
    if let Some(index) = proxy_ssl_sni_note
        && conv
            .notes
            .get(index)
            .is_some_and(|note| note.severity == Severity::Done)
    {
        let has_server_name = rule
            .use_backend
            .as_deref()
            .and_then(|backend| conv.backends.get(backend))
            .and_then(|backend| backend.tls.as_ref())
            .and_then(|tls| tls.server_name.as_ref())
            .is_some();
        if !has_server_name && let Some(note) = conv.notes.get_mut(index) {
            note.severity = Severity::Info;
            "SNI solicitado sin nombre DNS (destino IP o sin proxy_pass https): \
                            no se emite server_name"
                .clone_into(&mut note.message);
        }
    }

    if !has_target {
        return Ok((None, length.max(1)));
    }
    length = length.max(1);
    Ok((Some(rule), length))
}

/// Resolves a `proxy_pass` authority: an upstream name, or a literal address
/// that becomes a single-server backend.
fn resolve_backend(
    conv: &mut Conv,
    authority: &str,
    default_port: u16,
    context: &str,
) -> Option<String> {
    let cleaned = sanitize_name(authority);
    if conv.has_backend(&cleaned) {
        return Some(cleaned);
    }
    if conv.has_backend(authority) {
        return Some(authority.to_owned());
    }
    let addr = normalize_addr(
        conv,
        authority,
        Some(default_port),
        &format!("proxy_pass {authority}"),
        context,
        true,
    )?;
    let name = conv.free_backend_name(&format!("backend_{}", sanitize_name(authority)));
    let mut backend = super::Backend::default();
    backend.servers.push(super::Server {
        addr: addr.clone(),
        weight: 1,
        backup: false,
        maxconn: 0,
    });
    conv.add_backend(name.clone(), backend);
    conv.done(
        1,
        format!("proxy_pass {authority}"),
        context,
        format!("origen literal convertido en backend «{name}» ({addr})"),
    );
    Some(name)
}

/// The boolean value of `proxy_ssl_verify` / `proxy_ssl_server_name`.
///
/// `None` is an invalid value, not nginx's off default: treating a typo as
/// `off` would silently weaken the conversion.
fn proxy_ssl_toggle(node: &Node) -> Option<bool> {
    match node.args.first().map(|arg| arg.trim()) {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    }
}

/// A `proxy_ssl_trusted_certificate` path or a `proxy_ssl_name` value.
/// Empty is invalid, so the caller reports it instead of emitting an empty
/// field.
fn proxy_ssl_path(node: &Node) -> Option<String> {
    let raw = node.args.first()?.trim_matches(['"', '\'']);
    (!raw.is_empty()).then(|| raw.to_owned())
}

/// Collects the `proxy_ssl_*` values declared directly in `node`, starting
/// from the values inherited from its parent scope. nginx replaces a value
/// only when the same directive is declared at the narrower level; keeping
/// each field optional preserves that rule.
fn collect_proxy_ssl(node: &Node, inherited: &ProxySslDefaults) -> ProxySslDefaults {
    let mut out = inherited.clone();
    for child in &node.children {
        match child.name.as_str() {
            "proxy_ssl_verify" => {
                if let Some(on) = proxy_ssl_toggle(child) {
                    out.verify = Some(on);
                }
            }
            "proxy_ssl_trusted_certificate" => {
                if let Some(path) = proxy_ssl_path(child) {
                    out.ca_file = Some(path);
                }
            }
            "proxy_ssl_server_name" => {
                if let Some(on) = proxy_ssl_toggle(child) {
                    out.sni = Some(on);
                }
            }
            "proxy_ssl_name" => {
                if let Some(name) = proxy_ssl_path(child) {
                    out.server_name = Some(name);
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether this block (or any nested server/location) has a literal
/// `proxy_pass https://…`. A variable or plaintext target does not qualify.
fn has_https_target(node: &Node) -> bool {
    node.children.iter().any(|child| {
        let https = child.name == "proxy_pass"
            && child.args.first().is_some_and(|target| {
                !target.contains('$')
                    && target
                        .split_once("://")
                        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"))
            });
        https || has_https_target(child)
    })
}

/// Reads one listener TLS value from a directive's first argument.
///
/// Nginx inherits each directive independently, so the caller must keep a
/// missing value distinct from an empty one; the report uses this to explain
/// what was actually inherited.
fn tls_arg(node: &Node, name: &str) -> Option<String> {
    first(node, name)
        .and_then(|directive| directive.args.first())
        .filter(|value| !value.is_empty())
        .cloned()
}

/// Collects the listener TLS values declared directly in the `http` block.
/// Only directives that are expressible on `[listener.*.tls]` are kept; a
/// server can override each one independently.
fn collect_http_tls(http: &Node) -> HttpTlsDefaults {
    HttpTlsDefaults {
        cert: tls_arg(http, "ssl_certificate"),
        key: tls_arg(http, "ssl_certificate_key"),
        protocols: first(http, "ssl_protocols").map(|node| node.args.clone()),
        client_ca: tls_arg(http, "ssl_client_certificate"),
        verify_client: tls_arg(http, "ssl_verify_client"),
    }
}

/// Whether a server in the `http` block has a TLS listener (or inherits a
/// certificate from http). Used only to classify the http-scope directives in
/// the report: a TLS value with no TLS listener changes nothing.
fn has_tls_server(http: &Node, defaults: &HttpTlsDefaults) -> bool {
    let inherited_cert = defaults.cert.is_some() || defaults.key.is_some();
    inherited_cert
        || children(http, "server").iter().any(|server| {
            first(server, "ssl_certificate").is_some()
                || first(server, "ssl_certificate_key").is_some()
                || children(server, "listen").iter().any(|listen| {
                    listen.args.iter().skip(1).any(|flag| {
                        flag == "ssl"
                            || flag == "quic"
                            || flag.starts_with("http3")
                            || flag.starts_with("quic")
                    })
                })
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientVerifyMode {
    Off,
    Required,
    Unsupported(&'static str),
}

/// Only `on` has an exact `EQProxy` equivalent: `client_ca` always requires a
/// client certificate. nginx's optional modes cannot be represented without
/// either dropping verification or making the certificate mandatory.
fn client_verify_mode(value: Option<&str>) -> ClientVerifyMode {
    match value {
        Some("on") => ClientVerifyMode::Required,
        Some("optional") => ClientVerifyMode::Unsupported("optional"),
        Some("optional_no_ca") => ClientVerifyMode::Unsupported("optional_no_ca"),
        _ => ClientVerifyMode::Off,
    }
}

fn client_verify_active(value: Option<&str>) -> bool {
    matches!(client_verify_mode(value), ClientVerifyMode::Required)
}

fn reject_optional_client_verify(conv: &mut Conv, text: &str, context: &str, mode: &'static str) {
    conv.reject(
        1,
        text,
        context,
        format!(
            "modo {mode} no es expresable: EQProxy exige un certificado si configura \
             client_ca y nginx lo pedía solo como opcional; se rechaza para no endurecer la \
             política"
        ),
    );
}

/// Whether http-scope client verification becomes active in any server of the
/// block, either through http inheritance or a server override.
fn http_client_verify_active(http: &Node, defaults: &HttpTlsDefaults) -> bool {
    client_verify_active(defaults.verify_client.as_deref())
        || children(http, "server")
            .iter()
            .any(|server| client_verify_active(tls_arg(server, "ssl_verify_client").as_deref()))
}

/// Whether a client CA is available at http scope or in any server.
fn http_has_client_ca(http: &Node, defaults: &HttpTlsDefaults) -> bool {
    defaults.client_ca.is_some()
        || children(http, "server")
            .iter()
            .any(|server| tls_arg(server, "ssl_client_certificate").is_some())
}

/// nginx's `ssl_protocols` expressed as the schema's min/max pair. `None`
/// means the directive names neither TLS 1.2 nor 1.3; the bool reports a
/// legacy protocol, which `EQProxy` ignores.
type TlsProtocolRange = Option<(Option<&'static str>, Option<&'static str>)>;

fn tls_protocol_range(args: &[String]) -> (TlsProtocolRange, bool) {
    let has = |wanted: &str| args.iter().any(|arg| arg.eq_ignore_ascii_case(wanted));
    let has_12 = has("TLSv1.2");
    let has_13 = has("TLSv1.3");
    let legacy = ["TLSv1.1", "TLSv1", "SSLv3", "SSLv2"]
        .iter()
        .any(|name| has(name));
    let range = match (has_12, has_13) {
        (true, true) => Some((Some("1.2"), None)),
        (true, false) => Some((Some("1.2"), Some("1.2"))),
        (false, true) => Some((Some("1.3"), Some("1.3"))),
        (false, false) => None,
    };
    (range, legacy)
}

/// Classifies one `proxy_ssl_*` directive declared at http or server scope.
///
/// The value has already been collected for inheritance; this function only
/// writes the report row. A supported field is `Done` when some HTTPS target
/// in the scope can use it; an unexpressible TLS property is `Degraded`, never
/// a silent conversion. An unknown `proxy_ssl_*` stays a review note because
/// this converter cannot claim whether it relaxed security.
fn classify_proxy_ssl(conv: &mut Conv, child: &Node, context: &str, applies: bool) {
    let text = directive(child);
    match child.name.as_str() {
        "proxy_ssl_verify" => match proxy_ssl_toggle(child) {
            Some(on) => {
                proxy_ssl_scope_mapped(conv, &text, context, applies, &format!("verify = {on}"));
            }
            None => conv.review(
                1,
                &text,
                context,
                "proxy_ssl_verify debe ser on u off: se ignora",
            ),
        },
        "proxy_ssl_trusted_certificate" => match proxy_ssl_path(child) {
            Some(path) => {
                proxy_ssl_scope_mapped(
                    conv,
                    &text,
                    context,
                    applies,
                    &format!("ca_file = «{path}»"),
                );
            }
            None => conv.review(
                1,
                &text,
                context,
                "proxy_ssl_trusted_certificate sin ruta: se ignora",
            ),
        },
        "proxy_ssl_server_name" => match proxy_ssl_toggle(child) {
            Some(true) => {
                proxy_ssl_scope_mapped(conv, &text, context, applies, "SNI hacia el origen: true");
            }
            Some(false) => conv.info(
                1,
                &text,
                context,
                "SNI desactivado: EQProxy no emite server_name (sin SNI es el valor por defecto)",
            ),
            None => conv.review(
                1,
                &text,
                context,
                "proxy_ssl_server_name debe ser on u off: se ignora",
            ),
        },
        "proxy_ssl_name" => match proxy_ssl_path(child) {
            Some(name) => proxy_ssl_scope_mapped(
                conv,
                &text,
                context,
                applies,
                &format!("server_name = «{name}»"),
            ),
            None => conv.review(1, &text, context, "proxy_ssl_name vacío: se ignora"),
        },
        "proxy_ssl_certificate" | "proxy_ssl_certificate_key" => proxy_ssl_scope_lost(
            conv,
            &text,
            context,
            applies,
            Degradation::ClientCertificate,
        ),
        "proxy_ssl_protocols" => {
            proxy_ssl_scope_lost(conv, &text, context, applies, Degradation::ProtocolPolicy);
        }
        "proxy_ssl_ciphers" => {
            proxy_ssl_scope_lost(conv, &text, context, applies, Degradation::CipherPolicy);
        }
        "proxy_ssl_verify_depth" => {
            proxy_ssl_scope_lost(conv, &text, context, applies, Degradation::VerifyDepth);
        }
        name => unknown_proxy_ssl(conv, &text, context, name),
    }
}

/// One explicit row for a `proxy_ssl_*` this importer does not know.
///
/// The fallback must name the directive family and say that no backend TLS
/// field is emitted: a generic location/upstream note would let an operator
/// assume the TLS qualifier was either translated or harmless.
fn unknown_proxy_ssl(conv: &mut Conv, text: &str, context: &str, name: &str) {
    conv.review(
        1,
        text,
        context,
        format!(
            "directiva proxy_ssl_* desconocida «{name}»: sin traducción automática y sin campo \
             [backend.*.tls] que la represente; revísela a mano"
        ),
    );
}

fn proxy_ssl_scope_mapped(conv: &mut Conv, text: &str, context: &str, applies: bool, mapped: &str) {
    if applies {
        conv.done(
            1,
            text,
            context,
            format!("{mapped} (TLS al backend; se hereda a las locations que no lo sobrescriban)"),
        );
    } else {
        conv.info(
            1,
            text,
            context,
            "sin proxy_pass https://… en este bloque: no aplica al backend",
        );
    }
}

fn proxy_ssl_scope_lost(
    conv: &mut Conv,
    text: &str,
    context: &str,
    applies: bool,
    lost: Degradation,
) {
    if applies {
        conv.degraded(1, text, context, lost, "no llega a [backend.*.tls]");
    } else {
        conv.info(
            1,
            text,
            context,
            "sin proxy_pass https://… en este bloque: no aplica al backend",
        );
    }
}

/// N-2: a `proxy_ssl_*` directive only means something when the location
/// proxies over TLS. On a plaintext (or unresolvable) target nothing is lost,
/// so the row says that instead of claiming a conversion. Returns whether the
/// directive reaches `[backend.*.tls]`.
fn tls_applies(conv: &mut Conv, text: &str, context: &str, tls_target: bool) -> bool {
    if tls_target {
        return true;
    }
    conv.info(
        1,
        text,
        context,
        "sin destino https literal en la location: la directiva no llega al TOML",
    );
    false
}

/// EXT-H-04: a qualifier `[backend.*.tls]` can hold as written.
fn tls_mapped(conv: &mut Conv, text: &str, context: &str, tls_target: bool, mapped: String) {
    if tls_applies(conv, text, context, tls_target) {
        conv.done(1, text, context, mapped);
    }
}

/// N-2: a policy the schema cannot express. TLS is still emitted for the
/// backend, so losing it is a degradation, never a conversion. The row already
/// carries the directive and its value verbatim, so the summary groups by the
/// lost property instead of repeating the value in every message.
fn tls_lost(conv: &mut Conv, text: &str, context: &str, tls_target: bool, lost: Degradation) {
    if tls_applies(conv, text, context, tls_target) {
        conv.degraded(1, text, context, lost, "no llega a [backend.*.tls]");
    }
}

/// The TLS a location declares for its backend, gathered from
/// `proxy_pass https://…` and the `proxy_ssl_*` directives that qualify it.
/// `None` means the directive was absent, which is not the same as nginx's
/// default `off`: the difference is what the idempotency check compares.
#[derive(Default)]
struct BackendTlsOptions<'a> {
    /// Host of the `proxy_pass` authority; it is the SNI when the location sets
    /// `proxy_ssl_server_name on` and no `proxy_ssl_name`.
    authority: Option<&'a str>,
    /// `proxy_ssl_verify` (nginx default: off).
    verify: Option<bool>,
    /// `proxy_ssl_trusted_certificate`.
    ca_file: Option<String>,
    /// `proxy_ssl_server_name` (nginx default: off, no SNI).
    sni: Option<bool>,
    /// `proxy_ssl_name`.
    server_name: Option<String>,
}

/// EXT-H-04: a `proxy_pass https://…` target keeps TLS. nginx verifies
/// nothing and sends no SNI by default; `proxy_ssl_verify`,
/// `proxy_ssl_trusted_certificate`, `proxy_ssl_server_name` and
/// `proxy_ssl_name` turn either on, so exactly those reach
/// `[backend.<name>.tls]` and nothing is invented.
fn apply_backend_tls(
    conv: &mut Conv,
    backend: &str,
    context: &str,
    directive: &str,
    tls: BackendTlsOptions<'_>,
) {
    let BackendTlsOptions {
        authority,
        verify,
        ca_file,
        sni,
        server_name,
    } = tls;
    let verify = verify.unwrap_or(false);
    let server_name = server_name.or_else(|| {
        if sni.unwrap_or(false) {
            authority.and_then(sni_host).map(str::to_owned)
        } else {
            None
        }
    });
    // A backend shared by several locations carries one TLS combination. The
    // same one stays silent (several paths to one origin is the normal shape);
    // only a real divergence is reported, because the model cannot hold two
    // proxy_ssl_* sets for one backend. `None` = no TLS yet, `Some(true)` = the
    // same combination, `Some(false)` = a divergence. Compared by reference, so
    // the check costs no allocations.
    let existing = conv.backend_mut(backend).and_then(|target| {
        target.tls.as_ref().map(|tls| {
            tls.verify == verify && tls.ca_file == ca_file && tls.server_name == server_name
        })
    });
    match existing {
        Some(true) => return,
        Some(false) => {
            // N-2: one backend holds one TLS identity, so the second location
            // silently inherits the first one's — it verifies a name its own
            // source never declared. That is a relaxation, not a review note.
            conv.degraded(
                1,
                directive,
                context,
                Degradation::IdentityRelaxed,
                format!(
                    "el backend «{backend}» ya declara [backend.{backend}.tls] con otros valores: \
                     revise qué combinación aplica a cada location"
                ),
            );
            return;
        }
        None => {}
    }
    // Built before the values move into the backend.
    let mut fields = vec![format!("verify = {verify}")];
    if let Some(ca) = &ca_file {
        fields.push(format!("ca_file = «{ca}»"));
    }
    if let Some(name) = &server_name {
        fields.push(format!("server_name = «{name}»"));
    }
    let detail = fields.join(", ");
    if let Some(target) = conv.backend_mut(backend) {
        target.tls = Some(super::BackendTls {
            verify,
            ca_file,
            server_name,
            alpn: Vec::new(),
            min_version: None,
        });
    }
    conv.info(
        1,
        directive,
        context,
        format!("TLS al backend en [backend.{backend}.tls]: {detail}"),
    );
}

/// The host of an authority when it is a name: TLS SNI carries DNS names, so an
/// IP literal (bracketed or not) never becomes `server_name`.
fn sni_host(authority: &str) -> Option<&str> {
    let host = match authority.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map_or(rest, |(inside, _)| inside),
        None => authority.split(':').next().unwrap_or(authority),
    };
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    Some(host)
}

/// `10.9.9.9` → `10.9.9.9/32`, `::1` → `::1/128`; a CIDR is kept.
fn cidr_of(raw: &str) -> String {
    if raw.contains('/') {
        return raw.to_owned();
    }
    if raw.contains(':') {
        format!("{raw}/128")
    } else {
        format!("{raw}/32")
    }
}

/// nginx variables the header templates do not know.
fn nginx_variable_unsupported(value: &str) -> Option<String> {
    let mut rest = value;
    while let Some(index) = rest.find('$') {
        let tail = &rest[index + 1..];
        let name: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            return Some("$".to_owned());
        }
        if !matches!(name.as_str(), "remote_addr" | "host" | "scheme") {
            return Some(format!("${name}"));
        }
        rest = &tail[name.len()..];
    }
    None
}

/// `stream { … }` → `mode = "tcp"` listeners.
#[allow(clippy::too_many_lines)]
fn stream(conv: &mut Conv, stream: &Node) -> Result<(), ImportError> {
    conv.info(
        1,
        "stream",
        "stream",
        "el bloque stream (TCP) se traduce a listeners con mode = \"tcp\"",
    );
    for up in children(stream, "upstream") {
        let source_name = up
            .args
            .first()
            .cloned()
            .unwrap_or_else(|| "upstream_tcp".to_owned());
        let context = format!("stream upstream {source_name}");
        let name = conv.free_backend_name(&source_name);
        let mut backend = super::Backend::default();
        for child in &up.children {
            let before = conv.notes.len();
            let text = directive(child);
            if child.name == "server" {
                if let Some(raw) = child.args.first() {
                    if let Some(addr) = normalize_addr(conv, raw, None, &text, &context, false) {
                        backend.servers.push(super::Server {
                            addr,
                            weight: 1,
                            backup: false,
                            maxconn: 0,
                        });
                        conv.done(1, &text, &context, "servidor TCP del upstream");
                    }
                } else {
                    conv.review(1, &text, &context, "servidor sin dirección: se omite");
                }
            } else if child.name.starts_with("proxy_ssl_") {
                unknown_proxy_ssl(conv, &text, &context, &child.name);
            } else {
                conv.gap(
                    1,
                    &text,
                    &context,
                    "directiva de upstream TCP sin equivalente",
                );
            }
            conv.touch(&directive(child), &context, before)?;
        }
        if !backend.servers.is_empty() {
            conv.add_backend(name, backend);
        }
    }

    for srv in children(stream, "server") {
        let context = "stream server".to_owned();
        let Some(listen) = first(srv, "listen") else {
            conv.reject(
                1,
                "server",
                &context,
                "server de stream sin listen: se omite",
            );
            continue;
        };
        let Some(raw) = listen.args.first() else {
            conv.reject(1, "listen", &context, "listen sin dirección: se omite");
            continue;
        };
        let Some(addr) = normalize_addr(conv, raw, None, &directive(listen), &context, false)
        else {
            continue;
        };
        let Some(pass) = first(srv, "proxy_pass") else {
            conv.reject(
                1,
                "server",
                &context,
                "server de stream sin proxy_pass: se omite",
            );
            continue;
        };
        let Some(target) = pass.args.first() else {
            conv.reject(1, "proxy_pass", &context, "proxy_pass vacío: se omite");
            continue;
        };
        // A stream `proxy_pass` names an address, not a URL: there is no
        // scheme to carry (nginx does not accept one there either).
        let Some(backend) = resolve_backend(conv, target, 80, &context) else {
            continue;
        };
        let name = conv.free_listener_name(&format!("tcp_{backend}"));
        let mut listener = Listener::new(addr, "tcp", backend.clone());
        for child in &srv.children {
            let before = conv.notes.len();
            let text = directive(child);
            match child.name.as_str() {
                "listen" | "proxy_pass" => {}
                "proxy_timeout" => {
                    listener.idle_timeout = convert_duration(
                        conv,
                        child.args.first().map_or_default(String::as_str),
                        "s",
                        &text,
                        &context,
                    );
                    conv.review(1, &text, &context, "listener.idle_timeout (aprox.)");
                }
                "proxy_connect_timeout" => {
                    listener.connect_timeout = convert_duration(
                        conv,
                        child.args.first().map_or_default(String::as_str),
                        "s",
                        &text,
                        &context,
                    );
                    conv.done(1, &text, &context, "listener.connect_timeout");
                }
                "ssl_preread" => conv.gap(
                    1,
                    &text,
                    &context,
                    "enrutado por SNI (ssl_preread): EQProxy no inspecciona el ClientHello",
                ),
                name if name.starts_with("proxy_ssl_") => {
                    unknown_proxy_ssl(conv, &text, &context, name);
                }
                name => match supported_manually(name) {
                    Some(place) => conv.review(
                        1,
                        &text,
                        &context,
                        format!("EQProxy sí lo soporta en {place}: transcríbalo a mano"),
                    ),
                    None => conv.gap(
                        1,
                        &text,
                        &context,
                        "directiva de stream sin equivalente conocido",
                    ),
                },
            }
            conv.touch(&directive(child), &context, before)?;
        }
        conv.done(
            1,
            format!("listen {raw}"),
            &context,
            format!("listener «{name}» (tcp) → backend «{backend}»"),
        );
        conv.add_listener(name, listener);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Note;
    use super::*;

    #[test]
    fn the_parser_reads_nested_blocks_and_quoted_words() {
        let nodes = parse(
            &tokenize("http {\n  server {\n    listen 80;\n    add_header X \"a;b\" ;\n  }\n}\n")
                .expect("tokenize"),
        )
        .expect("parse");
        assert_eq!(nodes.len(), 1);
        let server = &nodes[0].children[0];
        assert_eq!(server.name, "server");
        assert_eq!(children(server, "listen")[0].args, vec!["80".to_owned()]);
        assert_eq!(
            children(server, "add_header")[0].args,
            vec!["X".to_owned(), "a;b".to_owned()],
            "a «;» inside quotes does not end the directive"
        );
    }

    #[test]
    fn ssl_protocols_becomes_a_min_max_version_range() {
        // nginx `ssl_protocols TLSv1.2` means exactly 1.2; with both versions
        // the default upper bound (1.3) stays. B-N16.
        let only_12 = "http {\n  server {\n    listen 443 ssl;\n    ssl_certificate /etc/c.pem;\n    ssl_certificate_key /etc/k.pem;\n    ssl_protocols TLSv1.2;\n    location / { proxy_pass http://127.0.0.1:8080; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, only_12).expect("convert");
        assert!(
            outcome.toml.contains("min_version = \"1.2\"")
                && outcome.toml.contains("max_version = \"1.2\""),
            "TLSv1.2 only must cap the range:\n{}",
            outcome.toml
        );

        let both = only_12.replace("ssl_protocols TLSv1.2;", "ssl_protocols TLSv1.2 TLSv1.3;");
        let outcome = super::super::convert(super::super::Source::Nginx, &both).expect("convert");
        assert!(
            outcome.toml.contains("min_version = \"1.2\"") && !outcome.toml.contains("max_version"),
            "both versions keep the default upper bound:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn upstream_max_conns_becomes_the_per_server_cap() {
        // nginx `server … max_conns=N` (B-H16): translated, and a value past
        // the schema ceiling is clipped with a note instead of producing an
        // invalid file.
        let source = "http {\n  upstream app {\n    server 10.0.0.1:8080 max_conns=50;\n  }\n  server {\n    listen 80;\n    location / { proxy_pass http://app; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.toml.contains("maxconn = 50"),
            "the cap must reach the file:\n{}",
            outcome.toml
        );

        let clipped = "http {\n  upstream app {\n    server 10.0.0.1:8080 max_conns=5000000;\n  }\n  server {\n    listen 80;\n    location / { proxy_pass http://app; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, clipped).expect("convert");
        assert!(
            outcome.toml.contains("maxconn = 1000000"),
            "an over-ceiling cap is clipped:\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("recortado a 1000000"),
            "and the clipping is named in the report:\n{}",
            outcome.report
        );
    }

    #[test]
    fn proxy_pass_https_keeps_tls_and_defaults_to_443() {
        // EXT-H-04 / P0-13: the scheme used to be dropped, so nginx's TLS to
        // the origin became plaintext with nothing in the report.
        let https = "http {\n  server {\n    listen 80;\n    location / { proxy_pass https://backend.internal:8443; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, https).expect("convert");
        assert!(
            outcome
                .toml
                .contains("[backend.backend_backend_internal_8443.tls]"),
            "https must reach [backend.*.tls]:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("server = \"backend.internal:8443\""),
            "the port is kept:\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("TLS al backend"),
            "the report must name the TLS mapping, not stay silent:\n{}",
            outcome.report
        );

        // No port means 443, never 80.
        let default_port = "http {\n  server {\n    listen 80;\n    location / { proxy_pass https://10.0.0.1; }\n  }\n}\n";
        let outcome =
            super::super::convert(super::super::Source::Nginx, default_port).expect("convert");
        assert!(
            outcome.toml.contains("server = \"10.0.0.1:443\""),
            "https without a port is 443, never 80:\n{}",
            outcome.toml
        );
        assert!(outcome.toml.contains(".tls]"), "\n{}", outcome.toml);

        // Plain http keeps port 80 and gains no TLS.
        let plain = "http {\n  server {\n    listen 80;\n    location / { proxy_pass http://10.0.0.2; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, plain).expect("convert");
        assert!(
            outcome.toml.contains("server = \"10.0.0.2:80\""),
            "\n{}",
            outcome.toml
        );
        assert!(
            !outcome.toml.contains(".tls]"),
            "plain http must not gain TLS:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn proxy_ssl_directives_fill_the_backend_tls_block() {
        // nginx defaults: no verification of the origin certificate and no
        // SNI; `proxy_ssl_*` is what turns either on, so the import must not
        // invent them.
        let source = "http {\n  server {\n    listen 80;\n    location / {\n      proxy_pass https://backend.internal:8443;\n      proxy_ssl_verify on;\n      proxy_ssl_trusted_certificate /etc/ca.pem;\n      proxy_ssl_server_name on;\n      proxy_ssl_name app.example;\n    }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.toml.contains("verify = true"),
            "proxy_ssl_verify on must verify:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("ca_file = \"/etc/ca.pem\""),
            "proxy_ssl_trusted_certificate must become ca_file:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("server_name = \"app.example\""),
            "proxy_ssl_name must become server_name:\n{}",
            outcome.toml
        );

        // `proxy_ssl_server_name on` without `proxy_ssl_name` takes the SNI from
        // the authority, and an IP literal (no DNS name to present) never
        // becomes server_name.
        let sni = "http {\n  server {\n    listen 80;\n    location / {\n      proxy_pass https://backend.internal:8443;\n      proxy_ssl_server_name on;\n    }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, sni).expect("convert");
        assert!(
            outcome.toml.contains("server_name = \"backend.internal\""),
            "SNI comes from the authority:\n{}",
            outcome.toml
        );

        let ip_sni = "http {\n  server {\n    listen 80;\n    location / {\n      proxy_pass https://10.0.0.9:8443;\n      proxy_ssl_server_name on;\n    }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, ip_sni).expect("convert");
        assert!(
            !outcome.toml.contains("server_name = "),
            "an IP literal is never an SNI name:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn tls_policy_the_schema_cannot_express_is_degraded_not_converted() {
        // N-2: the origin authenticates itself with a client certificate and
        // pins the version policy and the chain depth. The import emits TLS but
        // none of those, so those rows must never read as converted.
        let source = "http {\n  server {\n    listen 80;\n    location / {\n      proxy_pass https://backend.internal:8443;\n      proxy_ssl_verify on;\n      proxy_ssl_certificate /etc/client.pem;\n      proxy_ssl_certificate_key /etc/client.key;\n      proxy_ssl_protocols TLSv1.3;\n      proxy_ssl_verify_depth 1;\n    }\n  }\n}\n";
        let (outcome, degraded) =
            super::super::convert_with_degraded(super::super::Source::Nginx, source)
                .expect("convert");
        assert!(
            outcome.toml.contains("verify = true"),
            "the TLS that does map is still emitted:\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("## Convertido con degradación"),
            "a lost security property must have its own summary:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("· con degradación **4** ·"),
            "certificate, its key, version policy and depth are four losses:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("certificado de cliente")
                && outcome.report.contains("versiones TLS")
                && outcome.report.contains("profundidad de verificación"),
            "every degradation names the property that was lost:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("equivalente en seguridad"),
            "the summary must say the result is not security-equivalent:\n{}",
            outcome.report
        );
        assert_eq!(
            degraded.len(),
            4,
            "certificate, its key, version policy and depth are four losses: {degraded:?}"
        );

        // The same directives on a plaintext origin do nothing in nginx either:
        // nothing is lost, so nothing is degraded.
        let plaintext = "http {\n  upstream app { server 10.0.0.1:8080; }\n  server {\n    listen 80;\n    location / {\n      proxy_pass http://app;\n      proxy_ssl_certificate /etc/client.pem;\n      proxy_ssl_protocols TLSv1.3;\n    }\n  }\n}\n";
        let outcome =
            super::super::convert(super::super::Source::Nginx, plaintext).expect("convert");
        assert!(
            !outcome.report.contains("## Convertido con degradación"),
            "a directive nginx ignores loses nothing:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("la directiva no llega al TOML"),
            "the report says why it did not apply:\n{}",
            outcome.report
        );
    }

    #[test]
    fn vhost_tls_divergence_is_degraded_and_never_silent() {
        // mTLS, versions and ciphers cannot be carried per vhost: the listener
        // applies the default_server's policy, so a vhost that declares a
        // different one loses it and must read as degraded.
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    ssl_protocols TLSv1.3;\n    ssl_ciphers HIGH;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name b.example;\n    ssl_certificate /etc/b.pem;\n    ssl_certificate_key /etc/b.key;\n    ssl_protocols TLSv1.2;\n    ssl_ciphers 'LOW';\n    ssl_client_certificate /etc/clients.pem;\n    ssl_verify_client on;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.report.contains("## Convertido con degradación"),
            "split TLS policy must be degraded:\n{}",
            outcome.report
        );
        for loss in [
            "política de versiones TLS",
            "política de cifrados",
            "certificado de cliente (mTLS)",
        ] {
            assert!(
                outcome.report.contains(loss),
                "the report must name {loss}:\n{}",
                outcome.report
            );
        }
        // The listener keeps the default_server policy and the second cert.
        assert!(
            outcome.toml.contains("min_version = \"1.3\"")
                && !outcome.toml.contains("min_version = \"1.2\""),
            "the default_server version policy wins:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains(".tls.certs]]")
                && outcome.toml.contains("names = [\"b.example\"]"),
            "the second vhost keeps its certificate:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn a_secondary_vhost_without_exact_names_emits_no_hostless_rules() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    ssl_certificate /etc/catchall.pem;\n    ssl_certificate_key /etc/catchall.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.report.contains("vhost sin server_name traducible")
                && outcome.report.contains("reglas no se emiten"),
            "the report must explain why the catch-all is dropped:\n{}",
            outcome.report
        );
        assert_eq!(
            outcome
                .toml
                .lines()
                .filter(|line| line.starts_with("[[listener.") && line.contains(".rules]]"))
                .count(),
            1,
            "only the named default vhost contributes a rule:\n{}",
            outcome.toml
        );
        assert!(
            !outcome.toml.contains("catchall.pem"),
            "an unreachable certificate must not be emitted:\n{}",
            outcome.toml
        );
        assert!(
            !outcome
                .toml
                .contains("use_backend = \"backend_127_0_0_1_9001\""),
            "the unnamed vhost's catch-all rule must not be emitted:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn a_default_server_is_chosen_per_listen_address() {
        // The first vhost is the default of 443 and has no exact names; it is
        // only a secondary on 8443, where the second vhost declares
        // default_server. Looking at the first vhost's 443 flag would make it
        // primary on 8443 too and turn its no-name rules into a catch-all.
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    listen 8443 ssl;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 8443 ssl default_server;\n    server_name b.example;\n    ssl_certificate /etc/b.pem;\n    ssl_certificate_key /etc/b.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert_eq!(
            outcome
                .toml
                .lines()
                .filter(|line| line.starts_with("[[listener.") && line.contains(".rules]]"))
                .count(),
            2,
            "443 keeps its default catch-all and 8443 only the named default rule:\n{}",
            outcome.toml
        );
        assert_eq!(
            outcome
                .toml
                .lines()
                .filter(|line| line.starts_with("host = "))
                .count(),
            1,
            "the only named vhost on 8443 owns the only host rule:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("host = \"b.example\""),
            "the real default of 8443 keeps its host rule:\n{}",
            outcome.toml
        );
        assert!(
            !outcome
                .toml
                .contains("use_backend = \"backend_127_0_0_1_9000_2\""),
            "the secondary no-name vhost must not contribute a catch-all on 8443:\n{}",
            outcome.toml
        );
        assert!(
            !outcome.report.lines().any(|line| {
                line.contains("regla con use_backend = «backend_127_0_0_1_9000_2»")
                    && !line.contains("no se emite")
            }),
            "the report must not claim a rule that was dropped:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("vhost sin server_name traducible")
                && outcome.report.contains("no se convierten en catch-all")
                && outcome.report.contains("la regla con use_backend"),
            "the report must explain the drop and the default_server fallback:\n{}",
            outcome.report
        );
    }

    #[test]
    fn a_secondary_vhost_with_only_a_wildcard_emits_a_host_regex_and_certificate() {
        // nginx's `*.example` matches one or more labels below the suffix, so
        // it becomes a host_regex (not a catch-all) and its certificate is
        // announced by the one-label SNI wildcard the schema can express.
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name *.example;\n    ssl_certificate /etc/catchall.pem;\n    ssl_certificate_key /etc/catchall.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            !outcome.report.contains("vhost sin server_name traducible"),
            "the wildcard vhost must be translated, not dropped:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("host_regex"),
            "the report must name the generated host condition:\n{}",
            outcome.report
        );
        assert_eq!(
            outcome
                .toml
                .lines()
                .filter(|line| line.starts_with("[[listener.") && line.contains(".rules]]"))
                .count(),
            2,
            "each vhost contributes a rule:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("host = \"a.example\""),
            "the emitted rule must stay tied to the exact host:\n{}",
            outcome.toml
        );
        assert!(
            outcome
                .toml
                .contains("use_backend = \"backend_127_0_0_1_9001\""),
            "the wildcard vhost must route to its own backend:\n{}",
            outcome.toml
        );
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("wildcard"))
            .expect("generated TOML parses");
        let listener = parsed.listener.values().next().expect("listener");
        let regex = listener
            .rules
            .iter()
            .find_map(|rule| rule.host_regex.as_deref())
            .expect("wildcard host_regex");
        assert_eq!(
            regex, r"^[^.]+(?:\.[^.]+)*\.example$",
            "the wildcard must use nginx's multi-label semantics"
        );
        let tls = listener.tls.as_ref().expect("listener TLS");
        assert_eq!(tls.certs[0].names, ["*.example"]);
    }

    #[test]
    fn leading_dot_server_name_covers_the_apex_and_subdomains() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name alpha.example.test;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name .example.test;\n    ssl_certificate /etc/dot.pem;\n    ssl_certificate_key /etc/dot.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.toml.contains("host = \"example.test\""),
            "the leading dot covers the exact apex:\n{}",
            outcome.toml
        );
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("leading-dot"))
            .expect("generated TOML parses");
        let listener = parsed.listener.values().next().expect("listener");
        let regex = listener
            .rules
            .iter()
            .find_map(|rule| rule.host_regex.as_deref())
            .expect("leading-dot host_regex");
        assert_eq!(
            regex, r"^[^.]+(?:\.[^.]+)*\.example\.test$",
            "the leading dot covers every subdomain depth"
        );
        let tls = listener.tls.as_ref().expect("listener TLS");
        assert_eq!(
            tls.certs[0].names,
            ["example.test", "*.example.test"],
            "SNI must announce the apex and the wildcard the certificate covers"
        );
    }

    #[test]
    fn trailing_wildcard_becomes_host_prefix_and_reports_the_sni_limit() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name alpha.example.test;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name mail.*;\n    ssl_certificate /etc/mail.pem;\n    ssl_certificate_key /etc/mail.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.toml.contains("host_prefix = \"mail.\""),
            "a trailing wildcard is a literal host prefix:\n{}",
            outcome.toml
        );
        assert!(
            !outcome.toml.contains("names = [\"mail.*\"]"),
            "certs[].names cannot express a trailing wildcard:\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("vhost sin nombre expresable")
                && outcome.report.contains("mail.*"),
            "the report must name the SNI certificate that was not emitted:\n{}",
            outcome.report
        );
    }

    #[test]
    fn regex_server_name_becomes_a_case_insensitive_host_regex() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name alpha.example.test;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name ~^API-[0-9]+\\.example\\.test$;\n    ssl_certificate /etc/api.pem;\n    ssl_certificate_key /etc/api.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("regex"))
            .expect("generated TOML parses");
        let regex = parsed
            .listener
            .values()
            .next()
            .expect("listener")
            .rules
            .iter()
            .find_map(|rule| rule.host_regex.as_deref())
            .expect("regex host_regex");
        assert_eq!(
            regex, r"(?i)^API-[0-9]+\.example\.test$",
            "an uppercase PCRE name is caseless in nginx; the generated regex must say so"
        );
        assert!(
            outcome.report.contains("host_regex") && outcome.report.contains("API-[0-9]+"),
            "the report must name the translated expression:\n{}",
            outcome.report
        );
    }

    #[test]
    fn expressly_caseless_regex_gets_the_inline_flag() {
        let source = "http {\n  server {\n    listen 80;\n    server_name ~*^api-[0-9]+\\.example\\.test$;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("regex-star"))
            .expect("generated TOML parses");
        let regex = parsed
            .listener
            .values()
            .next()
            .expect("listener")
            .rules
            .iter()
            .find_map(|rule| rule.host_regex.as_deref())
            .expect("regex host_regex");
        assert_eq!(
            regex, r"(?i)^api-[0-9]+\.example\.test$",
            "nginx's ~* is explicitly caseless even with no uppercase letter"
        );
    }

    #[test]
    fn an_unsupported_regex_never_becomes_a_catch_all() {
        // Rust's regex engine has no look-around; EQProxy refuses the
        // expression instead of emitting invalid TOML or dropping the host
        // condition and turning the vhost into a catch-all.
        let source = "http {\n  server {\n    listen 80;\n    server_name ~^(?=api)\\.example\\.test$;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.report.contains("sin forma traducible")
                && outcome.report.contains("no se puede evaluar"),
            "the report must explain the unsupported expression:\n{}",
            outcome.report
        );
        assert!(
            !outcome.toml.contains(".rules]]"),
            "a declared name that cannot be translated must not become a catch-all rule:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn server_name_rules_follow_nginx_precedence() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name api-7.example.test;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name *.example.test;\n    ssl_certificate /etc/wild.pem;\n    ssl_certificate_key /etc/wild.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n  server {\n    listen 443 ssl;\n    server_name ~^api-[0-9]+\\.example\\.test$;\n    ssl_certificate /etc/regex.pem;\n    ssl_certificate_key /etc/regex.key;\n    location / { proxy_pass http://127.0.0.1:9002; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("precedence"))
            .expect("generated TOML parses");
        let rules = &parsed.listener.values().next().expect("listener").rules;
        assert_eq!(rules.len(), 3, "one rule per vhost: {rules:?}");
        assert_eq!(rules[0].host.as_deref(), Some("api-7.example.test"));
        assert_eq!(
            rules[1].host_regex.as_deref(),
            Some(r"^[^.]+(?:\.[^.]+)*\.example\.test$")
        );
        assert_eq!(
            rules[2].host_regex.as_deref(),
            Some(r"^api-[0-9]+\.example\.test$")
        );
    }

    #[test]
    fn longest_wildcards_and_trailing_wildcards_sort_before_regexes() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name api-7.example.test;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name *.example.test;\n    ssl_certificate /etc/one.pem;\n    ssl_certificate_key /etc/one.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n  server {\n    listen 443 ssl;\n    server_name *.sub.example.test;\n    ssl_certificate /etc/two.pem;\n    ssl_certificate_key /etc/two.key;\n    location / { proxy_pass http://127.0.0.1:9002; }\n  }\n  server {\n    listen 443 ssl;\n    server_name mail.*;\n    ssl_certificate /etc/three.pem;\n    ssl_certificate_key /etc/three.key;\n    location / { proxy_pass http://127.0.0.1:9003; }\n  }\n  server {\n    listen 443 ssl;\n    server_name ~^api-[0-9]+\\.example\\.test$;\n    ssl_certificate /etc/four.pem;\n    ssl_certificate_key /etc/four.key;\n    location / { proxy_pass http://127.0.0.1:9004; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("precedence-full"))
            .expect("generated TOML parses");
        let rules = &parsed.listener.values().next().expect("listener").rules;
        assert_eq!(rules.len(), 5, "one rule per vhost: {rules:?}");
        assert_eq!(rules[0].host.as_deref(), Some("api-7.example.test"));
        assert_eq!(
            rules[1].host_regex.as_deref(),
            Some(r"^[^.]+(?:\.[^.]+)*\.sub\.example\.test$"),
            "the longest leading wildcard comes first"
        );
        assert_eq!(
            rules[2].host_regex.as_deref(),
            Some(r"^[^.]+(?:\.[^.]+)*\.example\.test$"),
            "a shorter leading wildcard follows"
        );
        assert_eq!(
            rules[3].host_prefix.as_deref(),
            Some("mail."),
            "trailing wildcards follow all leading wildcards"
        );
        assert_eq!(
            rules[4].host_regex.as_deref(),
            Some(r"^api-[0-9]+\.example\.test$"),
            "regexes come last, before the catch-all"
        );
    }

    #[test]
    fn every_vhost_of_a_shared_listener_gets_its_certificate_and_host_rule() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name b.example;\n    ssl_certificate /etc/b.pem;\n    ssl_certificate_key /etc/b.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n  server {\n    listen 443 ssl;\n    server_name c.example;\n    ssl_certificate /etc/c.pem;\n    ssl_certificate_key /etc/c.key;\n    location / { proxy_pass http://127.0.0.1:9002; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert_eq!(
            outcome.toml.matches(".tls.certs]]").count(),
            2,
            "the two non-default vhosts each need a certificate entry:\n{}",
            outcome.toml
        );
        for host in ["a.example", "b.example", "c.example"] {
            assert!(
                outcome.toml.contains(&format!("host = \"{host}\"")),
                "{host} must have a host rule:\n{}",
                outcome.toml
            );
        }
    }

    #[test]
    fn a_repeated_default_certificate_is_not_duplicated_in_certs() {
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/shared.pem;\n    ssl_certificate_key /etc/shared.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name b.example;\n    ssl_certificate /etc/shared.pem;\n    ssl_certificate_key /etc/shared.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            !outcome.toml.contains(".tls.certs]]"),
            "the default certificate already serves the second name:\n{}",
            outcome.toml
        );
        assert!(
            outcome
                .report
                .contains("no se duplica en [listener.*.tls].certs[]"),
            "the report must say the certificate was not duplicated:\n{}",
            outcome.report
        );
        assert!(
            outcome.toml.contains("host = \"b.example\""),
            "\n{}",
            outcome.toml
        );
    }

    #[test]
    fn vhost_disabling_inherited_mtls_is_degraded() {
        // The default_server inherits ssl_verify_client on and the CA from
        // http. A secondary vhost that turns verification off has no mTLS
        // client_ca in the shared listener; that policy divergence must be
        // reported as degraded instead of silently applying mTLS to it.
        let source = "http {\n  ssl_certificate /etc/http.pem;\n  ssl_certificate_key /etc/http.key;\n  ssl_client_certificate /etc/clients.pem;\n  ssl_verify_client on;\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name b.example;\n    ssl_verify_client off;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome.report.contains("## Convertido con degradación"),
            "the mTLS divergence must be degraded:\n{}",
            outcome.report
        );
        assert!(
            outcome.report.contains("certificado de cliente (mTLS)"),
            "the report must name the client-certificate loss:\n{}",
            outcome.report
        );
        assert!(
            outcome.toml.contains("client_ca = \"/etc/clients.pem\""),
            "the default_server keeps mTLS for the listener:\n{}",
            outcome.toml
        );
    }

    #[test]
    fn repeated_server_name_directives_all_become_hosts() {
        // `server_name` may be repeated; every exact name must reach both the
        // rules and the certificate, not just the first directive.
        let source = "http {\n  server {\n    listen 443 ssl default_server;\n    server_name a.example;\n    ssl_certificate /etc/a.pem;\n    ssl_certificate_key /etc/a.key;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 443 ssl;\n    server_name b.example;\n    server_name c.example;\n    ssl_certificate /etc/b.pem;\n    ssl_certificate_key /etc/b.key;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source).expect("convert");
        assert!(
            outcome
                .toml
                .contains("names = [\"b.example\", \"c.example\"]"),
            "both names must claim the certificate:\n{}",
            outcome.toml
        );
        for host in ["b.example", "c.example"] {
            assert!(
                outcome.toml.contains(&format!("host = \"{host}\"")),
                "{host} must route to its own copy of the rules:\n{}",
                outcome.toml
            );
        }
    }

    #[test]
    fn two_locations_to_the_same_https_backend_are_idempotent() {
        // The same backend reached from two locations with the same
        // proxy_ssl_* combination is one TLS configuration, not a conflict:
        // the second location must not be reported for review.
        let same = "http {\n  upstream app { server 10.0.0.1:8443; }\n  server {\n    listen 80;\n    location /a { proxy_pass https://app; }\n    location /b { proxy_pass https://app; }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, same).expect("convert");
        assert_eq!(
            outcome.toml.matches("[backend.app.tls]").count(),
            1,
            "one TLS block for the shared backend:\n{}",
            outcome.toml
        );
        assert!(
            !outcome.report.contains("ya declara"),
            "identical combinations must not be reviewed:\n{}",
            outcome.report
        );
        assert_eq!(
            outcome.report.matches("TLS al backend").count(),
            1,
            "the mapping is named once, for the first location:\n{}",
            outcome.report
        );

        // A different combination for the same backend is the case the model
        // cannot represent, and it stays a review that names the backend.
        let diverging = "http {\n  upstream app { server 10.0.0.1:8443; }\n  server {\n    listen 80;\n    location /a { proxy_pass https://app; }\n    location /b {\n      proxy_pass https://app;\n      proxy_ssl_verify on;\n    }\n  }\n}\n";
        let outcome =
            super::super::convert(super::super::Source::Nginx, diverging).expect("convert");
        assert!(
            outcome
                .report
                .contains("ya declara [backend.app.tls] con otros valores"),
            "a real divergence is still reported:\n{}",
            outcome.report
        );
        assert_eq!(
            outcome.toml.matches("[backend.app.tls]").count(),
            1,
            "\n{}",
            outcome.toml
        );
    }

    #[test]
    fn error_page_and_intercept_errors_become_a_listener_page() {
        // B-N14: a server-level `error_page` covers every location, exactly
        // EQProxy's listener scope; `proxy_intercept_errors on` also replaces
        // a matching backend response.
        let source = "http {\n  server {\n    listen 80;\n    proxy_intercept_errors on;\n    error_page 502 503 =200 /errors/50x.html;\n    location / { proxy_pass http://app; }\n  }\n  upstream app { server 10.0.0.1:8080; }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source)
            .expect("error_page must convert");
        assert!(
            outcome.toml.contains("codes = [502, 503]"),
            "every code of the directive must be kept:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("status = 200") && outcome.toml.contains("intercept = true"),
            "`=200` and proxy_intercept_errors must survive:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("file = \"errors/50x.html\""),
            "nginx's URI becomes a config-relative path:\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("no contra el root de nginx"),
            "the root-vs-config-directory difference must be named:\n{}",
            outcome.report
        );
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("nginx.conf"))
            .expect("generated TOML parses");
        let check =
            crate::validate::check(&parsed, &outcome.toml, std::path::Path::new("nginx.conf"));
        assert!(check.is_valid(), "{check:?}");

        // A location-scoped error_page cannot be represented: the page is
        // per listener, so the converter says so instead of widening it.
        let scoped = "http {\n  server {\n    listen 80;\n    location / {\n      error_page 500 /500.html;\n      proxy_pass http://10.0.0.1:8080;\n    }\n  }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, scoped)
            .expect("location error_page is classified");
        assert!(!outcome.toml.contains("error_pages"), "{}", outcome.toml);
        assert!(
            outcome.report.contains("página de error por location"),
            "the location scope must be explained:\n{}",
            outcome.report
        );
    }

    #[test]
    fn later_error_page_codes_do_not_erase_the_other_codes_of_an_earlier_page() {
        // nginx last-wins per code, not per directive: `error_page 500 /b`
        // replaces 500 but 502 keeps the page from `error_page 500 502 /a`.
        let source = "http {\n  server {\n    listen 80;\n    error_page 500 502 /a.html;\n    error_page 500 /b.html;\n    location / { proxy_pass http://app; }\n  }\n  upstream app { server 10.0.0.1:8080; }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source)
            .expect("overlapping error pages must convert");
        assert!(
            outcome.toml.contains("codes = [502]") && outcome.toml.contains("file = \"a.html\""),
            "502 must keep the first page:\n{}",
            outcome.toml
        );
        assert!(
            outcome.toml.contains("codes = [500]") && outcome.toml.contains("file = \"b.html\""),
            "500 must take the later page:\n{}",
            outcome.toml
        );
        let parsed = crate::parse_str(&outcome.toml, std::path::Path::new("nginx.conf"))
            .expect("generated TOML parses");
        let check =
            crate::validate::check(&parsed, &outcome.toml, std::path::Path::new("nginx.conf"));
        assert!(check.is_valid(), "{check:?}");
    }

    #[test]
    fn comments_are_not_directives() {
        let nodes = tokenize("# comentario\nlisten 80; # otro\n")
            .and_then(|tokens| parse(&tokens))
            .expect("parse");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "listen");
    }

    #[test]
    fn broken_input_fails_with_a_named_cause() {
        for (text, expected) in [
            ("http {", "bloque sin cerrar"),
            ("listen 80", "no termina en «;»"),
            ("}", "sin bloque abierto"),
            ("server_name \"x", "sin cerrar"),
        ] {
            let err = tokenize(text)
                .and_then(|tokens| parse(&tokens))
                .expect_err("must fail");
            assert!(err.message.contains(expected), "{}", err.message);
        }
    }

    #[test]
    fn inherited_sni_without_a_dns_name_is_not_done() {
        let source = "http {\n  proxy_ssl_server_name on;\n  server {\n    listen 80;\n    location / { proxy_pass https://10.0.0.9:8443; }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("convert");
        let note = conv
            .notes
            .iter()
            .find(|note| note.directive.starts_with("proxy_ssl_server_name"))
            .expect("the inherited directive is classified");
        assert_eq!(note.severity, Severity::Info, "{}", note.message);
        assert!(
            note.message.contains("sin nombre DNS"),
            "the row must explain why no field was written: {}",
            note.message
        );
        let toml = super::super::emit_toml(&conv);
        assert!(!toml.contains("server_name = "), "{toml}");
    }

    #[test]
    fn unknown_proxy_ssl_is_explicitly_classified_in_every_scope() {
        let source = "proxy_ssl_global_unknown on;\nhttp {\n  proxy_ssl_http_unknown on;\n  upstream backend {\n    server 10.0.0.2:8443;\n    proxy_ssl_upstream_unknown on;\n  }\n  server {\n    listen 80;\n    proxy_ssl_server_unknown on;\n    location / {\n      proxy_pass https://backend.internal:8443;\n      proxy_ssl_location_unknown on;\n    }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("unknown proxy_ssl_* is classified");

        for name in [
            "proxy_ssl_global_unknown",
            "proxy_ssl_http_unknown",
            "proxy_ssl_upstream_unknown",
            "proxy_ssl_server_unknown",
            "proxy_ssl_location_unknown",
        ] {
            let notes: Vec<&Note> = conv
                .notes
                .iter()
                .filter(|note| note.directive.contains(name))
                .collect();
            assert_eq!(
                notes.len(),
                1,
                "{name} must produce exactly one classification: {:?}",
                conv.notes
            );
            assert_eq!(
                notes[0].severity,
                Severity::Review,
                "an unknown TLS qualifier is never Done: {name}"
            );
            assert!(
                notes[0].message.contains("proxy_ssl_*") && notes[0].message.contains(name),
                "the row must name the unknown family and directive: {}",
                notes[0].message
            );
        }
    }

    #[test]
    fn secondary_listener_knobs_are_reviewed_as_discarded() {
        // ADR-0051: a secondary vhost contributes certificates and rules, not
        // listener-scope values. Its timeouts, body limit, compression and
        // cache must be visible in the report as discarded (`Review`), never
        // as applied and never as a security `Degraded`.
        let source = "http {\n  server {\n    listen 80 default_server;\n    server_name primary.example;\n    client_header_timeout 10s;\n    keepalive_timeout 60s;\n    proxy_connect_timeout 1s;\n    proxy_read_timeout 30s;\n    client_max_body_size 1m;\n    gzip on;\n    gzip_comp_level 5;\n    gzip_min_length 512;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 80;\n    server_name secondary.example;\n    client_header_timeout 20s;\n    keepalive_timeout 120s;\n    proxy_connect_timeout 3s;\n    proxy_read_timeout 45s;\n    client_max_body_size 2m;\n    gzip off;\n    gzip_comp_level 7;\n    gzip_min_length 1024;\n    proxy_cache other_zone;\n    limit_conn addr 5;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("convert");
        let toml = super::super::emit_toml(&conv);

        for needle in [
            "client_header_timeout = \"10s\"",
            "idle_timeout = \"1m\"",
            "connect_timeout = \"1s\"",
            "server_header_timeout = \"30s\"",
            "max_body_bytes = 1048576",
            "gzip = true",
            "level = 5",
            "min_size = 512",
        ] {
            assert!(
                toml.contains(needle),
                "the default_server value must reach the listener ({needle}):\n{toml}"
            );
        }
        for needle in [
            "client_header_timeout = \"20s\"",
            "idle_timeout = \"2m\"",
            "connect_timeout = \"3s\"",
            "server_header_timeout = \"45s\"",
            "max_body_bytes = 2097152",
            "level = 7",
            "min_size = 1024",
            "gzip = false",
            ".cache]",
        ] {
            assert!(
                !toml.contains(needle),
                "the secondary value must not reach the listener ({needle}):\n{toml}"
            );
        }

        let context = "server secondary.example";
        for (directive, marker) in [
            ("client_header_timeout 20s", "no aplica"),
            ("keepalive_timeout 120s", "no aplica"),
            ("proxy_connect_timeout 3s", "no aplica"),
            ("proxy_read_timeout 45s", "no aplica"),
            ("client_max_body_size 2m", "no aplica"),
            ("gzip off", "no aplica"),
            ("gzip_comp_level 7", "no aplica"),
            ("gzip_min_length 1024", "no aplica"),
            ("proxy_cache other_zone", "no aplica"),
        ] {
            let note = conv
                .notes
                .iter()
                .find(|note| note.context == context && note.directive == directive)
                .unwrap_or_else(|| panic!("missing classification for {directive}"));
            assert_eq!(
                note.severity,
                Severity::Review,
                "a discarded listener knob is a review, not {directive}: {}",
                note.message
            );
            assert!(
                note.message.contains(marker),
                "{directive} must say it is not applied: {}",
                note.message
            );
        }

        let limit_conn = conv
            .notes
            .iter()
            .find(|note| note.context == context && note.directive == "limit_conn addr 5")
            .expect("limit_conn classified");
        assert_eq!(limit_conn.severity, Severity::Review);
        assert!(
            limit_conn.message.contains("listener.maxconn"),
            "maxconn stays a manual listener value: {}",
            limit_conn.message
        );
        assert!(
            !conv
                .notes
                .iter()
                .any(|note| note.context == context && note.severity == Severity::Degraded),
            "listener-scope behavior is not a TLS security degradation"
        );
    }

    #[test]
    fn matching_secondary_listener_knobs_are_info_not_review() {
        let source = "http {\n  server {\n    listen 80 default_server;\n    server_name primary.example;\n    client_header_timeout 10s;\n    keepalive_timeout 60s;\n    proxy_connect_timeout 1s;\n    proxy_read_timeout 30s;\n    client_max_body_size 1m;\n    gzip on;\n    gzip_comp_level 5;\n    gzip_min_length 512;\n    proxy_cache shared_zone;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 80;\n    server_name secondary.example;\n    client_header_timeout 10s;\n    keepalive_timeout 60s;\n    proxy_connect_timeout 1s;\n    proxy_read_timeout 30s;\n    client_max_body_size 1m;\n    gzip on;\n    gzip_comp_level 5;\n    gzip_min_length 512;\n    proxy_cache shared_zone;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("convert");
        let context = "server secondary.example";
        for directive in [
            "client_header_timeout 10s",
            "keepalive_timeout 60s",
            "proxy_connect_timeout 1s",
            "proxy_read_timeout 30s",
            "client_max_body_size 1m",
            "gzip on",
            "gzip_comp_level 5",
            "gzip_min_length 512",
            "proxy_cache shared_zone",
        ] {
            let note = conv
                .notes
                .iter()
                .find(|note| note.context == context && note.directive == directive)
                .unwrap_or_else(|| panic!("missing classification for {directive}"));
            assert_eq!(
                note.severity,
                Severity::Info,
                "an identical value is already applied by the listener ({directive}): {}",
                note.message
            );
            assert!(
                note.message.contains("coincide"),
                "the row must name the shared value ({directive}): {}",
                note.message
            );
        }
    }

    #[test]
    fn secondary_error_pages_are_not_reported_as_listener_pages() {
        let source = "http {\n  server {\n    listen 80 default_server;\n    server_name primary.example;\n    error_page 502 /primary.html;\n    proxy_intercept_errors on;\n    location / { proxy_pass http://127.0.0.1:9000; }\n  }\n  server {\n    listen 80;\n    server_name secondary.example;\n    error_page 503 /secondary.html;\n    proxy_intercept_errors on;\n    location / { proxy_pass http://127.0.0.1:9001; }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("convert");
        let toml = super::super::emit_toml(&conv);
        assert!(
            toml.contains("file = \"primary.html\"") && !toml.contains("secondary.html"),
            "only the default_server's listener page may be emitted:\n{toml}"
        );
        let context = "server secondary.example";
        for directive in [
            "error_page 503 /secondary.html",
            "proxy_intercept_errors on",
        ] {
            let note = conv
                .notes
                .iter()
                .find(|note| note.context == context && note.directive == directive)
                .unwrap_or_else(|| panic!("missing classification for {directive}"));
            assert_eq!(note.severity, Severity::Review, "{}", note.message);
            assert!(
                note.message.contains("no se aplica"),
                "{directive} must say it is not applied: {}",
                note.message
            );
        }
    }

    #[test]
    fn location_timeouts_are_not_claimed_as_applied_to_the_listener() {
        let source = "http {\n  server {\n    listen 80;\n    server_name primary.example;\n    location / {\n      proxy_pass http://127.0.0.1:9000;\n      proxy_connect_timeout 3s;\n      proxy_read_timeout 45s;\n    }\n  }\n}\n";
        let mut conv = Conv::new();
        super::convert(&mut conv, source, None).expect("convert");
        for directive in ["proxy_connect_timeout 3s", "proxy_read_timeout 45s"] {
            let note = conv
                .notes
                .iter()
                .find(|note| note.directive == directive)
                .unwrap_or_else(|| panic!("missing classification for {directive}"));
            assert_eq!(note.severity, Severity::Review, "{}", note.message);
            assert!(
                note.message.contains("no se aplica"),
                "a location timeout must not read as applied: {}",
                note.message
            );
        }
    }

    #[test]
    fn cidr_and_variable_helpers_are_conservative() {
        assert_eq!(cidr_of("10.1.2.3"), "10.1.2.3/32");
        assert_eq!(cidr_of("::1"), "::1/128");
        assert_eq!(cidr_of("10.0.0.0/8"), "10.0.0.0/8");
        assert_eq!(nginx_variable_unsupported("$remote_addr"), None);
        assert_eq!(
            nginx_variable_unsupported("$proxy_add_x_forwarded_for"),
            Some("$proxy_add_x_forwarded_for".to_owned())
        );
    }

    fn include_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "eqproxy-nginx-include-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create include scratch dir");
        dir
    }

    fn write_include(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create include parent");
        }
        std::fs::write(path, text).expect("write include fixture");
    }

    fn expand_include_tree(
        root: &Path,
        text: &str,
        limits: IncludeLimits,
    ) -> Result<Vec<Node>, ImportError> {
        let nodes = parse(&tokenize(text)?)?;
        IncludeExpander::new(root, text.len(), limits)
            .and_then(|expander| expander.expand(nodes, "global"))
            .map(|(nodes, _)| nodes)
    }

    #[test]
    fn include_expands_globs_in_order_and_keeps_included_vhosts() {
        let root = include_scratch("vhosts");
        write_include(
            &root,
            "conf.d/20-beta.conf",
            "server {\n  listen 80;\n  server_name beta.example.test;\n  location / { proxy_pass http://127.0.0.1:9002; }\n}\n",
        );
        write_include(
            &root,
            "conf.d/10-alpha.conf",
            "server {\n  listen 80;\n  server_name alpha.example.test;\n  include snippets/location.conf;\n}\n",
        );
        write_include(
            &root,
            "snippets/location.conf",
            "location / { proxy_pass http://127.0.0.1:9001; }\n",
        );
        let source = "http {\n  include conf.d/*.conf;\n}\n";
        let outcome = super::super::convert_with_options(
            super::super::Source::Nginx,
            source,
            super::super::ImportOptions {
                include_root: Some(&root),
            },
        )
        .expect("included vhosts convert");
        for needle in [
            "host = \"alpha.example.test\"",
            "host = \"beta.example.test\"",
            "use_backend = \"backend_127_0_0_1_9001\"",
            "use_backend = \"backend_127_0_0_1_9002\"",
        ] {
            assert!(
                outcome.toml.contains(needle),
                "included vhost content must survive at its include position ({needle}):\n{}",
                outcome.toml
            );
        }
        let alpha = outcome
            .toml
            .find("host = \"alpha.example.test\"")
            .expect("alpha rule");
        let beta = outcome
            .toml
            .find("host = \"beta.example.test\"")
            .expect("beta rule");
        assert!(
            alpha < beta,
            "glob matches must be expanded in lexicographic order (10 before 20):\n{}",
            outcome.toml
        );
        assert!(
            outcome.report.contains("include expandido: 2 fichero(s)"),
            "the include itself must stay classified:\n{}",
            outcome.report
        );
        let again = super::super::convert_with_options(
            super::super::Source::Nginx,
            source,
            super::super::ImportOptions {
                include_root: Some(&root),
            },
        )
        .expect("second include pass");
        assert_eq!(again.toml, outcome.toml, "include TOML is deterministic");
        assert_eq!(
            again.report, outcome.report,
            "include report is deterministic"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn include_without_root_remains_a_manual_review_item() {
        let source = "http {\n  include conf.d/*.conf;\n  server { listen 80; location / { proxy_pass http://127.0.0.1:9000; } }\n}\n";
        let outcome = super::super::convert(super::super::Source::Nginx, source)
            .expect("convert without include root");
        assert!(
            outcome
                .report
                .contains("include no se expande: revise «conf.d/*.conf»"),
            "without the flag the old manual warning must remain:\n{}",
            outcome.report
        );
    }

    #[test]
    fn include_cycle_is_reported_with_the_chain() {
        let root = include_scratch("cycle");
        write_include(&root, "a.conf", "include b.conf;\n");
        write_include(&root, "b.conf", "include a.conf;\n");
        let source = "http {\n  include a.conf;\n  server { listen 80; location / { proxy_pass http://127.0.0.1:9000; } }\n}\n";
        let error = super::super::convert_with_options(
            super::super::Source::Nginx,
            source,
            super::super::ImportOptions {
                include_root: Some(&root),
            },
        )
        .expect_err("a cycle must fail the import");
        assert!(
            error.message.contains("ciclo detectado")
                && error.message.contains("a.conf")
                && error.message.contains("b.conf"),
            "the cycle error must name the chain: {}",
            error.message
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn include_limits_stop_depth_files_and_bytes() {
        let root = include_scratch("limits");
        for index in 0..3 {
            write_include(&root, &format!("part-{index}.conf"), "# included\n");
        }
        let depth = expand_include_tree(
            &root,
            "include part-0.conf;\n",
            IncludeLimits {
                depth: 0,
                files: 10,
                bytes: 1_024,
            },
        )
        .expect_err("depth zero rejects the first include");
        assert!(
            depth.message.contains("profundidad máxima"),
            "{}",
            depth.message
        );

        let files = expand_include_tree(
            &root,
            "include part-*.conf;\n",
            IncludeLimits {
                depth: 4,
                files: 2,
                bytes: 1_024,
            },
        )
        .expect_err("a third file exceeds the file limit");
        assert!(
            files.message.contains("máximo de 2 ficheros"),
            "{}",
            files.message
        );

        let bytes = expand_include_tree(
            &root,
            "include part-0.conf;\n",
            IncludeLimits {
                depth: 4,
                files: 10,
                bytes: 5,
            },
        )
        .expect_err("root plus included bytes exceed the byte limit");
        assert!(
            bytes.message.contains("máximo de 5 bytes"),
            "{}",
            bytes.message
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn include_rejects_missing_files_and_parent_escapes() {
        let root = include_scratch("paths");
        let missing = expand_include_tree(&root, "include missing.conf;\n", INCLUDE_LIMITS)
            .expect_err("a literal missing include is an error");
        assert!(
            missing.message.contains("no se puede leer"),
            "{}",
            missing.message
        );
        let escape = expand_include_tree(&root, "include ../outside.conf;\n", INCLUDE_LIMITS)
            .expect_err("a relative include cannot escape the root");
        assert!(
            escape.message.contains("sale de --include-root"),
            "{}",
            escape.message
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn include_reads_an_absolute_path_as_written() {
        let root = include_scratch("absolute-root");
        let included = root.join("absolute-location.conf");
        std::fs::write(
            &included,
            "location / { proxy_pass http://127.0.0.1:9010; }\n",
        )
        .expect("write absolute include");
        let source = format!(
            "http {{\n  server {{\n    listen 80;\n    include {};\n  }}\n}}\n",
            included.display()
        );
        let outcome = super::super::convert_with_options(
            super::super::Source::Nginx,
            &source,
            super::super::ImportOptions {
                include_root: Some(&root),
            },
        )
        .expect("an absolute include converts");
        assert!(
            outcome
                .toml
                .contains("use_backend = \"backend_127_0_0_1_9010\"")
                || outcome
                    .toml
                    .contains("backend = \"backend_127_0_0_1_9010\""),
            "the absolute include must contribute its location:\n{}",
            outcome.toml
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_include_glob_is_reported_not_silently_dropped() {
        let root = include_scratch("empty");
        let source = "http {\n  include conf.d/*.conf;\n  server { listen 80; location / { proxy_pass http://127.0.0.1:9000; } }\n}\n";
        let outcome = super::super::convert_with_options(
            super::super::Source::Nginx,
            source,
            super::super::ImportOptions {
                include_root: Some(&root),
            },
        )
        .expect("an empty glob is valid in nginx");
        assert!(
            outcome.report.contains("include sin coincidencias"),
            "an empty glob must be visible in the report:\n{}",
            outcome.report
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_matcher_covers_the_nginx_subset() {
        assert!(glob_match("*.conf", "10-alpha.conf"));
        assert!(!glob_match("*.conf", "10-alpha.txt"));
        assert!(glob_match("site-?.conf", "site-a.conf"));
        assert!(glob_match("site-[0-9].conf", "site-7.conf"));
        assert!(glob_match("site-[!0-9].conf", "site-x.conf"));
        assert!(!glob_match("*", ".hidden"));
        assert!(glob_match(".*", ".hidden"));
        assert!(glob_match("literal\\*.conf", "literal*.conf"));
    }
}
