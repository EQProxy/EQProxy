//! HTTP response cache E2E journeys (B-N7/H1, ADR-0034) with real sockets.
//!
//! The origin is a real TCP server with programmable routes; "served from
//! the cache" is a fact about the socket (the origin saw one request), not
//! a mock. Every H1 guarantee has a journey: session isolation, `Set-Cookie`
//! exclusion, `Vary` variants, `ETag`/`304` revalidation, the cache lock,
//! disk persistence across a restart, and the admin inventory.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use eqproxy_core::Metrics;
use eqproxy_core::shutdown;
use eqproxy_net::{
    BoundProxy, Cache, CacheSettings, ProxyMode, ResolvedBackend, ResolvedListener, ResolvedServer,
    bind, cache,
};

const WAIT: Duration = Duration::from_secs(10);

/// Programmable origin route.
#[derive(Clone)]
struct Behavior {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    etag: Option<String>,
    delay: Duration,
    /// Extra headers the origin attaches only to a `304`.
    revalidate_headers: Vec<(String, String)>,
}

impl Behavior {
    fn ok(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("content-type".to_owned(), "text/plain".to_owned()),
                ("content-length".to_owned(), body.len().to_string()),
                // A versioned origin banner the product must replace, cached
                // or not.
                ("server".to_owned(), "nginx/1.24.0".to_owned()),
            ],
            body: body.as_bytes().to_vec(),
            etag: None,
            delay: Duration::ZERO,
            revalidate_headers: Vec::new(),
        }
    }
}

struct Origin {
    addr: SocketAddr,
    dials: Arc<AtomicUsize>,
    /// Requests per path.
    path_hits: Arc<Mutex<HashMap<String, usize>>>,
    /// `If-None-Match` values the origin saw, in order.
    conditionals: Arc<Mutex<Vec<String>>>,
    behaviors: Arc<Mutex<HashMap<String, Behavior>>>,
}

impl Origin {
    fn dials(&self) -> usize {
        self.dials.load(Ordering::Relaxed)
    }

    fn hits(&self, path: &str) -> usize {
        self.path_hits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(path)
            .copied()
            .unwrap_or(0)
    }

    fn set_body(&self, path: &str, body: &str, etag: Option<&str>) {
        let mut behaviors = self
            .behaviors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(behavior) = behaviors.get_mut(path) {
            behavior.body = body.as_bytes().to_vec();
            behavior.etag = etag.map(str::to_owned);
            for (name, value) in &mut behavior.headers {
                if name == "content-length" {
                    *value = body.len().to_string();
                }
            }
        }
    }
}

fn head_value(head: &str, name: &str) -> Option<String> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_owned())
    })
}

fn head_path(head: &str) -> String {
    head.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned()
}

