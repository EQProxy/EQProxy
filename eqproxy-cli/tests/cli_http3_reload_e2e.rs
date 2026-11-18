//! Minimal HTTP/3 hot-reload E2E through the **real** `eqproxy` binary and the
//! admin API: a prior backend swap commits a new generation, then a listener
//! PUT renews alpha and adds gamma. The live QUIC endpoint must present the
//! renewed/gamma leaves, TCP must still negotiate h2 with the same material,
//! and a real h2 request must reach the origin selected by the prior swap.
//!
//! The h3 client is a real `quinn` endpoint offering ALPN `h3`; assertions are
//! on the leaf the handshake actually presented and on the negotiated
//! protocol, never on configuration state or a reported fingerprint.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eqproxy_net::tls;
use rustls::client::danger::{ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

const FIXTURE_CERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/cert.pem"));
const FIXTURE_KEY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/key.pem"));
const FIXTURE_ALPHA_CERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/alpha.pem"));
const FIXTURE_ALPHA_KEY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/alpha.key"));
const FIXTURE_ALPHA2_CERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/alpha2.pem"));
const FIXTURE_ALPHA2_KEY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/alpha2.key"));
const FIXTURE_BETA_CERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/beta.pem"));
const FIXTURE_BETA_KEY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/beta.key"));
const FIXTURE_GAMMA_CERT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/gamma.pem"));
const FIXTURE_GAMMA_KEY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/gamma.key"));

const READ_TOKEN: &str = "3f9a1c0d7b2e4f6a8c5d1e0b9a7c3f2d";
const WRITE_TOKEN: &str = "9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e";
const WAIT: Duration = Duration::from_secs(10);
const CURL_CERTS_MARKER: &str = "\n--eqproxy-certs--\n";
static CURL_CERTS_SUPPORT: OnceLock<()> = OnceLock::new();
const ALPHA: (&str, &str, &str) = ("alpha.pem", "alpha.key", "alpha.example.test");
const ALPHA2: (&str, &str, &str) = ("alpha2.pem", "alpha2.key", "alpha.example.test");
const BETA: (&str, &str, &str) = ("beta.pem", "beta.key", "beta.example.test");
const GAMMA: (&str, &str, &str) = ("gamma.pem", "gamma.key", "gamma.example.test");

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_eqproxy"))
}

fn leaf(pem: &[u8]) -> Vec<u8> {
    tls::parse_certs(pem).expect("fixture certificate parses")[0]
        .as_ref()
        .to_vec()
}

