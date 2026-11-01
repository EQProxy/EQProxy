//! Backend Unix-domain sockets and per-server `source` bind E2E
//! (ADR-0043), with **real sockets**: a real `UnixListener` origin, the real
//! proxy binary code path, and real clients over TCP.
//!
//! What each test proves:
//!
//! - a `mode = "tcp"` listener relays bytes to a Unix backend;
//! - a `mode = "http"` listener performs a real HTTP/1.1 exchange against it,
//!   and the keep-alive pool keys the connection by the Unix endpoint (the
//!   second request reuses it instead of re-dialling);
//! - a missing socket path fails explicitly and fast (502 + counted connect
//!   failure), never a hang or a retry storm;
//! - `source` binds the outbound socket (the connect succeeds from the chosen
//!   local address) and an unusable source is an explicit connect failure;
//! - the health checker speaks to a Unix endpoint (HTTP check over the
//!   socket), which is what makes `fall`/`rise` work for these servers.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener};
use tokio::task::JoinHandle;

use eqproxy_core::expectation::StatusExpectation;
use eqproxy_core::{Metrics, shutdown};
use eqproxy_net::health::{HttpCheck, HttpCheckMethod, http_ok};
use eqproxy_net::{
    BoundProxy, Endpoint, ProxyMode, ResolvedBackend, ResolvedListener, ResolvedServer, ServerAddr,
    bind,
};

const WAIT: Duration = Duration::from_secs(5);
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Length of a Unix socket path the schema accepts (ADR-0043).
const MAX_PATH: usize = 100;

/// Removes the socket and its directory when the test ends, including on a
/// panic: a stale file would make the next `bind` fail for a real reason.
struct SocketDir(PathBuf);

impl Drop for SocketDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("app.sock"));
        let _ = std::fs::remove_dir(&self.0);
    }
}

/// A unique, short socket path under `/tmp` (macOS' `temp_dir()` is long
/// enough to approach the socket-path limit, so it is not used).
fn socket_path(tag: &str) -> (PathBuf, SocketDir) {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(format!("/tmp/eqp-unix-{}-{n}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp socket dir");
    let path = dir.join("app.sock");
    assert!(
        path.as_os_str().len() <= MAX_PATH,
        "test socket path must fit the schema limit: {path:?}"
    );
    (path, SocketDir(dir))
}

/// Unix origin that writes `banner` on accept and echoes whatever arrives.
async fn start_unix_banner(path: &Path, banner: &'static [u8]) -> (JoinHandle<()>, Arc<AtomicU64>) {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).expect("bind unix origin");
    let accepted = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&accepted);
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::Relaxed);
            tokio::spawn(async move {
                if socket.write_all(banner).await.is_err() {
                    return;
                }
                let mut buf = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 || socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    (task, accepted)
}

/// Unix origin that answers every HTTP/1.1 request on a connection and keeps
/// it open: the proxy's pool has something reusable to park.
async fn start_unix_http(path: &Path, body: &'static [u8]) -> JoinHandle<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).expect("bind unix http origin");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffered: Vec<u8> = Vec::new();
                loop {
                    // Read one request head, then answer it. The body is not
                    // part of these requests (GET without a payload).
                    let head_end = loop {
                        if let Some(pos) = buffered.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                        let mut chunk = [0u8; 1024];
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buffered.extend_from_slice(&chunk[..n]),
                        }
                    };
                    buffered.drain(..head_end);
                    let response =
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    if socket.write_all(response.as_bytes()).await.is_err()
                        || socket.write_all(body).await.is_err()
                    {
                        return;
                    }
                }
            });
        }
    })
}