/// Real TCP origin: programmable per-path responses with `ETag`/`304`
/// support, per-path counters and conditional-header recording.
async fn start_origin() -> Origin {
    let behaviors: Arc<Mutex<HashMap<String, Behavior>>> = Arc::new(Mutex::new(HashMap::from([
        (
            "/static".to_owned(),
            Behavior {
                etag: Some("\"v1\"".to_owned()),
                ..Behavior::ok("static-body-v1")
            },
        ),
        (
            "/login".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("content-length".to_owned(), "5".to_owned()),
                    ("set-cookie".to_owned(), "session=abc; Path=/".to_owned()),
                ],
                body: b"login".to_vec(),
                etag: None,
                delay: Duration::ZERO,
                revalidate_headers: Vec::new(),
            },
        ),
        (
            "/vary".to_owned(),
            Behavior {
                status: 200,
                headers: vec![("vary".to_owned(), "accept-language".to_owned())],
                body: Vec::new(),
                etag: None,
                delay: Duration::ZERO,
                revalidate_headers: Vec::new(),
            },
        ),
        (
            "/slow".to_owned(),
            Behavior {
                delay: Duration::from_millis(300),
                ..Behavior::ok("slow-body")
            },
        ),
        (
            "/nostore".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("cache-control".to_owned(), "no-store".to_owned()),
                ],
                body: b"nostore".to_vec(),
                etag: None,
                delay: Duration::ZERO,
                revalidate_headers: Vec::new(),
            },
        ),
        (
            "/flaky".to_owned(),
            Behavior {
                status: 500,
                headers: vec![("content-length".to_owned(), "5".to_owned())],
                body: b"error".to_vec(),
                etag: None,
                delay: Duration::ZERO,
                revalidate_headers: Vec::new(),
            },
        ),
        (
            "/poison".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("cache-control".to_owned(), "max-age=60".to_owned()),
                ],
                body: b"poison-body".to_vec(),
                etag: Some("\"p1\"".to_owned()),
                delay: Duration::ZERO,
                revalidate_headers: vec![
                    ("set-cookie".to_owned(), "leak=1; Path=/".to_owned()),
                    ("cache-control".to_owned(), "private".to_owned()),
                    ("x-audit-304".to_owned(), "yes".to_owned()),
                ],
            },
        ),
        (
            "/poison-sticky".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("cache-control".to_owned(), "max-age=60".to_owned()),
                ],
                body: b"poison-body".to_vec(),
                etag: Some("\"pst1\"".to_owned()),
                delay: Duration::ZERO,
                revalidate_headers: vec![
                    ("set-cookie".to_owned(), "leak=1; Path=/".to_owned()),
                    ("cache-control".to_owned(), "private".to_owned()),
                    ("x-audit-304".to_owned(), "yes".to_owned()),
                ],
            },
        ),
        (
            // Same revocation, slow enough for a second client to become a
            // cache-lock waiter while the owner is at the origin.
            "/poison-slow".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("cache-control".to_owned(), "max-age=60".to_owned()),
                ],
                body: b"poison-body".to_vec(),
                etag: Some("\"ps1\"".to_owned()),
                delay: Duration::from_millis(250),
                revalidate_headers: vec![
                    ("set-cookie".to_owned(), "leak=1; Path=/".to_owned()),
                    ("cache-control".to_owned(), "private".to_owned()),
                    ("x-audit-304".to_owned(), "yes".to_owned()),
                ],
            },
        ),
        (
            "/vary-change".to_owned(),
            Behavior {
                status: 200,
                headers: vec![
                    ("content-type".to_owned(), "text/plain".to_owned()),
                    ("cache-control".to_owned(), "max-age=60".to_owned()),
                    ("vary".to_owned(), "accept-language".to_owned()),
                ],
                body: b"vary-body".to_vec(),
                etag: Some("\"vc1\"".to_owned()),
                delay: Duration::ZERO,
                revalidate_headers: vec![("vary".to_owned(), "accept-encoding".to_owned())],
            },
        ),
    ])));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind origin");
    let addr = listener.local_addr().expect("origin addr");
    let dials = Arc::new(AtomicUsize::new(0));
    let path_hits = Arc::new(Mutex::new(HashMap::new()));
    let conditionals = Arc::new(Mutex::new(Vec::new()));
    let task_dials = Arc::clone(&dials);
    let task_hits = Arc::clone(&path_hits);
    let task_conditionals = Arc::clone(&conditionals);
    let task_behaviors = Arc::clone(&behaviors);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            task_dials.fetch_add(1, Ordering::Relaxed);
            let hits = Arc::clone(&task_hits);
            let conditionals = Arc::clone(&task_conditionals);
            let behaviors = Arc::clone(&task_behaviors);
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let Ok(read) = socket.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    head.extend_from_slice(&chunk[..read]);
                    if head.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let head_text = String::from_utf8_lossy(&head).into_owned();
                let path = head_path(&head_text);
                hits.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .entry(path.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                let seen = hits
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&path)
                    .copied()
                    .unwrap_or(0);
                if let Some(value) = head_value(&head_text, "if-none-match") {
                    conditionals
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(value);
                }
                let behavior = behaviors
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| Behavior::ok("default"));
                if !behavior.delay.is_zero() {
                    tokio::time::sleep(behavior.delay).await;
                }
                // Deliberately broken origin: after the first 200, every
                // request gets a 304, including the proxy's unconditional
                // retry. The proxy must answer 502, not forward the 304.
                if path == "/poison-sticky" && seen >= 2 {
                    let etag = behavior.etag.clone().unwrap_or_default();
                    let mut reply = format!("HTTP/1.1 304 Not Modified\r\netag: {etag}\r\n");
                    for (name, value) in &behavior.revalidate_headers {
                        reply.push_str(&format!("{name}: {value}\r\n"));
                    }
                    reply.push_str("content-length: 0\r\n\r\n");
                    let _ = socket.write_all(reply.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    return;
                }
                // Conditional request against a matching ETag: 304 with the
                // validators, no body — the exact exchange the proxy must
                // turn into a refresh instead of a download.
                let inm = head_value(&head_text, "if-none-match");
                if let (Some(inm), Some(etag)) = (inm, behavior.etag.clone())
                    && cache::client_fresh(&inm, Some(&etag))
                {
                    let mut reply = format!("HTTP/1.1 304 Not Modified\r\netag: {etag}\r\n");
                    for (name, value) in &behavior.revalidate_headers {
                        reply.push_str(&format!("{name}: {value}\r\n"));
                    }
                    reply.push_str("content-length: 0\r\n\r\n");
                    let _ = socket.write_all(reply.as_bytes()).await;
                    let _ = socket.shutdown().await;
                    return;
                }
                // `/vary` answers in the client's language: the proxy must
                // key the variants, not the path alone.
                let mut body = behavior.body.clone();
                let mut headers = behavior.headers.clone();
                if path == "/vary" {
                    let lang = head_value(&head_text, "accept-language").unwrap_or_default();
                    body = format!("lang={lang}").into_bytes();
                    headers.push(("content-length".to_owned(), body.len().to_string()));
                }
                if let Some(etag) = &behavior.etag {
                    headers.push(("etag".to_owned(), etag.clone()));
                }
                let mut reply = format!("HTTP/1.1 {} test\r\n", behavior.status).into_bytes();
                for (name, value) in &headers {
                    reply.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
                }
                if !headers.iter().any(|(n, _)| n == "content-length") {
                    reply.extend_from_slice(
                        format!("content-length: {}\r\n", body.len()).as_bytes(),
                    );
                }
                reply.extend_from_slice(b"\r\n");
                reply.extend_from_slice(&body);
                let _ = socket.write_all(&reply).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    Origin {
        addr,
        dials,
        path_hits,
        conditionals,
        behaviors,
    }
}