/// The one TOML shape shared by the startup file and the listener PUT: the
/// default certificate followed by the current SNI entries.
fn sni_blocks(dir: &Path, certs: &[(&str, &str, &str)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (cert, key, name) in certs {
        let _ = writeln!(
            out,
            "[[listener.web.tls.certs]]\ncert = \"{}\"\nkey = \"{}\"\nnames = [\"{name}\"]",
            dir.join(cert).display(),
            dir.join(key).display(),
        );
    }
    out
}

/// The listener section shared by the startup file and the listener PUT. The
/// PUT replaces only this section, so it carries no root, backend or admin
/// keys.
fn listener_body(dir: &Path, certs: &[(&str, &str, &str)]) -> String {
    format!(
        "[listener.web]\n\
         listen = \"127.0.0.1:0\"\n\
         mode = \"http\"\n\
         backend = \"api\"\n\
         quic = true\n\
         [listener.web.tls]\n\
         cert = \"{}\"\n\
         key = \"{}\"\n\
         alpn = [\"h2\", \"http/1.1\"]\n\
         {}",
        dir.join("server.pem").display(),
        dir.join("server.key").display(),
        sni_blocks(dir, certs),
    )
}

fn startup_config(dir: &Path, certs: &[(&str, &str, &str)], backend: &str) -> String {
    format!(
        "schema_version = 1\n\
         shutdown_grace = \"1s\"\n\
         {}\
         [backend.api]\n\
         server = \"{backend}\"\n\
         [admin]\n\
         listen = \"127.0.0.1:0\"\n\
         token_file = \"{}\"\n\
         write_token_file = \"{}\"\n",
        listener_body(dir, certs),
        dir.join("read.token").display(),
        dir.join("write.token").display(),
    )
}

fn write_fixture(dir: &Path, stem: &str, cert: &[u8], key: &[u8]) {
    std::fs::write(dir.join(format!("{stem}.pem")), cert).expect("write fixture certificate");
    std::fs::write(dir.join(format!("{stem}.key")), key).expect("write fixture key");
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eqproxy-cli-h3-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

/// Writes every fixture a test may select from. Tests still choose their own
/// SNI set and backend; only the throwaway files and tokens are shared.
fn fixture_dir(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write_fixture(&dir, "server", FIXTURE_CERT, FIXTURE_KEY);
    write_fixture(&dir, "alpha", FIXTURE_ALPHA_CERT, FIXTURE_ALPHA_KEY);
    write_fixture(&dir, "alpha2", FIXTURE_ALPHA2_CERT, FIXTURE_ALPHA2_KEY);
    write_fixture(&dir, "beta", FIXTURE_BETA_CERT, FIXTURE_BETA_KEY);
    write_fixture(&dir, "gamma", FIXTURE_GAMMA_CERT, FIXTURE_GAMMA_KEY);
    std::fs::write(dir.join("read.token"), format!("{READ_TOKEN}\n")).expect("write read token");
    std::fs::write(dir.join("write.token"), format!("{WRITE_TOKEN}\n")).expect("write write token");
    dir
}

fn start_proxy(tag: &str, certs: &[(&str, &str, &str)], backend: &str) -> Proxy {
    let dir = fixture_dir(tag);
    let config = startup_config(&dir, certs, backend);
    Proxy::start(dir, &config)
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn stderr_address(stderr: &mut BufReader<std::process::ChildStderr>, needle: &str) -> String {
    let deadline = Instant::now() + WAIT;
    let mut lines = Vec::new();
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        let read = stderr.read_line(&mut line).expect("read stderr");
        assert!(
            read > 0,
            "eqproxy closed stderr before {needle:?}; it said: {lines:?}"
        );
        lines.push(line.trim_end().to_owned());
        if let Some(rest) = line.split(needle).nth(1) {
            return rest
                .split_whitespace()
                .next()
                .expect("address after announcement")
                .to_owned();
        }
    }
    panic!("timeout waiting for {needle:?}; log: {lines:?}");
}

/// Accepts any chain: the assertion is on the leaf the handshake carried,
/// which is exactly the material a real client receives.
#[derive(Debug)]
struct AcceptAny {
    provider: rustls::crypto::CryptoProvider,
}

impl AcceptAny {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            provider: rustls::crypto::aws_lc_rs::default_provider(),
        })
    }
}

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Performs a real QUIC handshake offering `h3` and returns the leaf the
/// server presented. `handshake.protocol` must be `h3`, so a handshake that
/// negotiates another protocol (or none) fails before any leaf is trusted.
async fn h3_handshake(addr: SocketAddr, sni: &str) -> Vec<u8> {
    let mut config =
        rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .dangerous()
            .with_custom_certificate_verifier(AcceptAny::new())
            .with_no_client_auth();
    config.alpn_protocols = vec![b"h3".to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(config)
        .expect("QUIC client TLS 1.3 config");
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().expect("client addr"))
        .expect("client QUIC endpoint");
    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
    let connection = tokio::time::timeout(WAIT, async {
        endpoint
            .connect(addr, sni)
            .expect("QUIC connect")
            .await
            .expect("QUIC handshake")
    })
    .await
    .expect("QUIC handshake in time");
    let handshake = connection
        .handshake_data()
        .expect("handshake data")
        .downcast::<quinn::crypto::rustls::HandshakeData>()
        .expect("rustls handshake data");
    assert_eq!(
        handshake.protocol.as_deref(),
        Some(b"h3".as_slice()),
        "the QUIC handshake must negotiate h3"
    );
    let certs = connection
        .peer_identity()
        .expect("peer identity")
        .downcast::<Vec<CertificateDer<'static>>>()
        .expect("rustls peer certificates");
    let presented = certs
        .first()
        .expect("the handshake must present a certificate")
        .as_ref()
        .to_vec();
    endpoint.close(0u32.into(), b"e2e done");
    endpoint.wait_idle().await;
    presented
}