/// Proxy fronting `backend`, listening on an ephemeral loopback port.
async fn start_proxy(
    backend: Arc<ResolvedBackend>,
    mode: ProxyMode,
    metrics: Arc<Metrics>,
) -> (SocketAddr, shutdown::ShutdownHandle, JoinHandle<()>) {
    let cfg = ResolvedListener::new(
        "web",
        "127.0.0.1:0".parse().expect("proxy listen addr"),
        "api",
        "127.0.0.1:1".parse::<SocketAddr>().expect("placeholder"),
        mode,
        64,
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(30),
    )
    .with_backend(backend);
    let bound: BoundProxy = bind(vec![cfg], Arc::clone(&metrics))
        .await
        .expect("bind proxy");
    let addr = bound.local_addrs()[0].1;
    let (handle, signal) = shutdown::channel();
    let serve = tokio::spawn(async move {
        bound.serve(signal, WAIT).await.expect("serve");
    });
    (addr, handle, serve)
}

fn unix_backend(path: &Path) -> Arc<ResolvedBackend> {
    Arc::new(ResolvedBackend::new(
        "api",
        vec![ResolvedServer {
            addr: ServerAddr::Unix(path.to_path_buf()),
            source: None,
            weight: 1,
            backup: false,
            priority: 0,
            maxconn: 0,
        }],
        3,
        2,
    ))
}

/// GET that keeps the client connection alive: the proxy is allowed to pool
/// the backend connection for it (a `Connection: close` request never is).
async fn http_get_keepalive(proxy: SocketAddr, path: &str) -> (u16, String) {
    let mut client = tokio::time::timeout(WAIT, TcpStream::connect(proxy))
        .await
        .expect("connect in time")
        .expect("connect to proxy");
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: example.test\r\nConnection: keep-alive\r\n\r\n");
    client.write_all(request.as_bytes()).await.expect("write");

    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let n = tokio::time::timeout(WAIT, client.read(&mut chunk))
            .await
            .expect("head in time")
            .expect("read head");
        assert!(n > 0, "EOF before a complete response head");
        raw.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let length: usize = head
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().to_owned())
        })
        .and_then(|v| v.parse().ok())
        .expect("the test origin always declares Content-Length");
    while raw.len() < head_end + length {
        let n = tokio::time::timeout(WAIT, client.read(&mut chunk))
            .await
            .expect("body in time")
            .expect("read body");
        assert!(n > 0, "EOF before the declared body");
        raw.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&raw[head_end..head_end + length]).into_owned();
    (status, body)
}

async fn http_get(proxy: SocketAddr, path: &str) -> (u16, String) {
    let mut client = tokio::time::timeout(WAIT, TcpStream::connect(proxy))
        .await
        .expect("connect in time")
        .expect("connect to proxy");
    let request = format!("GET {path} HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n");
    client.write_all(request.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    tokio::time::timeout(WAIT, client.read_to_end(&mut raw))
        .await
        .expect("response in time")
        .expect("read response");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, text)
}

#[cfg_attr(
    miri,
    ignore = "needs Unix sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_listener_relays_to_a_unix_backend() {
    let (path, _dir) = socket_path("tcp");
    let (origin, accepted) = start_unix_banner(&path, b"pong\n").await;
    let metrics = Arc::new(Metrics::default());
    let (proxy, handle, serve) =
        start_proxy(unix_backend(&path), ProxyMode::Tcp, Arc::clone(&metrics)).await;

    let mut client = tokio::time::timeout(WAIT, TcpStream::connect(proxy))
        .await
        .expect("connect in time")
        .expect("connect to proxy");
    let mut banner = [0u8; 5];
    tokio::time::timeout(WAIT, client.read_exact(&mut banner))
        .await
        .expect("banner in time")
        .expect("banner read");
    assert_eq!(&banner, b"pong\n", "the bytes come from the Unix origin");

    // The tunnel is bidirectional: what the client writes reaches the origin
    // and comes back through its echo loop.
    client.write_all(b"hello").await.expect("write");
    let mut echoed = [0u8; 5];
    tokio::time::timeout(WAIT, client.read_exact(&mut echoed))
        .await
        .expect("echo in time")
        .expect("echo read");
    assert_eq!(&echoed, b"hello");

    assert_eq!(
        accepted.load(Ordering::Relaxed),
        1,
        "one accept on the origin"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let snap = metrics.snapshot();
    assert_eq!(snap.connect_failures, 0);
    assert_eq!(snap.backend_connections_opened, 1);
    assert_eq!(snap.bytes_client_to_backend, 5);
    assert_eq!(snap.bytes_backend_to_client, 10, "banner + echo");

    drop(client);
    origin.abort();
    handle.trigger();
    tokio::time::timeout(WAIT, serve)
        .await
        .expect("serve finishes")
        .expect("serve task without panic");
}

