//! E2E of custom error pages (B-H15 · B-E18 · B-N14, ADR-0045) with **real
//! sockets**: a locally generated failure uses the configured page, an
//! origin response is replaced only when the page declares `intercept`, and
//! both are counted.
//!
//! The pages here are resolved the way the CLI resolves them (body bytes,
//! status, content type already decided), so the test covers the dataplane
//! contract. The file/`format = "raw"` half is covered by the config crate and
//! by the release-binary journey in `docs/evidence/v091-error-pages.md`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use eqproxy_core::{Metrics, shutdown};
use eqproxy_net::{BoundProxy, ErrorPage, ErrorPages, ProxyMode, ResolvedListener, bind};

mod common;

const WAIT: Duration = Duration::from_secs(5);

/// Origin that answers every request with the same raw response and counts
/// the connections it accepted.
async fn start_origin(response: &'static [u8]) -> (SocketAddr, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind origin");
    let addr = listener.local_addr().expect("origin addr");
    let connections = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&connections);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::Relaxed);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    if socket.write_all(response).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (addr, connections)
}

fn page(codes: &[u16], status: Option<u16>, body: &str, intercept: bool) -> ErrorPage {
    ErrorPage {
        codes: codes.to_vec(),
        status,
        content_type: "text/html; charset=utf-8".to_owned(),
        body: Arc::from(body.as_bytes()),
        location: None,
        intercept,
    }
}

fn redirect(codes: &[u16], status: u16, location: &str, intercept: bool) -> ErrorPage {
    ErrorPage {
        codes: codes.to_vec(),
        status: Some(status),
        content_type: "text/html; charset=utf-8".to_owned(),
        body: Arc::from(b"".as_slice()),
        location: Some(location.to_owned()),
        intercept,
    }
}

async fn start_proxy(
    server: SocketAddr,
    pages: ErrorPages,
    metrics: Arc<Metrics>,
) -> (SocketAddr, shutdown::ShutdownHandle) {
    let cfg = ResolvedListener::new(
        "web",
        // Port 0: the kernel assigns it and the test reads it back, so no
        // neighbour can hold the address (tests/common/mod.rs).
        "127.0.0.1:0".parse().expect("listen addr"),
        "api",
        server,
        ProxyMode::Http,
        64,
        Duration::from_millis(200),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .with_error_pages(pages);
    let bound: BoundProxy = bind(vec![cfg], Arc::clone(&metrics)).await.expect("bind");
    let addr = bound.local_addrs()[0].1;
    let (handle, signal) = shutdown::channel();
    tokio::spawn(async move {
        bound
            .serve(signal, Duration::from_secs(2))
            .await
            .expect("serve");
    });
    (addr, handle)
}

/// Sends one request with `Connection: close` and reads until the proxy
/// closes, which is how every locally generated error ends.
async fn request(addr: SocketAddr, method: &str) -> String {
    let mut client = tokio::time::timeout(WAIT, TcpStream::connect(addr))
        .await
        .expect("connect in time")
        .expect("connect");
    let raw = format!("{method} / HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n");
    client.write_all(raw.as_bytes()).await.expect("write");
    let mut acc = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = tokio::time::timeout(WAIT, client.read(&mut buf))
            .await
            .expect("response in time")
            .expect("read");
        if n == 0 {
            break;
        }
        acc.extend_from_slice(&buf[..n]);
    }
    String::from_utf8_lossy(&acc).into_owned()
}

fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default()
}

fn header_value<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// A locally generated page must carry exactly one valid `Date`.
fn assert_one_http_date(response: &str) {
    let lines = response
        .lines()
        .skip(1)
        .filter(|line| line.to_ascii_lowercase().starts_with("date:"))
        .count();
    assert_eq!(lines, 1, "exactly one Date line: {response}");
    common::assert_http_date(header_value(response, "date").expect("Date header"));
}

#[tokio::test]
async fn a_local_connect_failure_serves_the_configured_page() {
    let (dead, _guard) = common::dead_addr();
    let metrics = Arc::new(Metrics::default());
    let pages = ErrorPages::from_pages(vec![page(&[502], None, "<h1>sin origen</h1>", false)]);
    let (addr, _shutdown) = start_proxy(dead, pages, Arc::clone(&metrics)).await;

    let response = request(addr, "GET").await;
    assert!(
        response.starts_with("HTTP/1.1 502"),
        "the fired status is kept: {response}"
    );
    assert!(
        header_value(&response, "content-type") == Some("text/html; charset=utf-8"),
        "the page's content type is served: {response}"
    );
    assert_eq!(body_of(&response), "<h1>sin origen</h1>");
    assert_one_http_date(&response);
    let expected_len = "<h1>sin origen</h1>".len().to_string();
    assert_eq!(
        header_value(&response, "content-length"),
        Some(expected_len.as_str()),
        "the declared length matches the page: {response}"
    );
    common::wait_for_counter("error_pages_served", 1, || {
        metrics.snapshot().error_pages_served
    })
    .await;
}