/// A real HTTP/1.1 origin. The body identifies the origin generation, so the
/// final h2 response proves which backend the prior swap selected.
fn start_ok_origin(body: &'static str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind h1 origin");
    let addr = listener.local_addr().expect("h1 origin addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nX-Origin: h1\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    addr
}

/// Real TCP + TLS handshake with SNI and ALPN h2. Returns the leaf the TCP
/// listener presented, so the assertion is on material a client received.
fn tcp_tls_leaf(addr: SocketAddr, sni: &str) -> Vec<u8> {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(AcceptAny::new())
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    let server_name = rustls_pki_types::ServerName::try_from(sni.to_owned()).expect("SNI name");
    let mut connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .expect("TLS client connection");
    let mut socket = TcpStream::connect(addr).expect("connect TCP listener");
    socket
        .set_read_timeout(Some(WAIT))
        .expect("TCP TLS read timeout");
    socket
        .set_write_timeout(Some(WAIT))
        .expect("TCP TLS write timeout");
    while connection.is_handshaking() {
        connection
            .complete_io(&mut socket)
            .expect("TCP TLS handshake");
    }
    assert_eq!(
        connection.alpn_protocol(),
        Some(b"h2".as_slice()),
        "the TCP listener must still negotiate h2"
    );
    connection
        .peer_certificates()
        .expect("TCP handshake must present a certificate")[0]
        .as_ref()
        .to_vec()
}

fn curl_http2(addr: SocketAddr) -> String {
    let url = format!("https://{addr}/");
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "15",
            "-k",
            "--noproxy",
            "*",
            "--http2",
            "-D",
            "-",
            &url,
        ])
        .output()
        .expect("run curl (needed for the TCP/h2 e2e)");
    assert!(
        output.status.success(),
        "curl {url} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        response.starts_with("HTTP/2 200"),
        "the TCP listener must still serve a real h2 request: {response}"
    );
    response
}