fn cache_settings(listener: &str) -> CacheSettings {
    CacheSettings {
        listener: listener.to_owned(),
        ..CacheSettings::test_defaults()
    }
}

async fn start_proxy(
    origin: SocketAddr,
    cache: CacheSettings,
) -> (
    SocketAddr,
    Arc<Metrics>,
    shutdown::ShutdownHandle,
    tokio::task::JoinHandle<()>,
) {
    start_proxy_with(origin, cache, None).await
}

async fn start_proxy_with(
    origin: SocketAddr,
    cache: CacheSettings,
    backend: Option<Arc<ResolvedBackend>>,
) -> (
    SocketAddr,
    Arc<Metrics>,
    shutdown::ShutdownHandle,
    tokio::task::JoinHandle<()>,
) {
    let (addr, metrics, _cache, handle, serve) =
        start_proxy_with_cache(origin, cache, backend).await;
    (addr, metrics, handle, serve)
}

async fn start_proxy_with_cache(
    origin: SocketAddr,
    cache: CacheSettings,
    backend: Option<Arc<ResolvedBackend>>,
) -> (
    SocketAddr,
    Arc<Metrics>,
    Arc<Cache>,
    shutdown::ShutdownHandle,
    tokio::task::JoinHandle<()>,
) {
    let metrics = Arc::new(Metrics::default());
    let cache = Cache::load(cache, Arc::clone(&metrics)).expect("cache builds");
    let mut cfg = ResolvedListener::new(
        "web",
        "127.0.0.1:0".parse().expect("listen"),
        "api",
        origin,
        ProxyMode::Http,
        64,
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .with_cache(Arc::clone(&cache));
    if let Some(backend) = backend {
        cfg = cfg.with_backend(backend);
    }
    let bound: BoundProxy = bind(vec![cfg], Arc::clone(&metrics))
        .await
        .expect("bind proxy");
    let addr = bound.local_addrs()[0].1;
    let (handle, signal) = shutdown::channel();
    let serve = tokio::spawn(async move {
        bound.serve(signal, WAIT).await.expect("serve");
    });
    (addr, metrics, cache, handle, serve)
}

fn cookie_backend(origin: SocketAddr) -> Arc<ResolvedBackend> {
    use eqproxy_net::{AffinityMode, ResolvedAffinity};
    Arc::new(
        ResolvedBackend::new(
            "api",
            vec![ResolvedServer {
                addr: origin.into(),
                source: None,
                weight: 1,
                backup: false,
                priority: 0,
                maxconn: 0,
            }],
            3,
            2,
        )
        .with_affinity(ResolvedAffinity {
            mode: AffinityMode::Cookie,
            cookie: "eq_aff".to_owned(),
        }),
    )
}

/// Sends one request with `connection: close` and returns status, head and
/// body separately.
async fn fetch(
    addr: SocketAddr,
    method: &str,
    path: &str,
    extra: &[(&str, &str)],
) -> (u16, String, Vec<u8>) {
    let mut socket = TcpStream::connect(addr).await.expect("connect proxy");
    let mut request = format!("{method} {path} HTTP/1.1\r\nhost: api\r\nconnection: close\r\n");
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    socket
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    let mut response = Vec::new();
    tokio::time::timeout(WAIT, socket.read_to_end(&mut response))
        .await
        .expect("response in time")
        .expect("read response");
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
        .unwrap_or(response.len());
    let head = String::from_utf8_lossy(&response[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .expect("status line");
    (status, head, response[split..].to_vec())
}

fn header(head: &str, name: &str) -> Option<String> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_owned())
    })
}

#[tokio::test]
async fn miss_then_hit_serves_identical_bytes() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (status, head, first) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(header(&head, "server").as_deref(), Some("EQProxy"));
    assert_eq!(origin.hits("/static"), 1);

    let (status, head, second) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(first, second);
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(
        header(&head, "server").as_deref(),
        Some("EQProxy"),
        "a cached entry serves the product banner, not the stored nginx one"
    );
    assert!(header(&head, "age").is_some(), "a hit carries its age");
    assert_eq!(origin.hits("/static"), 1, "the second request never dialed");

    let snap = metrics.snapshot();
    assert_eq!(snap.cache_hits, 1);
    assert_eq!(snap.cache_misses, 1);
    assert_eq!(snap.cache_stores, 1);
    assert_eq!(snap.http_requests_proxied, 2);
}

#[tokio::test]
async fn sessions_never_share_an_entry() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    // Ten concurrent sessions plus one anonymous request: every sessioned
    // request bypasses, so the origin sees all eleven.
    let mut tasks = Vec::new();
    for index in 0..10 {
        tasks.push(tokio::spawn(async move {
            fetch(
                proxy,
                "GET",
                "/static",
                &[("cookie", &format!("session={index}"))],
            )
            .await
        }));
    }
    tasks.push(tokio::spawn(async move {
        fetch(proxy, "GET", "/static", &[]).await
    }));
    for task in tasks {
        let (status, _, _) = task.await.expect("request finishes");
        assert_eq!(status, 200);
    }
    assert_eq!(origin.hits("/static"), 11, "no session saw another's bytes");

    // The anonymous request stored its entry: the next one hits.
    let (_, head, _) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 11);
}