#[cfg_attr(
    miri,
    ignore = "needs Unix sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_listener_proxies_to_a_unix_backend_and_pools_the_connection() {
    let (path, _dir) = socket_path("http");
    let origin = start_unix_http(&path, b"unix\n").await;
    let metrics = Arc::new(Metrics::default());
    let (proxy, handle, serve) =
        start_proxy(unix_backend(&path), ProxyMode::Http, Arc::clone(&metrics)).await;

    let (status, body) = http_get_keepalive(proxy, "/one").await;
    assert_eq!(
        status, 200,
        "the Unix origin answered through the proxy: {body:?}"
    );
    assert_eq!(body, "unix\n", "body forwarded");

    // A second exchange on a fresh client connection must reuse the parked
    // backend connection: the pool keys on the Unix endpoint (ADR-0043), not
    // on a TCP address it does not have.
    let (status, body) = http_get_keepalive(proxy, "/two").await;
    assert_eq!(status, 200);
    assert_eq!(body, "unix\n");

    tokio::time::sleep(Duration::from_millis(100)).await;
    let snap = metrics.snapshot();
    assert_eq!(snap.connect_failures, 0);
    assert_eq!(
        snap.backend_connections_opened, 1,
        "one dial to the socket for both exchanges"
    );
    assert_eq!(
        snap.backend_connections_reused, 1,
        "the second exchange came from the keep-alive pool"
    );

    origin.abort();
    handle.trigger();
    tokio::time::timeout(WAIT, serve)
        .await
        .expect("serve finishes")
        .expect("serve task without panic");
}

#[cfg_attr(
    miri,
    ignore = "needs Unix sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_unix_socket_fails_explicitly_and_fast() {
    let (path, _dir) = socket_path("missing");
    // Nothing binds this path: the connect must fail, not hang.
    let metrics = Arc::new(Metrics::default());
    let (proxy, handle, serve) =
        start_proxy(unix_backend(&path), ProxyMode::Http, Arc::clone(&metrics)).await;

    let started = std::time::Instant::now();
    let (status, _) = http_get(proxy, "/").await;
    assert_eq!(status, 502, "an unreachable backend is a 502");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "ENOENT must fail at once, took {:?}",
        started.elapsed()
    );

    let snap = metrics.snapshot();
    assert_eq!(snap.connect_failures, 1, "the failure is counted");
    assert_eq!(
        snap.backend_connections_opened, 0,
        "nothing was ever opened"
    );

    handle.trigger();
    tokio::time::timeout(WAIT, serve)
        .await
        .expect("serve finishes")
        .expect("serve task without panic");
}