/// Fails once, with an actionable message, when the curl on PATH is older than
/// the first release that supports the `%{certs}` write-out (7.88). Without
/// this check an old curl would fail with an opaque "unknown write-out" error.
fn assert_curl_certs_support() {
    CURL_CERTS_SUPPORT.get_or_init(|| {
        let output = Command::new("curl")
            .arg("--version")
            .output()
            .expect("run curl --version (needed for the TCP/h2 e2e)");
        assert!(
            output.status.success(),
            "curl --version failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let banner = String::from_utf8_lossy(&output.stdout);
        let line = banner.lines().next().unwrap_or_default();
        let version = line.split_whitespace().nth(1).unwrap_or_default();
        let mut parts = version.split('.');
        let major = parts.next().and_then(|part| part.parse::<u32>().ok());
        let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
        let (Some(major), Some(minor)) = (major, minor) else {
            panic!("curl --version returned an unrecognized banner: {line:?}");
        };
        assert!(
            (major, minor) >= (7, 88),
            "curl >= 7.88 is required for the `%{{certs}}` write-out used by this \
             E2E, but this host has curl {version}. Upgrade curl or put a newer \
             curl first in PATH."
        );
    });
}

/// Real h2 request addressed to `sni`, resolved to the loopback listener. The
/// URL carries the name, so curl sends it as SNI and Host; `%{certs}` returns
/// the chain the listener actually presented, keeping this assertion on the
/// same material as the h3/TCP checks. `-k` is needed for the expected
/// fallback: after removal the default leaf is not valid for beta.example.test.
fn curl_http2_sni(addr: SocketAddr, sni: &str) -> (String, Vec<u8>) {
    assert_curl_certs_support();
    let port = addr.port();
    let url = format!("https://{sni}:{port}/");
    let resolve = format!("{sni}:{port}:{}", addr.ip());
    let write_out = format!("{CURL_CERTS_MARKER}%{{certs}}");
    let output = Command::new("curl")
        .args(["-sS", "--max-time", "15", "-k", "--noproxy", "*"])
        .args(["--http2", "-D", "-"])
        .arg("--resolve")
        .arg(&resolve)
        .arg("--write-out")
        .arg(&write_out)
        .arg(&url)
        .output()
        .expect("run curl with SNI (needed for the TCP/h2 e2e)");
    assert!(
        output.status.success(),
        "curl {url} with SNI {sni} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let (response, certs) = stdout
        .rsplit_once(CURL_CERTS_MARKER)
        .unwrap_or_else(|| panic!("curl {url} did not report the peer chain"));
    assert!(
        response.starts_with("HTTP/2 200"),
        "the TCP listener must still serve a real h2 request for {sni}: {response}"
    );
    let certs = tls::parse_certs(certs.as_bytes()).expect("parse curl peer chain");
    assert!(!certs.is_empty(), "curl {url} saw an empty peer chain");
    (response.to_owned(), certs[0].as_ref().to_vec())
}

/// Asserts one real h2 exchange for beta.example.test: the URL/SNI names beta,
/// the origin body identifies the active generation, and `%{certs}` identifies
/// the leaf the h2 TLS session actually presented.
fn assert_h2_beta(proxy: &Proxy, expected_body: &str, expected_leaf: &[u8], phase: &str) {
    let (response, presented) = proxy.h2_sni("beta.example.test");
    assert!(
        response.contains(expected_body),
        "{phase}: the h2 request for beta must reach the expected origin; got {response}"
    );
    assert_eq!(
        presented,
        leaf(expected_leaf),
        "{phase}: the h2 request for beta must present the expected leaf; got {presented:?}"
    );
}

/// Waits until the admin listener can complete a real status request. The
/// listener address is printed before its accept loop is necessarily ready;
/// the first write is not a reliable readiness probe.
fn wait_admin(addr: &str) {
    let deadline = Instant::now() + WAIT;
    let request = format!(
        "GET /api/v1/status HTTP/1.1\r\n\
         host: admin\r\n\
         authorization: Bearer {READ_TOKEN}\r\n\
         connection: close\r\n\
         \r\n"
    );
    loop {
        if let Ok(mut client) = TcpStream::connect(addr) {
            let _ = client.write_all(request.as_bytes());
            let _ = client.set_read_timeout(Some(Duration::from_millis(200)));
            let mut raw = Vec::new();
            let _ = client.read_to_end(&mut raw);
            let text = String::from_utf8_lossy(&raw);
            if text.starts_with("HTTP/1.1 200") {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "admin listener never answered a status request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn admin_exchange(addr: &str, request: &str) -> (u16, String) {
    let mut client = TcpStream::connect(addr).expect("connect admin");
    client
        .write_all(request.as_bytes())
        .expect("write admin request");
    client
        .set_read_timeout(Some(WAIT))
        .expect("admin read timeout");
    let mut raw = Vec::new();
    client.read_to_end(&mut raw).expect("read admin answer");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let head_end = text
        .find("\r\n\r\n")
        .unwrap_or_else(|| panic!("no admin head: {text:?}"));
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no admin status: {text:?}"));
    (status, text[head_end + 4..].to_owned())
}

fn admin_put(admin: &str, path: &str, if_match: &str, body: &str) -> (u16, String) {
    let request = format!(
        "PUT {path} HTTP/1.1\r\n\
         host: admin\r\n\
         authorization: Bearer {WRITE_TOKEN}\r\n\
         content-type: application/toml\r\n\
         if-match: \"{if_match}\"\r\n\
         content-length: {}\r\n\
         \r\n{body}",
        body.len()
    );
    admin_exchange(admin, &request)
}

fn put_listener(admin: &str, if_match: &str, body: &str) -> (u16, String) {
    admin_put(admin, "/api/v1/listeners/web", if_match, body)
}

fn put_backend_at(admin: &str, if_match: &str, server: &str) -> (u16, String) {
    let body = format!("server = \"{server}\"\n");
    admin_put(admin, "/api/v1/backends/api", if_match, &body)
}

fn put_listener_certs(
    proxy: &Proxy,
    if_match: &str,
    certs: &[(&str, &str, &str)],
) -> (u16, String) {
    put_listener(&proxy.admin, if_match, &listener_body(&proxy.dir, certs))
}

/// Commits one listener generation and checks the shared admin contract in one
/// place: HTTP 200, the expected revision, and the listener socket retained.
fn commit_listener(
    proxy: &Proxy,
    if_match: &str,
    certs: &[(&str, &str, &str)],
    revision: &str,
    phase: &str,
) {
    let (status, body) = put_listener_certs(proxy, if_match, certs);
    assert_eq!(status, 200, "{phase} must commit: {body}");
    let applied: serde_json::Value = serde_json::from_str(&body).expect("listener PUT json");
    assert_eq!(applied["revision"], revision, "{phase} response: {body}");
    assert_eq!(
        applied["listeners"][0]["listen"],
        proxy.addr.to_string(),
        "{phase} must keep the listener socket: {body}"
    );
}

fn commit_backend(proxy: &Proxy, if_match: &str, server: &str, revision: &str, phase: &str) {
    let (status, body) = put_backend_at(&proxy.admin, if_match, server);
    assert_eq!(status, 200, "{phase} must commit: {body}");
    let applied: serde_json::Value = serde_json::from_str(&body).expect("backend PUT json");
    assert_eq!(applied["revision"], revision, "{phase} response: {body}");
    assert_eq!(
        applied["listeners"][0]["listen"],
        proxy.addr.to_string(),
        "{phase} must keep the listener socket: {body}"
    );
}

struct Proxy {
    child: ChildGuard,
    dir: PathBuf,
    addr: SocketAddr,
    admin: String,
    runtime: tokio::runtime::Runtime,
}

impl Proxy {
    fn start(dir: PathBuf, config: &str) -> Self {
        let config_path = dir.join("eqproxy.toml");
        std::fs::write(&config_path, config).expect("write config");
        let mut child = ChildGuard(
            bin()
                .args(["run", "--config"])
                .arg(&config_path)
                .args(["--metrics-interval", "1h"])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn eqproxy run"),
        );
        let mut stderr = BufReader::new(child.0.stderr.take().expect("stderr piped"));
        let addr = stderr_address(&mut stderr, "listener web listening on ")
            .parse()
            .expect("proxy address");
        let admin = stderr_address(&mut stderr, "admin listening on ");
        wait_admin(&admin);
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        Self {
            child,
            dir,
            addr,
            admin,
            runtime,
        }
    }

    fn h3(&self, sni: &str) -> Vec<u8> {
        self.runtime.block_on(h3_handshake(self.addr, sni))
    }

    fn tcp(&self, sni: &str) -> Vec<u8> {
        tcp_tls_leaf(self.addr, sni)
    }

    fn h2(&self) -> String {
        curl_http2(self.addr)
    }

    fn h2_sni(&self, sni: &str) -> (String, Vec<u8>) {
        curl_http2_sni(self.addr, sni)
    }

    fn shutdown(mut self) {
        let pid = self.child.0.id();
        let killed = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status()
            .expect("run kill");
        assert!(killed.success(), "SIGINT must be accepted");
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.0.try_wait().expect("try_wait") {
                assert_eq!(status.code(), Some(0), "clean shutdown");
                break;
            }
            assert!(Instant::now() < deadline, "eqproxy did not stop on SIGINT");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.0.kill();
        let _ = self.child.0.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg_attr(
    miri,
    ignore = "needs processes and UDP/TCP sockets: miri does not support them (rust-lang/miri#3449)"
)]
#[test]
fn listener_put_renews_h3_and_tcp_after_a_prior_swap() {
    let origin = start_ok_origin("tcp-h2-before");
    let next_origin = start_ok_origin("tcp-h2-after");
    let proxy = start_proxy("listener-put", &[ALPHA, BETA], &origin.to_string());

    // (1) Initial material is served on h3.
    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the initial alpha certificate must reach h3"
    );

    // (2) A prior config swap commits a new generation. This is the condition
    //     the defect needed; the next listener PUT must still follow it.
    commit_backend(
        &proxy,
        "v1",
        &next_origin.to_string(),
        "v2",
        "the prior backend PUT",
    );
    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the prior swap must keep alpha on the QUIC resolver"
    );

    // (3) The listener PUT renews alpha and adds gamma after that swap. Both
    //     the new material and the new name must reach the live endpoint.
    commit_listener(
        &proxy,
        "v2",
        &[ALPHA2, BETA, GAMMA],
        "v3",
        "the listener PUT",
    );

    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA2_CERT),
        "the renewed alpha certificate must reach h3"
    );
    assert_eq!(
        proxy.h3("gamma.example.test"),
        leaf(FIXTURE_GAMMA_CERT),
        "the gamma name added by the listener PUT must reach h3"
    );
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_BETA_CERT),
        "the listener PUT must preserve beta"
    );
    assert_eq!(
        proxy.h3("default.invalid"),
        leaf(FIXTURE_CERT),
        "the listener PUT must preserve the default certificate"
    );

    // (4) TCP keeps the same renewed material and negotiates h2; the real h2
    //     request must reach the origin selected by the prior swap.
    assert_eq!(
        proxy.tcp("alpha.example.test"),
        leaf(FIXTURE_ALPHA2_CERT),
        "the renewed alpha certificate must also reach TCP"
    );
    assert_eq!(
        proxy.tcp("gamma.example.test"),
        leaf(FIXTURE_GAMMA_CERT),
        "the added gamma certificate must also reach TCP"
    );
    let response = proxy.h2();
    assert!(
        response.contains("tcp-h2-after"),
        "the h2 response must come from the origin selected by the prior swap: {response}"
    );

    proxy.shutdown();
}