#[tokio::test]
async fn set_cookie_responses_are_never_stored() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    for _ in 0..2 {
        let (status, head, _) = fetch(proxy, "GET", "/login", &[]).await;
        assert_eq!(status, 200);
        assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    }
    assert_eq!(origin.hits("/login"), 2);
    assert_eq!(metrics.snapshot().cache_stores, 0);
}

#[tokio::test]
async fn vary_selects_the_variant() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (_, _, english) = fetch(proxy, "GET", "/vary", &[("accept-language", "en")]).await;
    assert_eq!(english, b"lang=en");
    let (_, head, english_again) = fetch(proxy, "GET", "/vary", &[("accept-language", "en")]).await;
    assert_eq!(english_again, b"lang=en");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));

    let (_, _, french) = fetch(proxy, "GET", "/vary", &[("accept-language", "fr")]).await;
    assert_eq!(french, b"lang=fr");
    assert_eq!(origin.hits("/vary"), 2, "one fetch per variant");

    // No language at all is its own variant, not the English one.
    let (_, _, empty) = fetch(proxy, "GET", "/vary", &[]).await;
    assert_eq!(empty, b"lang=");
    assert_eq!(origin.hits("/vary"), 3);
}

#[tokio::test]
async fn etag_revalidation_refreshes_without_a_body() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (_, _, first) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(first, b"static-body-v1");

    // A client `no-cache` forces the revalidation path deterministically
    // (no sleeps): the origin answers `304`, the client keeps the body.
    let (_, head, second) = fetch(proxy, "GET", "/static", &[("cache-control", "no-cache")]).await;
    assert_eq!(second, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("REVALIDATED".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
    let conditionals = origin
        .conditionals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(conditionals, vec!["\"v1\"".to_owned()]);
    assert_eq!(metrics.snapshot().cache_revalidations, 1);

    // The origin rotates the representation: the next revalidation
    // replaces the entry instead of refreshing it.
    origin.set_body("/static", "static-body-v2", Some("\"v2\""));
    let (_, _, third) = fetch(proxy, "GET", "/static", &[("cache-control", "no-cache")]).await;
    assert_eq!(third, b"static-body-v2");
    assert_eq!(origin.hits("/static"), 3);

    // And the new representation serves from the store now.
    let (_, head, fourth) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(fourth, b"static-body-v2");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 3);
}

#[tokio::test]
async fn client_max_age_zero_revalidates_without_poisoning_the_shared_entry() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (_, _, warm) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(warm, b"static-body-v1");
    assert_eq!(origin.hits("/static"), 1);

    // max-age=0 asks for an origin check even though the entry is fresh.
    let (status, head, body) =
        fetch(proxy, "GET", "/static", &[("cache-control", "max-age=0")]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("REVALIDATED".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
    assert_eq!(metrics.snapshot().cache_revalidations, 1);

    // The directive was scoped to that request: the next anonymous client
    // still receives the shared fresh entry, not a forced revalidation.
    let (_, head, body) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 2);

    // A max-age above the stored age is also a HIT, not a revalidation.
    let (_, head, body) = fetch(proxy, "GET", "/static", &[("cache-control", "max-age=120")]).await;
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
}