#[tokio::test]
async fn a_page_can_override_the_status_and_head_omits_the_body() {
    let (dead, _guard) = common::dead_addr();
    let metrics = Arc::new(Metrics::default());
    let pages = ErrorPages::from_pages(vec![page(
        &[502],
        Some(503),
        "mantenimiento programado\n",
        false,
    )]);
    let (addr, _shutdown) = start_proxy(dead, pages, Arc::clone(&metrics)).await;

    let response = request(addr, "GET").await;
    assert!(
        response.starts_with("HTTP/1.1 503"),
        "`status` replaces the fired 502: {response}"
    );
    assert_eq!(body_of(&response), "mantenimiento programado\n");

    let head = request(addr, "HEAD").await;
    assert!(
        head.starts_with("HTTP/1.1 503"),
        "HEAD keeps the status: {head}"
    );
    let expected_len = "mantenimiento programado\n".len().to_string();
    assert_eq!(
        header_value(&head, "content-length"),
        Some(expected_len.as_str()),
        "HEAD declares the GET length: {head}"
    );
    assert!(
        body_of(&head).is_empty(),
        "HEAD must not carry the page: {head}"
    );
    common::wait_for_counter("error_pages_served", 2, || {
        metrics.snapshot().error_pages_served
    })
    .await;
}

#[tokio::test]
async fn intercept_replaces_an_origin_error_and_drops_its_connection() {
    let (origin, connections) = start_origin(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 6\r\nConnection: keep-alive\r\n\r\norigen",
    )
    .await;
    let metrics = Arc::new(Metrics::default());
    let pages = ErrorPages::from_pages(vec![page(&[503], None, "sin servicio\n", true)]);
    let (addr, _shutdown) = start_proxy(origin, pages, Arc::clone(&metrics)).await;

    for _ in 0..2 {
        let response = request(addr, "GET").await;
        assert!(
            response.starts_with("HTTP/1.1 503"),
            "the origin status is kept: {response}"
        );
        assert_eq!(body_of(&response), "sin servicio\n");
        assert_one_http_date(&response);
    }
    assert_eq!(
        connections.load(Ordering::Relaxed),
        2,
        "an intercepted response was never read; its connection must not be pooled"
    );
    common::wait_for_counter("error_pages_served", 2, || {
        metrics.snapshot().error_pages_served
    })
    .await;
}

#[tokio::test]
async fn without_intercept_the_origin_error_passes_through() {
    let (origin, _connections) = start_origin(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 6\r\nConnection: close\r\n\r\norigen",
    )
    .await;
    let metrics = Arc::new(Metrics::default());
    // The page covers 502 only: an origin 503 is not its business.
    let pages = ErrorPages::from_pages(vec![page(&[502], None, "sin origen\n", false)]);
    let (addr, _shutdown) = start_proxy(origin, pages, Arc::clone(&metrics)).await;

    let response = request(addr, "GET").await;
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert_eq!(
        body_of(&response),
        "origen",
        "the origin body must reach the client untouched: {response}"
    );
    assert_eq!(
        metrics.snapshot().error_pages_served,
        0,
        "a page that did not fire is not counted"
    );
}

#[tokio::test]
async fn errorloc_answers_a_local_failure_with_a_bodyless_redirect() {
    let (dead, _guard) = common::dead_addr();
    let metrics = Arc::new(Metrics::default());
    let pages = ErrorPages::from_pages(vec![redirect(
        &[502],
        302,
        "https://status.example.test/maintenance",
        false,
    )]);
    let (addr, _shutdown) = start_proxy(dead, pages, Arc::clone(&metrics)).await;

    let response = request(addr, "GET").await;
    assert!(
        response.starts_with("HTTP/1.1 302 found"),
        "errorloc answers 302: {response}"
    );
    assert_eq!(
        header_value(&response, "location"),
        Some("https://status.example.test/maintenance"),
        "the configured target is the Location header: {response}"
    );
    assert_eq!(header_value(&response, "content-length"), Some("0"));
    assert_one_http_date(&response);
    assert!(
        body_of(&response).is_empty(),
        "a redirect carries no body: {response}"
    );
    common::wait_for_counter("error_pages_served", 1, || {
        metrics.snapshot().error_pages_served
    })
    .await;
}

#[tokio::test]
async fn errorloc_intercepts_an_origin_error_and_keeps_its_connection_out_of_the_pool() {
    let (origin, connections) = start_origin(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 6\r\nConnection: keep-alive\r\n\r\norigen",
    )
    .await;
    let metrics = Arc::new(Metrics::default());
    let pages = ErrorPages::from_pages(vec![redirect(&[503], 303, "/status", true)]);
    let (addr, _shutdown) = start_proxy(origin, pages, Arc::clone(&metrics)).await;

    for _ in 0..2 {
        let response = request(addr, "GET").await;
        assert!(
            response.starts_with("HTTP/1.1 303 see other"),
            "the page's 303 replaces the origin 503: {response}"
        );
        assert_eq!(header_value(&response, "location"), Some("/status"));
        assert!(body_of(&response).is_empty());
    }
    assert_eq!(
        connections.load(Ordering::Relaxed),
        2,
        "the intercepted origin connection is dropped, never pooled"
    );
    common::wait_for_counter("error_pages_served", 2, || {
        metrics.snapshot().error_pages_served
    })
    .await;
}

#[tokio::test]
async fn no_page_keeps_the_builtin_minimal_error() {
    let (dead, _guard) = common::dead_addr();
    let metrics = Arc::new(Metrics::default());
    let (addr, _shutdown) = start_proxy(dead, ErrorPages::default(), Arc::clone(&metrics)).await;

    let response = request(addr, "GET").await;
    assert!(response.starts_with("HTTP/1.1 502"), "{response}");
    assert_eq!(
        header_value(&response, "content-length"),
        Some("0"),
        "the built-in error carries no body: {response}"
    );
    assert!(body_of(&response).is_empty());
    assert_one_http_date(&response);
    assert_eq!(metrics.snapshot().error_pages_served, 0);
}