/// Removing an SNI name is isolated from the rest of the listener: beta must
/// fall back to the default leaf on h3 and TCP, alpha must stay on its own
/// leaf, and h2 must keep serving through the real origin. The removal runs
/// both before and after a config swap; the second origin's body proves the
/// swap is active in the h2 direction.
#[cfg_attr(
    miri,
    ignore = "needs processes and UDP/TCP sockets: miri does not support them (rust-lang/miri#3449)"
)]
#[test]
#[allow(clippy::too_many_lines)]
fn listener_put_removes_an_sni_name_from_http3_and_tcp_before_and_after_a_swap() {
    let origin = start_ok_origin("tcp-h2-before");
    let next_origin = start_ok_origin("tcp-h2-after");
    let proxy = start_proxy("remove-sni", &[ALPHA, BETA], &origin.to_string());

    // Before touching the listener, beta is served on both transports. This
    // is the positive control the removal assertions compare against.
    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the initial alpha certificate must reach h3"
    );
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_BETA_CERT),
        "the initial beta certificate must reach h3"
    );
    assert_eq!(
        proxy.tcp("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the initial alpha certificate must reach TCP"
    );
    assert_eq!(
        proxy.tcp("beta.example.test"),
        leaf(FIXTURE_BETA_CERT),
        "the initial beta certificate must reach TCP"
    );
    assert_h2_beta(
        &proxy,
        "tcp-h2-before",
        FIXTURE_BETA_CERT,
        "before the pre-swap removal",
    );

    // (1) Remove beta before any swap. Beta must fall back on both transports;
    //     alpha must remain untouched and h2 must keep serving.
    commit_listener(&proxy, "v1", &[ALPHA], "v2", "the pre-swap removal");
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_CERT),
        "after removal beta must fall back to the default on h3 before any swap"
    );
    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "removing beta must keep alpha on h3"
    );
    assert_eq!(
        proxy.tcp("beta.example.test"),
        leaf(FIXTURE_CERT),
        "after removal beta must fall back to the default on TCP before any swap"
    );
    assert_eq!(
        proxy.tcp("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "removing beta must keep alpha on TCP"
    );
    assert_h2_beta(
        &proxy,
        "tcp-h2-before",
        FIXTURE_CERT,
        "after the pre-swap removal",
    );

    // (2) A prior config swap commits a new generation; beta stays retired.
    commit_backend(
        &proxy,
        "v2",
        &next_origin.to_string(),
        "v3",
        "the prior backend PUT",
    );
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_CERT),
        "the prior swap must keep beta retired on h3"
    );
    assert_eq!(
        proxy.tcp("beta.example.test"),
        leaf(FIXTURE_CERT),
        "the prior swap must keep beta retired on TCP"
    );
    assert_eq!(
        proxy.tcp("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the prior swap must keep alpha on TCP"
    );
    let h2_after_swap = proxy.h2();
    assert!(
        h2_after_swap.contains("tcp-h2-after"),
        "the prior swap must select the second origin for h2: {h2_after_swap}"
    );

    // (3) Add beta back after the swap, establish it on both transports, then
    //     remove it again. This isolates the post-swap removal from a listener
    //     that never had beta in this generation.
    commit_listener(&proxy, "v3", &[ALPHA, BETA], "v4", "the post-swap add");
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_BETA_CERT),
        "the post-swap add must put beta back on h3"
    );
    assert_eq!(
        proxy.tcp("beta.example.test"),
        leaf(FIXTURE_BETA_CERT),
        "the post-swap add must put beta back on TCP"
    );
    assert_h2_beta(
        &proxy,
        "tcp-h2-after",
        FIXTURE_BETA_CERT,
        "before the post-swap removal",
    );

    commit_listener(&proxy, "v4", &[ALPHA], "v5", "the post-swap removal");
    assert_eq!(
        proxy.h3("beta.example.test"),
        leaf(FIXTURE_CERT),
        "after removal following a prior swap beta must fall back to the default on h3"
    );
    assert_eq!(
        proxy.h3("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the post-swap removal must keep alpha on h3"
    );
    assert_eq!(
        proxy.tcp("beta.example.test"),
        leaf(FIXTURE_CERT),
        "after removal following a prior swap beta must fall back on TCP"
    );
    assert_eq!(
        proxy.tcp("alpha.example.test"),
        leaf(FIXTURE_ALPHA_CERT),
        "the post-swap removal must keep alpha on TCP"
    );
    assert_h2_beta(
        &proxy,
        "tcp-h2-after",
        FIXTURE_CERT,
        "after the post-swap removal",
    );

    proxy.shutdown();
}