/// TCP origin that reports the peer address it sees, so the test can prove
/// which local address the outbound socket used.
async fn start_peer_reporter() -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind origin");
    let addr = listener.local_addr().expect("origin addr");
    let task = tokio::spawn(async move {
        while let Ok((mut socket, peer)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = format!("{}\n", peer.ip());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (addr, task)
}

/// A loopback address that is not the kernel's default source for a
/// `127.0.0.1` target, when the platform has one. Linux owns all of
/// `127.0.0.0/8`; macOS reserves only `127.0.0.1`, so there the test can
/// prove the bind is accepted but not that it changed the peer.
fn distinct_loopback() -> Option<std::net::IpAddr> {
    let probe = std::net::TcpListener::bind("127.0.0.2:0").ok()?;
    drop(probe);
    Some("127.0.0.2".parse().expect("loopback alias"))
}

fn tcp_backend(addr: SocketAddr, source: Option<std::net::IpAddr>) -> Arc<ResolvedBackend> {
    Arc::new(ResolvedBackend::new(
        "api",
        vec![ResolvedServer {
            addr: ServerAddr::Ip(addr),
            source,
            weight: 1,
            backup: false,
            priority: 0,
            maxconn: 0,
        }],
        1,
        1,
    ))
}

#[cfg_attr(
    miri,
    ignore = "needs TCP sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_binds_the_outbound_socket_to_the_chosen_address() {
    let (origin, origin_task) = start_peer_reporter().await;
    // Where a second loopback address exists, it is the discriminating choice:
    // an unbound socket would appear as 127.0.0.1, so seeing 127.0.0.2 at the
    // origin is only possible if the bind took effect.
    let source: std::net::IpAddr =
        distinct_loopback().unwrap_or_else(|| "127.0.0.1".parse().expect("loopback"));
    // A bind that fails would make the exchange 502: success proves the
    // outbound socket was bound to the configured local address.
    let metrics = Arc::new(Metrics::default());
    let (proxy, handle, serve) = start_proxy(
        tcp_backend(origin, Some(source)),
        ProxyMode::Http,
        Arc::clone(&metrics),
    )
    .await;

    let (status, text) = http_get(proxy, "/").await;
    assert_eq!(status, 200, "bound source connects: {text}");
    assert!(
        text.ends_with(&format!("{source}\n")),
        "origin sees the configured source {source}: {text:?}"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(metrics.snapshot().connect_failures, 0);

    origin_task.abort();
    handle.trigger();
    tokio::time::timeout(WAIT, serve)
        .await
        .expect("serve finishes")
        .expect("serve task without panic");
}

#[cfg_attr(
    miri,
    ignore = "needs TCP sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unusable_source_is_an_explicit_connect_failure() {
    let (origin, origin_task) = start_peer_reporter().await;
    // TEST-NET-1 is never a local address: `bind` fails with EADDRNOTAVAIL.
    let unusable: std::net::IpAddr = "192.0.2.1".parse().expect("test-net");
    let metrics = Arc::new(Metrics::default());
    let (proxy, handle, serve) = start_proxy(
        tcp_backend(origin, Some(unusable)),
        ProxyMode::Http,
        Arc::clone(&metrics),
    )
    .await;

    let started = std::time::Instant::now();
    let (status, _) = http_get(proxy, "/").await;
    assert_eq!(status, 502, "a bind failure is a connect failure");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the bind fails at once, took {:?}",
        started.elapsed()
    );
    assert_eq!(
        metrics.snapshot().connect_failures,
        1,
        "never a silent fallback to the kernel-chosen address"
    );

    origin_task.abort();
    handle.trigger();
    tokio::time::timeout(WAIT, serve)
        .await
        .expect("serve finishes")
        .expect("serve task without panic");
}

#[cfg_attr(
    miri,
    ignore = "needs Unix sockets: miri does not support networking (rust-lang/miri#3449)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_health_checker_speaks_http_to_a_unix_endpoint() {
    let (path, _dir) = socket_path("health");
    let origin = start_unix_http(&path, b"ok\n").await;
    let endpoint = Endpoint::Unix(Arc::new(path.clone()));
    let check = HttpCheck {
        method: HttpCheckMethod::Get,
        path: "/healthz".to_owned(),
        host: Some("localhost".to_owned()),
        expect_status: StatusExpectation::parse("200-299").expect("status"),
        expect_rstring: None,
    };
    assert!(
        http_ok(&endpoint, &check, Duration::from_secs(2)).await,
        "a 200 over the Unix socket is UP"
    );

    origin.abort();
    // The socket file outlives its listener: connecting to it is refused
    // (ECONNREFUSED), which is exactly the DOWN signal the checker reports.
    assert!(
        !http_ok(&endpoint, &check, Duration::from_millis(500)).await,
        "a dead socket is DOWN"
    );
}