#[tokio::test]
async fn client_min_fresh_revalidates_only_when_the_remaining_lifetime_is_too_short() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(origin.hits("/static"), 1);

    // Default TTL is 60s: 30s of remaining freshness is satisfiable.
    let (_, head, body) = fetch(
        proxy,
        "GET",
        "/static",
        &[("cache-control", "min-fresh=30")],
    )
    .await;
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 1);

    // 120s cannot be satisfied by a 60s entry: revalidate at the origin.
    let (status, head, body) = fetch(
        proxy,
        "GET",
        "/static",
        &[("cache-control", "min-fresh=120")],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("REVALIDATED".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
}

#[tokio::test]
async fn client_max_stale_serves_only_within_the_requested_margin() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(
        origin.addr,
        CacheSettings {
            default_ttl: Duration::from_secs(1),
            ..cache_settings("web")
        },
    )
    .await;

    fetch(proxy, "GET", "/static", &[]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // The entry is stale by roughly one second; the client accepts stale up
    // to ten seconds, so it is served without dialing the origin.
    let (status, head, body) = fetch(
        proxy,
        "GET",
        "/static",
        &[("cache-control", "max-stale=10")],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/static"), 1);

    // max-stale=0 accepts no staleness: the stale entry is revalidated.
    let (status, head, body) =
        fetch(proxy, "GET", "/static", &[("cache-control", "max-stale=0")]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("REVALIDATED".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
}

#[tokio::test]
async fn max_age_zero_is_per_request_even_for_lock_waiters() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (_, _, warm) = fetch(proxy, "GET", "/slow", &[]).await;
    assert_eq!(warm, b"slow-body");
    assert_eq!(origin.hits("/slow"), 1);

    // The owner re-fetches under max-age=0. `/slow` has no validators, so
    // the request takes the full-fetch path while the entry stays fresh.
    let owner = tokio::spawn(async move {
        fetch(proxy, "GET", "/slow", &[("cache-control", "max-age=0")]).await
    });
    // `/slow` sleeps 300 ms after the origin receives the request. Wait for
    // that receipt instead of guessing with a sleep: the waiter below is
    // then guaranteed to meet the owner's in-flight cache lock.
    let mut owner_at_origin = false;
    for _ in 0..200 {
        if origin.hits("/slow") >= 2 {
            owner_at_origin = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        owner_at_origin,
        "owner reached the origin before the waiter"
    );
    // A second max-age=0 client must wait on the cache lock, while an
    // anonymous client may keep serving the still-fresh shared entry.
    let waiter = tokio::spawn(async move {
        fetch(proxy, "GET", "/slow", &[("cache-control", "max-age=0")]).await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let (status, head, body) = fetch(proxy, "GET", "/slow", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"slow-body");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));

    let (_, owner_head, owner_body) = owner.await.expect("owner finishes");
    let (_, waiter_head, waiter_body) = waiter.await.expect("waiter finishes");
    assert_eq!(owner_body, b"slow-body");
    assert_eq!(waiter_body, b"slow-body");
    assert_eq!(header(&owner_head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(header(&waiter_head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(origin.hits("/slow"), 3, "warm + owner + waiter");

    // The waiter's directive did not shorten the replacement's freshness.
    let (_, head, body) = fetch(proxy, "GET", "/slow", &[]).await;
    assert_eq!(body, b"slow-body");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(origin.hits("/slow"), 3);
}

#[tokio::test]
async fn a_304_with_session_headers_never_pollutes_the_shared_entry() {
    let origin = start_origin().await;
    let (proxy, metrics, cache, _handle, _serve) =
        start_proxy_with_cache(origin.addr, cache_settings("web"), None).await;

    let (_, head, body) = fetch(proxy, "GET", "/poison", &[]).await;
    assert_eq!(body, b"poison-body");
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(origin.hits("/poison"), 1);

    // The 304 revokes shared storability. The proxy must not serve the old
    // body under the new policy: it removes the entry and fetches in full.
    let (_, head, body) = fetch(proxy, "GET", "/poison", &[("cache-control", "no-cache")]).await;
    assert_eq!(body, b"poison-body");
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(header(&head, "set-cookie"), None);
    assert_eq!(
        header(&head, "cache-control").as_deref(),
        Some("max-age=60")
    );
    assert_eq!(header(&head, "x-audit-304"), None);
    assert_eq!(origin.hits("/poison"), 3, "304 + unconditional fetch");

    // The clean 200 is stored without the 304's headers: the next anonymous
    // client gets a hit that cannot contain the session material.
    let (_, head, body) = fetch(proxy, "GET", "/poison", &[]).await;
    assert_eq!(body, b"poison-body");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(header(&head, "set-cookie"), None);
    assert_eq!(header(&head, "x-audit-304"), None);
    assert_eq!(origin.hits("/poison"), 3);
    assert_eq!(metrics.snapshot().cache_revalidations, 0);
    assert_eq!(cache.cache_304_invalidations(), 1);
    assert_eq!(metrics.snapshot().cache_stores, 2);
}

#[tokio::test]
async fn a_second_304_to_the_unconditional_retry_is_a_bad_gateway() {
    let origin = start_origin().await;
    let (proxy, _metrics, cache, _handle, _serve) =
        start_proxy_with_cache(origin.addr, cache_settings("web"), None).await;

    let (_, _, first) = fetch(proxy, "GET", "/poison-sticky", &[]).await;
    assert_eq!(first, b"poison-body");

    // The 304 revokes storability. The retry is unconditional, but this
    // origin answers 304 again: the proxy must fail explicitly, not forward
    // a 304 to a client that never sent a conditional request.
    // Client freshness directives are present, but the revoked `304` still
    // wins: the unconditional retry is made and its second `304` is a 502.
    let (status, head, _) = fetch(
        proxy,
        "GET",
        "/poison-sticky",
        &[("cache-control", "max-age=0, min-fresh=60, max-stale=600")],
    )
    .await;
    assert_eq!(status, 502);
    assert_eq!(header(&head, "set-cookie"), None);
    assert_eq!(origin.hits("/poison-sticky"), 3, "warm + 304 + retry");
    assert_eq!(cache.cache_304_invalidations(), 1);
}

#[tokio::test]
async fn a_304_that_changes_vary_invalidates_the_entry() {
    let origin = start_origin().await;
    let (proxy, metrics, cache, _handle, _serve) =
        start_proxy_with_cache(origin.addr, cache_settings("web"), None).await;

    let (_, _, first) = fetch(proxy, "GET", "/vary-change", &[]).await;
    assert_eq!(first, b"vary-body");
    assert_eq!(origin.hits("/vary-change"), 1);

    // The 304 announces a different Vary. The stored key cannot be
    // re-keyed in place: the entry is removed and the proxy fetches the
    // 200 in full, whose own Vary keys the replacement.
    let (_, head, second) = fetch(
        proxy,
        "GET",
        "/vary-change",
        &[("cache-control", "no-cache")],
    )
    .await;
    assert_eq!(second, b"vary-body");
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(header(&head, "vary").as_deref(), Some("accept-language"));
    assert_eq!(origin.hits("/vary-change"), 3);

    // A different language is a different variant of the clean 200, not a
    // reuse of the empty-negotiation entry.
    let (_, head, third) = fetch(proxy, "GET", "/vary-change", &[("accept-language", "fr")]).await;
    assert_eq!(third, b"vary-body");
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(origin.hits("/vary-change"), 4);
    assert_eq!(metrics.snapshot().cache_revalidations, 0);
    assert_eq!(cache.cache_304_invalidations(), 1);
    assert_eq!(metrics.snapshot().cache_stores, 3);
}

#[tokio::test]
async fn a_revoking_304_never_serves_new_headers_to_a_lock_waiter() {
    let origin = start_origin().await;
    let (proxy, metrics, cache, _handle, _serve) =
        start_proxy_with_cache(origin.addr, cache_settings("web"), None).await;

    let (_, _, warm) = fetch(proxy, "GET", "/poison-slow", &[]).await;
    assert_eq!(warm, b"poison-body");
    assert_eq!(origin.hits("/poison-slow"), 1);

    // The owner sleeps in the origin; the second client must wait on the
    // cache lock rather than start a second revalidation. When the owner
    // publishes `Gone`, the waiter fetches directly (or wakes to the clean
    // replacement) — never the old body under the revoked headers.
    let owner = tokio::spawn(async move {
        fetch(
            proxy,
            "GET",
            "/poison-slow",
            &[("cache-control", "no-cache")],
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    let waiter = tokio::spawn(async move {
        fetch(
            proxy,
            "GET",
            "/poison-slow",
            &[("cache-control", "no-cache")],
        )
        .await
    });

    let (_, owner_head, owner_body) = owner.await.expect("owner finishes");
    let (_, waiter_head, waiter_body) = waiter.await.expect("waiter finishes");
    assert_eq!(owner_body, b"poison-body");
    assert_eq!(waiter_body, b"poison-body");
    assert_eq!(header(&owner_head, "x-cache"), Some("MISS".to_owned()));
    for head in [&owner_head, &waiter_head] {
        assert_eq!(header(head, "set-cookie"), None, "{head}");
        assert_eq!(header(head, "x-audit-304"), None, "{head}");
        assert_eq!(
            header(head, "cache-control").as_deref(),
            Some("max-age=60"),
            "{head}"
        );
        assert!(
            matches!(header(head, "x-cache").as_deref(), Some("MISS" | "HIT")),
            "{head}"
        );
    }
    assert!(
        origin.hits("/poison-slow") >= 3,
        "warm + 304 + at least one full fetch"
    );
    assert_eq!(cache.cache_304_invalidations(), 1);
    assert_eq!(metrics.snapshot().cache_revalidations, 0);
}

#[tokio::test]
async fn entries_expire_after_their_ttl() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(
        origin.addr,
        CacheSettings {
            default_ttl: Duration::from_secs(1),
            ..cache_settings("web")
        },
    )
    .await;

    fetch(proxy, "GET", "/static", &[]).await;
    let (_, head, _) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    tokio::time::sleep(Duration::from_secs(2)).await;
    // Expired with validators: the origin revalidates with a `304` and the
    // client never sees the expiry.
    let (status, head, body) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("REVALIDATED".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
}

#[tokio::test]
async fn concurrent_requests_share_one_origin_fetch() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let mut tasks = Vec::new();
    for _ in 0..10 {
        tasks.push(tokio::spawn(async move {
            fetch(proxy, "GET", "/slow", &[]).await
        }));
    }
    let mut hits = 0;
    for task in tasks {
        let (status, head, body) = task.await.expect("request finishes");
        assert_eq!(status, 200);
        assert_eq!(body, b"slow-body");
        if header(&head, "x-cache") == Some("HIT".to_owned()) {
            hits += 1;
        }
    }
    assert_eq!(origin.hits("/slow"), 1, "the lock coalesced the burst");
    assert_eq!(origin.dials(), 1, "one pooled connection served the burst");
    assert_eq!(hits, 9, "every waiter served from the store");
}

#[tokio::test]
async fn head_is_served_from_the_get_entry() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    fetch(proxy, "GET", "/static", &[]).await;
    let (status, head, body) = fetch(proxy, "HEAD", "/static", &[]).await;
    assert_eq!(status, 200);
    assert!(body.is_empty(), "a HEAD has no body");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(
        header(&head, "content-length"),
        Some("static-body-v1".len().to_string())
    );
    assert_eq!(origin.hits("/static"), 1);
}

#[tokio::test]
async fn authorization_and_ranges_bypass() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    for _ in 0..2 {
        let (status, head, _) = fetch(
            proxy,
            "GET",
            "/static",
            &[("authorization", "Bearer secret")],
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(header(&head, "x-cache"), None);
    }
    let (status, _, _) = fetch(proxy, "GET", "/static", &[("range", "bytes=0-3")]).await;
    assert_eq!(status, 200);
    assert_eq!(origin.hits("/static"), 3, "nothing was served nor stored");
    assert_eq!(metrics.snapshot().cache_bypasses, 3);
    assert_eq!(metrics.snapshot().cache_stores, 0);

    // The bypasses poisoned nothing: a plain request stores and hits.
    fetch(proxy, "GET", "/static", &[]).await;
    let (_, head, _) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
}

#[tokio::test]
async fn unstorable_statuses_and_directives_stream_through() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    for _ in 0..2 {
        let (status, _, _) = fetch(proxy, "GET", "/flaky", &[]).await;
        assert_eq!(status, 500);
    }
    assert_eq!(origin.hits("/flaky"), 2, "errors are never stored");
    for _ in 0..2 {
        fetch(proxy, "GET", "/nostore", &[]).await;
    }
    assert_eq!(origin.hits("/nostore"), 2);
    assert_eq!(metrics.snapshot().cache_stores, 0);
}

#[tokio::test]
async fn oversized_bodies_stream_through_uncached() {
    let origin = start_origin().await;
    let (proxy, _, _handle, _serve) = start_proxy(
        origin.addr,
        CacheSettings {
            // Below the 14-byte `/static` body: nothing is ever stored.
            max_entry_bytes: 4,
            ..cache_settings("web")
        },
    )
    .await;

    let (_, head, body) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(body, b"static-body-v1");
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    let (_, head, _) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(header(&head, "x-cache"), Some("MISS".to_owned()));
    assert_eq!(origin.hits("/static"), 2);
}

#[tokio::test]
async fn cookie_affinity_disables_the_cache_at_runtime() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy_with(
        origin.addr,
        cache_settings("web"),
        Some(cookie_backend(origin.addr)),
    )
    .await;

    for _ in 0..3 {
        let (status, head, _) = fetch(proxy, "GET", "/static", &[]).await;
        assert_eq!(status, 200);
        // No marker at all: the lookup never ran, so the answer is not a
        // "miss" either — it bypassed entirely.
        assert_eq!(header(&head, "x-cache"), None);
    }
    assert_eq!(origin.hits("/static"), 3);
    assert_eq!(metrics.snapshot().cache_bypasses, 3);
    assert_eq!(metrics.snapshot().cache_stores, 0);
}

#[tokio::test]
async fn disk_entries_survive_a_restart() {
    let dir = std::env::temp_dir().join(format!("eqproxy-cache-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let origin = start_origin().await;

    let (proxy, _, handle, serve) = start_proxy(
        origin.addr,
        CacheSettings {
            disk_dir: Some(dir.clone()),
            ..cache_settings("web")
        },
    )
    .await;
    let (_, _, first) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(first, b"static-body-v1");
    handle.trigger();
    serve.await.expect("proxy stops");

    // "Restart": a new cache on the same directory, with the origin gone
    // dark (every dial fails). A hit proves the bytes came from disk.
    origin.set_body("/static", "static-body-v2", Some("\"v2\""));
    let failing = {
        use eqproxy_net::ResolvedServer;
        Arc::new(ResolvedBackend::new(
            "api",
            vec![ResolvedServer {
                addr: eqproxy_net::ServerAddr::Ip(
                    "127.0.0.2:9".parse::<SocketAddr>().expect("dead addr"),
                ),
                source: None,
                weight: 1,
                backup: false,
                priority: 0,
                maxconn: 0,
            }],
            3,
            2,
        ))
    };
    let metrics = Arc::new(Metrics::default());
    let cache = Cache::load(
        CacheSettings {
            disk_dir: Some(dir.clone()),
            ..cache_settings("web")
        },
        Arc::clone(&metrics),
    )
    .expect("disk reloads");
    let cfg = ResolvedListener::new(
        "web",
        "127.0.0.1:0".parse().expect("listen"),
        "api",
        origin.addr,
        ProxyMode::Http,
        64,
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .with_cache(cache)
    .with_backend(failing);
    let bound: BoundProxy = bind(vec![cfg], Arc::clone(&metrics))
        .await
        .expect("bind proxy");
    let addr = bound.local_addrs()[0].1;
    let (handle, signal) = shutdown::channel();
    let serve = tokio::spawn(async move {
        bound.serve(signal, WAIT).await.expect("serve");
    });
    let (status, head, body) = fetch(addr, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"static-body-v1", "the bytes came from disk, not v2");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert!(
        metrics.snapshot().cache_disk_loads >= 1,
        "the reload restored the entry"
    );
    handle.trigger();
    serve.await.expect("proxy stops");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_revoked_304_entry_does_not_resurrect_from_disk() {
    let dir =
        std::env::temp_dir().join(format!("eqproxy-cache-e2e-revoked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let origin = start_origin().await;

    let (proxy, _, handle, serve) = start_proxy(
        origin.addr,
        CacheSettings {
            disk_dir: Some(dir.clone()),
            ..cache_settings("web")
        },
    )
    .await;
    let (_, _, first) = fetch(proxy, "GET", "/poison", &[]).await;
    assert_eq!(first, b"poison-body");
    let (_, _, second) = fetch(proxy, "GET", "/poison", &[("cache-control", "no-cache")]).await;
    assert_eq!(second, b"poison-body");
    assert_eq!(origin.hits("/poison"), 3);

    // The 304's cookie/private/x-* must not have reached disk either.
    let mut persisted = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("disk dir") {
        let path = entry.expect("dir entry").path();
        if path.is_file() {
            persisted.extend_from_slice(&std::fs::read(&path).expect("read cache file"));
        }
    }
    let persisted = String::from_utf8_lossy(&persisted);
    assert!(!persisted.contains("leak=1"), "304 cookie persisted");
    assert!(!persisted.contains("x-audit-304"), "304 header persisted");

    handle.trigger();
    serve.await.expect("proxy stops");

    // Restart with the origin dark: the clean 200 must load from disk; a
    // revoked entry would either be absent or carry the poison.
    let failing = {
        use eqproxy_net::ResolvedServer;
        Arc::new(ResolvedBackend::new(
            "api",
            vec![ResolvedServer {
                addr: eqproxy_net::ServerAddr::Ip(
                    "127.0.0.2:9".parse::<SocketAddr>().expect("dead addr"),
                ),
                source: None,
                weight: 1,
                backup: false,
                priority: 0,
                maxconn: 0,
            }],
            3,
            2,
        ))
    };
    let metrics = Arc::new(Metrics::default());
    let cache = Cache::load(
        CacheSettings {
            disk_dir: Some(dir.clone()),
            ..cache_settings("web")
        },
        Arc::clone(&metrics),
    )
    .expect("disk reloads");
    let cfg = ResolvedListener::new(
        "web",
        "127.0.0.1:0".parse().expect("listen"),
        "api",
        origin.addr,
        ProxyMode::Http,
        64,
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .with_cache(cache)
    .with_backend(failing);
    let bound: BoundProxy = bind(vec![cfg], Arc::clone(&metrics))
        .await
        .expect("bind proxy");
    let addr = bound.local_addrs()[0].1;
    let (handle, signal) = shutdown::channel();
    let serve = tokio::spawn(async move {
        bound.serve(signal, WAIT).await.expect("serve");
    });
    let (status, head, body) = fetch(addr, "GET", "/poison", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(body, b"poison-body");
    assert_eq!(header(&head, "x-cache"), Some("HIT".to_owned()));
    assert_eq!(header(&head, "set-cookie"), None);
    assert_eq!(
        header(&head, "cache-control").as_deref(),
        Some("max-age=60")
    );
    assert_eq!(header(&head, "x-audit-304"), None);
    assert!(metrics.snapshot().cache_disk_loads >= 1);
    handle.trigger();
    serve.await.expect("proxy stops");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Pulls one full message (head + `Content-Length` body) out of `buffer`,
/// which may already hold bytes of the next pipelined response. `None`
/// means the buffer does not yet carry a complete message.
fn take_message(buffer: &mut Vec<u8>) -> Option<(String, Vec<u8>)> {
    let split = buffer.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let head = String::from_utf8_lossy(&buffer[..split]).into_owned();
    let len = head
        .lines()
        .skip(1)
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .expect("served responses carry a content-length");
    if buffer.len() < split + len {
        return None;
    }
    let body = buffer[split..split + len].to_vec();
    buffer.drain(..split + len);
    Some((head, body))
}

#[tokio::test]
async fn pipelined_hits_keep_the_next_request_on_the_connection() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    // Warm the entry over a throwaway connection.
    let (status, _, _) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(origin.hits("/static"), 1);

    // Both GETs travel in one write. The first is answered from the cache
    // without dialing, and the bytes already queued for the second must
    // survive that local reply instead of being dropped with the exchange.
    let mut socket = TcpStream::connect(proxy).await.expect("connect proxy");
    socket
        .write_all(
            b"GET /static HTTP/1.1\r\nhost: api\r\n\r\n\
              GET /static HTTP/1.1\r\nhost: api\r\nconnection: close\r\n\r\n",
        )
        .await
        .expect("write pipeline");

    let mut buffer = Vec::new();
    let mut responses: Vec<(String, Vec<u8>)> = Vec::new();
    while responses.len() < 2 {
        let mut chunk = [0u8; 4096];
        let read = tokio::time::timeout(WAIT, socket.read(&mut chunk))
            .await
            .expect("response in time")
            .expect("read response");
        assert!(
            read > 0,
            "connection ended after {} of two pipelined responses",
            responses.len()
        );
        buffer.extend_from_slice(&chunk[..read]);
        while let Some(message) = take_message(&mut buffer) {
            responses.push(message);
        }
    }

    assert_eq!(header(&responses[0].0, "x-cache").as_deref(), Some("HIT"));
    assert_eq!(header(&responses[1].0, "x-cache").as_deref(), Some("HIT"));
    assert_eq!(responses[0].1, b"static-body-v1");
    assert_eq!(responses[1].1, b"static-body-v1");
    assert_eq!(
        origin.hits("/static"),
        1,
        "neither cached reply may dial the origin"
    );
    assert_eq!(metrics.snapshot().cache_hits, 2);
    assert_eq!(metrics.snapshot().cache_misses, 1);
}

#[tokio::test]
async fn a_hit_serves_the_stored_bytes_after_the_origin_changes() {
    let origin = start_origin().await;
    let (proxy, metrics, _handle, _serve) = start_proxy(origin.addr, cache_settings("web")).await;

    let (status, _, first) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(first, b"static-body-v1");
    assert_eq!(origin.hits("/static"), 1);

    // The origin changes; the stored entry must keep the bytes the proxy
    // assembled on the miss, not pick up the new body behind its back.
    origin.set_body("/static", "static-body-v2", Some("\"v2\""));

    let (status, head, second) = fetch(proxy, "GET", "/static", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(header(&head, "x-cache").as_deref(), Some("HIT"));
    assert_eq!(second, b"static-body-v1");
    assert_eq!(origin.hits("/static"), 1, "the hit must not dial");
    assert_eq!(metrics.snapshot().cache_stores, 1);
    assert_eq!(metrics.snapshot().cache_hits, 1);
}