// ------------------------------------------------- EXT-H-06, QUIC scope ---

/// A plain-TCP listener at `maxconn = 1` and a QUIC listener at `maxconn = 2`:
/// different caps, so a cross-listener leak shows in both directions. The
/// generous idle/header budgets keep the held connections alive on their own,
/// so the journey never advances by waiting for a timeout.
fn maxconn_config(dir: &Path, backend: &str) -> String {
    format!(
        r#"schema_version = 1
shutdown_grace = "1s"
[listener.tcp]
listen = "127.0.0.1:0"
mode = "http"
backend = "api"
maxconn = 1
idle_timeout = "30s"
client_header_timeout = "30s"
[listener.web]
listen = "127.0.0.1:0"
mode = "http"
backend = "api"
quic = true
maxconn = 2
idle_timeout = "30s"
client_header_timeout = "30s"
[listener.web.tls]
cert = "{}"
key = "{}"
[backend.api]
server = "{backend}"
"#,
        dir.join("server.pem").display(),
        dir.join("server.key").display(),
    )
}

/// The real binary with the two listeners above; the addresses come from the
/// lines it prints when each listener is bound.
struct MaxconnProxy {
    child: ChildGuard,
    dir: PathBuf,
    tcp: SocketAddr,
    quic: SocketAddr,
}

impl MaxconnProxy {
    fn start(tag: &str, backend: &str) -> Self {
        let dir = fixture_dir(tag);
        let config_path = dir.join("eqproxy.toml");
        std::fs::write(&config_path, maxconn_config(&dir, backend)).expect("write config");
        let mut child = ChildGuard(
            bin()
                .args(["run", "--config"])
                .arg(&config_path)
                .args(["--metrics-interval", "1h"])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn eqproxy run"),
        );
        let mut stderr = BufReader::new(child.0.stderr.take().expect("stderr piped"));
        let tcp: SocketAddr = stderr_address(&mut stderr, "listener tcp listening on ")
            .parse()
            .expect("tcp address");
        let quic: SocketAddr = stderr_address(&mut stderr, "listener web listening on ")
            .parse()
            .expect("quic address");
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        Self {
            child,
            dir,
            tcp,
            quic,
        }
    }
}

impl Drop for MaxconnProxy {
    fn drop(&mut self) {
        let _ = self.child.0.kill();
        let _ = self.child.0.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A real HTTP/1.1 origin that answers keep-alive, so a connection the test
/// holds is a **served** one: the listener counted it before the response,
/// which is the event that proves the slot was taken.
fn start_keepalive_origin(body: &'static str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind h1 origin");
    let addr = listener.local_addr().expect("h1 origin addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                while matches!(stream.read(&mut request), Ok(read) if read > 0) {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Origin: h1\r\n\r\n{body}",
                        body.len()
                    );
                    if stream.write_all(response.as_bytes()).is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

/// One keep-alive request/response on an already admitted connection: a
/// complete `200` is the event that proves the listener serves it.
fn h1_keepalive_get(reader: &mut BufReader<TcpStream>) -> String {
    reader
        .get_mut()
        .set_read_timeout(Some(WAIT))
        .expect("read timeout");
    reader
        .get_mut()
        .write_all(b"GET / HTTP/1.1\r\nHost: tcp\r\nConnection: keep-alive\r\n\r\n")
        .expect("write request");
    let mut head = String::new();
    loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).expect("read response line") > 0,
            "the listener closed before the response head: {head:?}"
        );
        if line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
    }
    let length = head
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|value| value.trim().to_owned())
        })
        .and_then(|value| value.parse::<usize>().ok())
        .expect("the response must declare content-length");
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).expect("read response body");
    format!("{head}\r\n{}", String::from_utf8_lossy(&body))
}

/// A connection the listener must refuse: closed with no bytes, because
/// `maxconn` rejects explicitly instead of queueing.
fn refused_without_a_response(addr: SocketAddr) -> bool {
    let mut client = TcpStream::connect(addr).expect("connect listener");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut buf = [0u8; 16];
    matches!(client.read(&mut buf), Ok(0))
}

/// A QUIC client endpoint that offers `h3` and keeps idle connections alive,
/// so a held connection stays up instead of riding an idle timeout.
fn h3_client_endpoint(runtime: &tokio::runtime::Runtime) -> quinn::Endpoint {
    // `quinn::Endpoint::client` spawns its driver on the ambient runtime, so it
    // must be created inside the one the test keeps alive.
    let _entered = runtime.enter();
    let mut config =
        rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .dangerous()
            .with_custom_certificate_verifier(AcceptAny::new())
            .with_no_client_auth();
    config.alpn_protocols = vec![b"h3".to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(config)
        .expect("QUIC client TLS 1.3 config");
    let mut transport = quinn::TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(1)));
    let mut client = quinn::ClientConfig::new(Arc::new(crypto));
    client.transport_config(Arc::new(transport));
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().expect("client addr"))
        .expect("client QUIC endpoint");
    endpoint.set_default_client_config(client);
    endpoint
}

/// A QUIC connection the listener must admit; the endpoint is returned so the
/// connection stays alive (and its slot stays taken) while the test runs.
fn h3_connect(
    runtime: &tokio::runtime::Runtime,
    addr: SocketAddr,
) -> (quinn::Endpoint, quinn::Connection) {
    let endpoint = h3_client_endpoint(runtime);
    let outcome = runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            endpoint
                .connect(addr, "localhost")
                .expect("QUIC connect")
                .await
        })
        .await
    });
    let connection = outcome
        .expect("QUIC handshake within the deadline")
        .unwrap_or_else(|err| panic!("the QUIC listener must admit this handshake: {err:?}"));
    (endpoint, connection)
}

/// A handshake the QUIC listener must refuse because its `maxconn` is spent.
/// The assertion is the QUIC transport close code the listener chose, not an
/// error string and not a deadline.
fn assert_h3_refused(runtime: &tokio::runtime::Runtime, addr: SocketAddr) {
    let endpoint = h3_client_endpoint(runtime);
    let outcome = runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            endpoint
                .connect(addr, "localhost")
                .expect("QUIC connect")
                .await
        })
        .await
    });
    endpoint.close(0u32.into(), b"refused probe");
    runtime.block_on(endpoint.wait_idle());
    match outcome {
        Ok(Err(quinn::ConnectionError::ConnectionClosed(close))) => assert_eq!(
            close.error_code,
            quinn::TransportErrorCode::CONNECTION_REFUSED,
            "a full listener must refuse the handshake with CONNECTION_REFUSED, got {close:?}"
        ),
        Ok(Err(other)) => panic!("the refusal must be a QUIC CONNECTION_CLOSE, not {other:?}"),
        Err(elapsed) => {
            panic!("the handshake neither succeeded nor was refused within {WAIT:?}: {elapsed}")
        }
        Ok(Ok(_)) => panic!("a full QUIC listener must refuse the handshake"),
    }
}

/// EXT-H-06 on the QUIC path: the QUIC listener's `maxconn` is its own, the
/// TCP listener cannot spend it, and the QUIC listener cannot starve TCP. The
/// public `eqproxy_connections_active` gauge being their sum is proved by
/// `eqproxy-core`'s `maxconn_is_scoped_to_each_listener`, so this journey does
/// not read it: it holds real connections and asserts what each one gets.
#[cfg_attr(
    miri,
    ignore = "needs processes and UDP/TCP sockets: miri does not support them (rust-lang/miri#3449)"
)]
#[test]
fn maxconn_is_per_listener_across_tcp_and_quic() {
    let origin = start_keepalive_origin("origin-ok");
    let proxy = MaxconnProxy::start("maxconn-quic", &origin.to_string());
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    // (1) One served, held TCP connection takes the TCP listener's only slot.
    let mut tcp = BufReader::new(TcpStream::connect(proxy.tcp).expect("connect tcp listener"));
    assert!(
        h1_keepalive_get(&mut tcp).contains("origin-ok"),
        "the admitted tcp connection must be served by the origin"
    );

    // (2) The QUIC listener's cap is its own: both handshakes are admitted
    //     while the TCP listener is already at 1/1.
    let (tls_a, quic_a) = h3_connect(&runtime, proxy.quic);
    let (tls_b, quic_b) = h3_connect(&runtime, proxy.quic);
    assert!(
        quic_a.close_reason().is_none() && quic_b.close_reason().is_none(),
        "both admitted QUIC connections must still be open"
    );

    // (3) At its own cap of 2 the QUIC listener refuses the next handshake.
    assert_h3_refused(&runtime, proxy.quic);

    // (4) TCP is still bounded by its own cap: a new connection is closed
    //     without a byte, and the admitted one keeps being served.
    assert!(
        refused_without_a_response(proxy.tcp),
        "a full TCP listener must refuse the second connection"
    );
    assert!(
        h1_keepalive_get(&mut tcp).contains("origin-ok"),
        "the tcp listener must keep serving while the quic listener is at its cap"
    );

    drop((tls_a, tls_b, quic_a, quic_b));
}
