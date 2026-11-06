//! Differential harness: one **declared corpus**
//! of shared-semantics cases ([docs/differential-corpus.md](../../../docs/differential-corpus.md))
//! runs unchanged against EQProxy (the built binary) and the reference
//! implementations — HAProxy 3.4.6, the ingress-nginx data plane (the
//! `nginx` binary shipped inside the official controller image, driven
//! by a static config, like «Envoy estático») and Envoy v1.34.2 with a
//! static bootstrap. Every normalized observable result (status,
//! `Location`, body) must match the case's declared outcome, so the
//! four implementations must agree on the shared semantics.
//!
//! Normalization (declared in the corpus doc): the reason phrase is
//! ignored; `Location` is reduced to path + query when absolute; the
//! body is compared only when the case declares one (3xx bodies differ
//! legitimately per implementation); `chunked` bodies are decoded and
//! `content-encoding: gzip` bodies are gunzipped before comparison; an
//! empty declared response-header value means the header must be absent;
//! comma-joined header values use one canonical separator; and the
//! per-implementation client address (host loopback vs docker gateway) is
//! calibrated at run time and substituted as `CLIENT` in bodies.
//!
//! Header cases (the B-N4/B-I4 executable spec) live on two endpoints,
//! `main` (forwarded headers append) and `replace` (anti-spoofing),
//! each with its own config per implementation. Cases whose EQProxy
//! implementation is still pending its slice carry a `PENDING` skip;
//! the three references assert the declared outcome today.
//!
//! Run (needs Docker; images are pulled on first use):
//!     cargo test -p eqproxy-cli --test differential -- --ignored --nocapture
//!
//! The report is written to `$CARGO_TARGET_TMPDIR/differential-report.md`.

use flate2::read::GzDecoder;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HAPROXY_IMAGE: &str = "haproxy:3.4.6";
const INGRESS_NGINX_IMAGE: &str = "registry.k8s.io/ingress-nginx/controller:v1.13.0";
const ENVOY_IMAGE: &str = "envoyproxy/envoy:v1.34.2";
const BACKEND_IMAGE: &str = "nginx:alpine";

/// Fixed, comma-free body every backend serves under `/z/`. It is long
/// enough to exceed the reference compression thresholds (HAProxy does not
/// compress tiny payloads; nginx and Envoy are configured at 0) and is the
/// same on all three backends, so the case observes compression, not routing.
const PROBE_BODY: &str = "compression differential probe payload: the quick brown fox jumps over the lazy dog while the proxy negotiates a coding and the origin declares a plain text body long enough for every implementation to compress it";

/// Declared outcome of one case (the shared semantics).
struct Outcome {
    status: u16,
    /// Expected `Location`, reduced to path + query.
    location: Option<&'static str>,
    /// Expected body; `None` = not compared (documented per case).
    body: Option<&'static str>,
}

struct Case {
    id: &'static str,
    why: &'static str,
    /// Raw request head (sent verbatim).
    request: &'static str,
    expect: Outcome,
    /// Endpoint serving the case: `"main"` or `"replace"` (each one has
    /// its own config per implementation).
    endpoint: &'static str,
    /// Response headers the case declares as `(name, value)`; checked
    /// like the status and the body. Empty = not compared.
    response_headers: &'static [(&'static str, &'static str)],
    /// (implementation, reason) for documented exclusions: a genuine
    /// semantic divergence in that implementation, or an EQProxy
    /// implementation pending its slice (reason starts with `PENDING`).
    /// Recorded in the report, never asserted there.
    skip: &'static [(&'static str, &'static str)],
}

/// The corpus: path namespaces are disjoint, so one config per
/// implementation covers every case and first-match ordering is real.
const CASES: &[Case] = &[
    Case {
        id: "c1-prefix",
        why: "path_prefix routes the request to its backend",
        request: "GET /c1/users HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /c1/users"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c1-boundary",
        why: "a literal prefix boundary: /c1/ must not swallow /c10",
        request: "GET /c10/users HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("fallback /c10/users"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c2-host",
        why: "host match is case-insensitive",
        request: "GET /c2/x HTTP/1.1\r\nHost: Special.Example.com\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c2/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c2-port",
        why: "host match compares without the :port suffix",
        request: "GET /c2/x HTTP/1.1\r\nHost: special.example.com:8443\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c2/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c2-near",
        why: "a host near miss must not route",
        request: "GET /c2/x HTTP/1.1\r\nHost: other.example.com\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("fallback /c2/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c3-method",
        why: "method condition matches",
        request: "POST /c3/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c3/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c3-near",
        why: "method near miss must not route",
        request: "GET /c3/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("fallback /c3/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c4-header",
        why: "header exact match routes",
        request: "GET /c4/x HTTP/1.1\r\nHost: x\r\nx-beta: on\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c4/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c4-near",
        why: "missing header must not route",
        request: "GET /c4/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("fallback /c4/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c5-first",
        why: "ordered rules: the first matching rule wins over a later one",
        request: "GET /c5/x HTTP/1.1\r\nHost: x\r\nx-beta: on\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c5/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c5-second",
        why: "ordered rules: the later rule catches what the first misses",
        request: "GET /c5/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /c5/x"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c6-redirect",
        why: "local 3xx with Location (body not compared: per-implementation)",
        request: "GET /c6/old HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 301,
            location: Some("/c6/new"),
            body: None,
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c7-direct",
        why: "local direct response (no backend contacted)",
        request: "GET /c7/ping HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("pong"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c8-rewrite",
        why: "prefix rewrite: the backend sees the rewritten target",
        request: "GET /c8/v1/users HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /c8/api/users"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c11-regex",
        why: "path regex routes .json paths (spelling per implementation, see corpus doc)",
        request: "GET /c11/data.json HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("mgmt /c11/data.json"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "c11-boundary",
        why: "regex near miss (.jsonx) must not route",
        request: "GET /c11/data.jsonx HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("fallback /c11/data.jsonx"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    // --- header cases (the B-N4/B-I4 executable spec) ------------------
    Case {
        id: "h1-xff-append",
        why: "XFF grows: client-sent hops are kept, the client is appended",
        request: "GET /h1/x HTTP/1.1\r\nHost: x\r\nX-Forwarded-For: 10.9.9.9\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h1/x xff=[10.9.9.9, CLIENT] real=[] beta=[] gone=[] added=[]"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h2-xff-xri-replace",
        why: "anti-spoofing: XFF and X-Real-IP carry the real client only",
        request: "GET /h2/x HTTP/1.1\r\nHost: x\r\nX-Forwarded-For: 10.9.9.9\r\nX-Real-IP: 1.2.3.4\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h2/x xff=[CLIENT] real=[CLIENT] beta=[] gone=[] added=[]"),
        },
        endpoint: "replace",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h4-xfp",
        why: "X-Forwarded-Proto carries the client scheme",
        request: "GET /h4/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h4/x xff=[CLIENT] real=[] proto=[http] beta=[] gone=[] added=[]"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h5-set-header",
        why: "set-header replaces the client's value on the forwarded request",
        request: "GET /h5/x HTTP/1.1\r\nHost: x\r\nx-beta: off\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h5/x xff=[CLIENT] real=[] beta=[on] gone=[] added=[]"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h6-del-header",
        why: "del-header removes the client's header before forwarding",
        request: "GET /h6/x HTTP/1.1\r\nHost: x\r\nx-gone: please\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h6/x xff=[CLIENT] real=[] beta=[] gone=[] added=[]"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h7-add-header",
        why: "add-header combines the added value with the client's",
        request: "GET /h7/x HTTP/1.1\r\nHost: x\r\nx-added: base\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h7/x xff=[CLIENT] real=[] beta=[] gone=[] added=[base, extra]"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    Case {
        id: "h8-response-marker",
        why: "response headers can be set on the way back",
        request: "GET /h8/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("api /h8/x xff=[CLIENT] real=[] beta=[] gone=[] added=[]"),
        },
        endpoint: "main",
        response_headers: &[("x-marker", "v06")],
        skip: &[],
    },
    // --- compression cases (the B-N6/H6 executable spec) ----------------
    // The body is compared after transfer and content decoding: the corpus
    // declares the payload, not the wire framing each implementation picks.
    Case {
        id: "z1-gzip",
        why: "a gzip-capable client gets gzip and the same decoded body",
        request: "GET /z/probe HTTP/1.1\r\nHost: x\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some(PROBE_BODY),
        },
        endpoint: "main",
        response_headers: &[("content-encoding", "gzip")],
        skip: &[],
    },
    Case {
        id: "z2-no-accept-encoding",
        why: "no Accept-Encoding means identity and no content-encoding",
        request: "GET /z/probe HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some(PROBE_BODY),
        },
        endpoint: "main",
        // An empty declared value means the header must be absent.
        response_headers: &[("content-encoding", "")],
        skip: &[],
    },
    Case {
        id: "z3-identity",
        why: "Accept-Encoding: identity is not compressed",
        request: "GET /z/probe HTTP/1.1\r\nHost: x\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some(PROBE_BODY),
        },
        endpoint: "main",
        response_headers: &[("content-encoding", "")],
        skip: &[],
    },
    // --- custom error page on an upstream connect failure -------------
    Case {
        id: "e1-error-page-503",
        why: "a backend connect failure is replaced by the configured error page",
        request: "GET /e1/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 503,
            location: None,
            body: Some("custom-503"),
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[],
    },
    // --- TLS termination ------------------------------------------------
    Case {
        id: "t1-tls-http",
        why: "a TLS client completes a real handshake and gets the declared body",
        request: "GET /tls/ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        expect: Outcome {
            status: 200,
            location: None,
            body: Some("tls-ok"),
        },
        endpoint: "tls",
        response_headers: &[],
        skip: &[],
    },
    // --- protocol limit -------------------------------------------------
    Case {
        id: "l1-duplicate-content-length",
        why: "two conflicting Content-Length fields are rejected, never forwarded",
        request: "POST /l1/x HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nContent-Length: 2\r\nConnection: close\r\n\r\na",
        expect: Outcome {
            status: 400,
            location: None,
            body: None,
        },
        endpoint: "main",
        response_headers: &[],
        skip: &[(
            "haproxy",
            "HAProxy closes the connection without an HTTP response on duplicate Content-Length; recorded as a documented divergence",
        )],
    },
];

/// Cache cases: the same request is sent twice. Every backend answers
/// `/cache/` with a fresh per-request UUID, so two identical bodies prove
/// the second response was served from the cache without reaching the
/// origin; two different bodies prove the request bypassed or missed it.
struct CacheCase {
    id: &'static str,
    why: &'static str,
    request: &'static str,
    /// Prefix every origin body starts with (it carries the backend id).
    body_prefix: &'static str,
    /// `true`: the second response must equal the first (a HIT).
    /// `false`: it must differ (the origin was reached again).
    expect_hit: bool,
}

const CACHE_CASES: &[CacheCase] = &[
    CacheCase {
        id: "k1-miss-then-hit",
        why: "a cacheable GET is fetched once and the second request is served from the cache",
        request: "GET /cache/k1 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        body_prefix: "cache api ",
        expect_hit: true,
    },
    CacheCase {
        id: "k2-no-store-bypass",
        why: "an origin no-store response is never stored, so both requests reach the origin",
        request: "GET /cache/nostore HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        body_prefix: "cache-nostore api ",
        expect_hit: false,
    },
];

// --- docker orchestration -------------------------------------------------

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker")
        .args(args)
        .output()
        .expect("run docker (is Docker running?)");
    assert!(
        out.status.success(),
        "docker {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Cleanup helper: never panics.
fn docker_quiet(args: &[&str]) {
    let _ = Command::new("docker").args(args).output();
}

/// Owns the docker network and containers; Drop removes them even when
/// the test panics mid-assertion.
struct Scope {
    network: String,
    containers: Vec<String>,
}

impl Scope {
    fn new() -> Self {
        let network = format!("eqdiff-{}", std::process::id());
        docker(&["network", "create", "--subnet", "172.23.0.0/24", &network]);
        Scope {
            network,
            containers: Vec::new(),
        }
    }

    /// `before` are docker run flags (network, mounts, entrypoint
    /// override); `after` are command arguments placed after the image.
    fn run(&mut self, name: &str, image: &str, before: &[&str], after: &[&str]) {
        let mut args = vec![
            "run".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
            "--network".to_owned(),
            self.network.clone(),
        ];
        args.extend(before.iter().map(|a| (*a).to_owned()));
        args.push(image.to_owned());
        args.extend(after.iter().map(|a| (*a).to_owned()));
        let out = Command::new("docker")
            .args(&args)
            .output()
            .expect("run docker");
        assert!(
            out.status.success(),
            "docker {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self.containers.push(name.to_owned());
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        for name in &self.containers {
            docker_quiet(&["rm", "-f", name]);
        }
        docker_quiet(&["network", "rm", &self.network]);
    }
}

fn published_port(name: &str, inner: u16) -> u16 {
    for _ in 0..50 {
        let out = docker(&["port", name, &format!("{inner}/tcp")]);
        if let Some(port) = out.lines().next().and_then(|l| l.rsplit(':').next())
            && let Ok(p) = port.trim().parse()
        {
            return p;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no published host port for {name}:{inner}");
}

fn wait_tcp(addr: &str, label: &str, logs: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("{label} never accepted TCP on {addr}\n--- diagnostics ---\n{logs}");
}

fn container_logs(name: &str) -> String {
    docker(&["logs", "--tail", "30", name])
}

// --- requests and normalization ------------------------------------------

/// One parsed response. The body stays as bytes until the transfer and
/// content codings are undone: gzip output is not UTF-8.
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn http_request(addr: &str, raw: &str) -> Response {
    let mut stream = TcpStream::connect(addr).expect("connect to implementation");
    stream.write_all(raw.as_bytes()).expect("write request");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut wire = Vec::new();
    let _ = stream.read_to_end(&mut wire);
    parse_response(&wire)
}

/// Drive the public TLS listener through the host's OpenSSL client. This is
/// a real TLS handshake, not a plaintext request to a TLS port.
fn https_request(addr: &str, raw: &str) -> Response {
    let mut child = Command::new("openssl")
        .args([
            "s_client",
            "-quiet",
            "-connect",
            addr,
            "-servername",
            "localhost",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn openssl s_client");
    child
        .stdin
        .take()
        .expect("openssl stdin")
        .write_all(raw.as_bytes())
        .expect("write TLS request");
    let output = child.wait_with_output().expect("wait for openssl");
    parse_response(&output.stdout)
}

fn parse_response(wire: &[u8]) -> Response {
    let split = wire.windows(4).position(|window| window == b"\r\n\r\n");
    let (head, body) = match split {
        Some(index) => (&wire[..index], &wire[index + 4..]),
        None => (wire, &[] as &[u8]),
    };
    let head = String::from_utf8_lossy(head);
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect();
    let body = decode_body(&headers, body);
    Response {
        status,
        headers,
        body,
    }
}

/// Undo the transfer coding and the content coding each implementation
/// chooses. The corpus declares the payload, not the framing: the references
/// differ on `Content-Length` vs `chunked` and on whether they compress.
fn decode_body(headers: &[(String, String)], raw: &[u8]) -> Vec<u8> {
    let chunked = headers.iter().any(|(name, value)| {
        name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked")
    });
    let raw = if chunked { dechunk(raw) } else { raw.to_vec() };
    let encoding = headers
        .iter()
        .find(|(name, _)| name == "content-encoding")
        .map(|(_, value)| value.to_ascii_lowercase());
    match encoding.as_deref() {
        Some("gzip") => {
            let mut decoded = Vec::new();
            GzDecoder::new(&raw[..])
                .read_to_end(&mut decoded)
                .expect("decode the gzip response body");
            decoded
        }
        Some(other) => panic!("the differential harness cannot decode content-encoding {other:?}"),
        None => raw,
    }
}

/// Decode an HTTP/1.1 chunked body down to its payload. Chunk extensions and
/// trailers are accepted and discarded; the corpus does not assert framing.
fn dechunk(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = raw;
    loop {
        let Some(end) = rest.windows(2).position(|window| window == b"\r\n") else {
            panic!("chunked body without a size line");
        };
        let line = std::str::from_utf8(&rest[..end]).expect("chunk size is ASCII");
        let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
            .expect("chunk size is hexadecimal");
        rest = &rest[end + 2..];
        if size == 0 {
            return out;
        }
        assert!(
            rest.len() >= size + 2,
            "chunked body truncated: {size} bytes plus CRLF, {} available",
            rest.len()
        );
        out.extend_from_slice(&rest[..size]);
        assert_eq!(
            &rest[size..size + 2],
            b"\r\n",
            "chunk not terminated by CRLF"
        );
        rest = &rest[size + 2..];
    }
}

struct Normalized {
    status: u16,
    location: Option<String>,
    body: String,
    headers: Vec<(String, String)>,
}

fn normalize(response: Response, client_ip: &str) -> Normalized {
    let Response {
        status,
        headers,
        body,
    } = response;
    let location = headers
        .iter()
        .find_map(|(name, value)| (name == "location").then(|| reduce_location(value)));
    Normalized {
        status,
        location,
        body: normalize_body(&String::from_utf8_lossy(&body), client_ip),
        headers,
    }
}

/// Body normalization (declared): comma-joined header values use one
/// canonical separator, and the client address — topology-dependent, a
/// host loopback for the native binary vs the docker gateway for the
/// containerized references — becomes the `CLIENT` token.
fn normalize_body(body: &str, client_ip: &str) -> String {
    body.replace(client_ip, "CLIENT")
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Calibration (declared in the corpus doc): the client address each
/// implementation sees is discovered at run time instead of hardcoded —
/// the append endpoint echoes it as the sole XFF entry of a probe sent
/// without XFF.
fn probe_client_ip(addr: &str) -> String {
    let response = http_request(
        addr,
        "GET /h1/probe HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    let body = String::from_utf8_lossy(&response.body).into_owned();
    let ip = body
        .find("xff=[")
        .map(|i| body[i + 5..].split(']').next().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    assert!(!ip.is_empty(), "calibration probe saw no XFF entry: {body}");
    ip
}

/// Absolute `Location`s are reduced to path + query: the scheme and
/// authority legitimately differ per deployment, the redirect target is
/// the shared semantics.
fn reduce_location(value: &str) -> String {
    let rest = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"));
    match rest {
        Some(rest) => match rest.find('/') {
            Some(i) => rest[i..].to_owned(),
            None => "/".to_owned(),
        },
        None => value.to_owned(),
    }
}

fn summary(got: &Normalized, declared_headers: &[(&str, &str)]) -> String {
    let mut out = got.status.to_string();
    if let Some(location) = &got.location {
        out.push_str(&format!(" -> {location}"));
    }
    out.push_str(&format!(" {:?}", got.body));
    // Show what the case asserts, including absence, so the report carries
    // the observable and not only the verdict.
    for (name, _) in declared_headers {
        let value = got
            .headers
            .iter()
            .find(|(header, _)| header == name)
            .map_or("<absent>", |(_, value)| value.as_str());
        out.push_str(&format!(" {name}={value}"));
    }
    out
}

fn check(case: &Case, got: &Normalized) -> Result<(), String> {
    if got.status != case.expect.status {
        return Err(format!(
            "status {} != declared {}",
            got.status, case.expect.status
        ));
    }
    if let Some(want) = case.expect.location
        && got.location.as_deref() != Some(want)
    {
        return Err(format!("location {:?} != declared {want:?}", got.location));
    }
    if let Some(want) = case.expect.body
        && got.body != want
    {
        return Err(format!("body {:?} != declared {want:?}", got.body));
    }
    for (name, want) in case.response_headers {
        let got_value = got
            .headers
            .iter()
            .find(|(n, _)| n.as_str() == *name)
            .map(|(_, v)| v.as_str());
        // An empty declared value means the header must be absent: the
        // compression cases assert that identity responses carry no
        // `content-encoding`, which a body comparison alone cannot see once
        // the harness decodes it.
        if want.is_empty() {
            if got_value.is_some() {
                return Err(format!(
                    "response header {name} must be absent, got {got_value:?}"
                ));
            }
            continue;
        }
        if got_value != Some(*want) {
            return Err(format!(
                "response header {name} {got_value:?} != declared {want:?}"
            ));
        }
    }
    Ok(())
}

// --- generated configs (one per implementation) ---------------------------

/// Backends answer `<id> <request_uri>` so the received target — and
/// therefore any rewrite — is observable in the body. Header cases use
/// `/h…` paths, where the answer echoes the forwarded headers instead
/// (`/h4/` also echoes `X-Forwarded-Proto`, which Envoy sets natively
/// while the others are opt-in).
fn backend_conf(id: &str) -> String {
    let probe = PROBE_BODY;
    format!(
        "server {{\n    listen 80;\n    server_name _;\n    location /h4 {{\n        return 200 \"{id} $request_uri xff=[$http_x_forwarded_for] real=[$http_x_real_ip] proto=[$http_x_forwarded_proto] beta=[$http_x_beta] gone=[$http_x_gone] added=[$http_x_added]\";\n    }}\n    location /h {{\n        return 200 \"{id} $request_uri xff=[$http_x_forwarded_for] real=[$http_x_real_ip] beta=[$http_x_beta] gone=[$http_x_gone] added=[$http_x_added]\";\n    }}\n    location = /cache/nostore {{\n        default_type text/plain;\n        add_header Cache-Control no-store always;\n        return 200 'cache-nostore {id} $request_id';\n    }}\n    location /cache/ {{\n        default_type text/plain;\n        add_header Cache-Control 'public, max-age=60' always;\n        return 200 'cache {id} $request_id';\n    }}\n    location /z/ {{\n        default_type text/plain;\n        return 200 \"{probe}\";\n    }}\n    location / {{\n        return 200 \"{id} $request_uri\";\n    }}\n}}\n"
    )
}

/// HAProxy 3.4.6. `http-request` rules always run before `use_backend`
/// rules, so the c8 rewrite lands before the c8 routing decision.
fn haproxy_cfg() -> String {
    r#"global
    maxconn 256
defaults
    mode http
    timeout connect 5s
    timeout client 30s
    timeout server 30s
    errorfile 503 /usr/local/etc/haproxy/errors/503.http
cache diffcache
    total-max-size 8
    max-object-size 65536
    max-age 60
frontend fe
    bind :8080
    # Cache case (k1): store the cacheable probe for the second request.
    http-request cache-use diffcache if { path_beg /cache/ }
    # A response rule cannot evaluate path_beg; the cache itself ignores
    # responses that are not cacheable (no public max-age).
    http-response cache-store diffcache
    # Compression cases (z1-z3): gzip for the origin's text/plain payload.
    compression algo gzip
    compression type text/plain
    # Header cases (B-N4/B-I4 spec), scoped to /h/; values quoted
    # because log-format strings may contain spaces.
    http-request set-header X-Forwarded-For "%[hdr(x-forwarded-for)], %[src]" if { path_beg /h } { hdr(x-forwarded-for) -m found }
    http-request set-header X-Forwarded-For "%[src]" if { path_beg /h } !{ hdr(x-forwarded-for) -m found }
    http-request set-header X-Forwarded-Proto "http" if { path_beg /h4/ }
    http-request set-header x-beta on if { path_beg /h5/ }
    http-request del-header x-gone if { path_beg /h6/ }
    http-request add-header x-added extra if { path_beg /h7/ }
    http-response set-header x-marker v06
    http-request redirect code 301 location /c6/new if { path /c6/old }
    http-request return status 200 content-type "text/plain" string "pong" if { path /c7/ping }
    http-request set-path %[path,regsub(^/c8/v1/,/c8/api/)] if { path_beg /c8/v1/ }
    use_backend api if { path_beg /h }
    use_backend api if { path_beg /cache/ }
    use_backend dead if { path_beg /e1/ }
    use_backend api if { path_beg /c1/ }
    use_backend mgmt if { path_beg /c2/ } { req.hdr(host),lower,field(1,:) -m str special.example.com }
    use_backend mgmt if { path_beg /c3/ } { method POST }
    use_backend mgmt if { path_beg /c4/ } { req.hdr(x-beta) -m str on }
    use_backend mgmt if { path_beg /c5/ } { req.hdr(x-beta) -m str on }
    use_backend api if { path_beg /c5/ }
    use_backend api if { path_beg /c8/api/ }
    use_backend mgmt if { path_beg /c11/ } { path -m reg \.json$ }
    default_backend fallback
frontend fe-tls
    bind :8443 ssl crt /usr/local/etc/haproxy/tls.pem
    http-request return status 200 content-type "text/plain" string "tls-ok" if { path /tls/ping }
    default_backend fallback
frontend fe2
    bind :8081
    # Anti-spoofing endpoint: both forwarded headers carry the real
    # client only (h2).
    http-request set-header X-Forwarded-For "%[src]"
    http-request set-header X-Real-IP "%[src]"
    use_backend api
backend api
    server s1 172.23.0.11:80
backend mgmt
    server s1 172.23.0.12:80
backend fallback
    server s1 172.23.0.13:80
backend dead
    server s1 172.23.0.11:1
"#
    .to_owned()
}

/// Envoy v1.34 static bootstrap. `strip_any_host_port` declares the
/// port-free host comparison shared with the other implementations.
fn envoy_yaml() -> String {
    r#"admin:
  access_log_path: /dev/null
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
static_resources:
  listeners:
    - name: main
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: differential
                strip_any_host_port: true
                use_remote_address: true
                skip_xff_append: true
                local_reply_config:
                  mappers:
                    - filter:
                        status_code_filter:
                          comparison:
                            op: EQ
                            value:
                              default_value: 503
                              runtime_key: differential_local_503
                      status_code: 503
                      body:
                        inline_string: "custom-503"
                http_filters:
                  - name: envoy.filters.http.compressor
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.compressor.v3.Compressor
                      compressor_library:
                        name: gzip
                        typed_config:
                          "@type": type.googleapis.com/envoy.extensions.compression.gzip.compressor.v3.Gzip
                      response_direction_config:
                        common_config:
                          min_content_length: 0
                          content_type:
                            - text/plain
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
                route_config:
                  name: differential
                  virtual_hosts:
                    - name: special
                      domains: ["special.example.com"]
                      routes:
                        - match: { prefix: "/c2/" }
                          route: { cluster: mgmt }
                        - match: { prefix: "/" }
                          route: { cluster: fallback }
                    - name: default
                      domains: ["*"]
                      request_headers_to_add:
                        - header: { key: "x-forwarded-for", value: "%DOWNSTREAM_REMOTE_ADDRESS_WITHOUT_PORT%" }
                          append_action: APPEND_IF_EXISTS_OR_ADD
                      routes:
                        - match: { prefix: "/c1/" }
                          route: { cluster: api }
                        - match: { prefix: "/c3/", headers: [{ name: ":method", exact_match: "POST" }] }
                          route: { cluster: mgmt }
                        - match: { prefix: "/c4/", headers: [{ name: "x-beta", exact_match: "on" }] }
                          route: { cluster: mgmt }
                        - match: { prefix: "/c5/", headers: [{ name: "x-beta", exact_match: "on" }] }
                          route: { cluster: mgmt }
                        - match: { prefix: "/c5/" }
                          route: { cluster: api }
                        - match: { path: "/c6/old" }
                          redirect: { path_redirect: "/c6/new" }
                        - match: { path: "/c7/ping" }
                          direct_response: { status: 200, body: { inline_string: "pong" } }
                        - match: { prefix: "/c8/v1/" }
                          route: { cluster: api, prefix_rewrite: "/c8/api/" }
                        - match: { safe_regex: { google_re2: {}, regex: "/c11/.*\\.json$" } }
                          route: { cluster: mgmt }
                        - match: { prefix: "/h5" }
                          route: { cluster: api }
                          request_headers_to_remove: ["x-beta"]
                          request_headers_to_add: [{ header: { key: "x-beta", value: "on" }, append_action: OVERWRITE_IF_EXISTS_OR_ADD }]
                        - match: { prefix: "/h6" }
                          route: { cluster: api }
                          request_headers_to_remove: ["x-gone"]
                        - match: { prefix: "/h7" }
                          route: { cluster: api }
                          request_headers_to_add: [{ header: { key: "x-added", value: "extra" }, append_action: APPEND_IF_EXISTS_OR_ADD }]
                        - match: { prefix: "/h8" }
                          route: { cluster: api }
                          response_headers_to_add: [{ header: { key: "x-marker", value: "v06" } }]
                        - match: { prefix: "/h" }
                          route: { cluster: api }
                        - match: { prefix: "/e1/" }
                          route: { cluster: dead }
                        - match: { prefix: "/" }
                          route: { cluster: fallback }
    - name: tls
      address:
        socket_address: { address: 0.0.0.0, port_value: 8443 }
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/diff-cert.pem }
                    private_key: { filename: /tmp/diff-key.pem }
          filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: tls
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
                route_config:
                  name: tls
                  virtual_hosts:
                    - name: all
                      domains: ["*"]
                      routes:
                        - match: { path: "/tls/ping" }
                          direct_response: { status: 200, body: { inline_string: "tls-ok" } }
                        - match: { prefix: "/" }
                          direct_response: { status: 404 }
    - name: replace
      address:
        socket_address: { address: 0.0.0.0, port_value: 8081 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: replace
                strip_any_host_port: true
                use_remote_address: true
                skip_xff_append: true
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
                route_config:
                  name: replace
                  virtual_hosts:
                    - name: all
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/h2" }
                          route: { cluster: api }
                          request_headers_to_remove: ["x-forwarded-for", "x-real-ip"]
                          request_headers_to_add:
                            - header: { key: "x-forwarded-for", value: "%DOWNSTREAM_REMOTE_ADDRESS_WITHOUT_PORT%" }
                            - header: { key: "x-real-ip", value: "%DOWNSTREAM_REMOTE_ADDRESS_WITHOUT_PORT%" }
                        - match: { prefix: "/" }
                          route: { cluster: api }
    - name: cache
      address:
        socket_address: { address: 0.0.0.0, port_value: 8082 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: cache
                http_filters:
                  - name: envoy.filters.http.cache
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.cache.v3.CacheConfig
                      typed_config:
                        "@type": type.googleapis.com/envoy.extensions.http.cache.simple_http_cache.v3.SimpleHttpCacheConfig
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
                route_config:
                  name: cache
                  virtual_hosts:
                    - name: all
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/cache/" }
                          route: { cluster: api }
                        - match: { prefix: "/" }
                          route: { cluster: fallback }
  clusters:
    - name: api
      connect_timeout: 5s
      type: STATIC
      load_assignment:
        cluster_name: api
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.23.0.11, port_value: 80 }
    - name: mgmt
      connect_timeout: 5s
      type: STATIC
      load_assignment:
        cluster_name: mgmt
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.23.0.12, port_value: 80 }
    - name: fallback
      connect_timeout: 5s
      type: STATIC
      load_assignment:
        cluster_name: fallback
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.23.0.13, port_value: 80 }
    - name: dead
      connect_timeout: 5s
      type: STATIC
      load_assignment:
        cluster_name: dead
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.23.0.11, port_value: 1 }
"#
    .to_owned()
}

/// ingress-nginx data plane: the `nginx` binary from the official
/// controller image (nginx/1.27.x), driven by a static config in the
/// shapes the controller generates for these features. The controller
/// (k8s object → config translation) is NOT exercised; see the corpus doc.
fn ingress_nginx_conf() -> String {
    r#"worker_processes 1;
error_log /dev/stderr warn;
pid /tmp/diff-nginx.pid;
events { worker_connections 128; }
http {
    client_body_temp_path /tmp/diff-client-body;
    proxy_temp_path /tmp/diff-proxy-temp;
    proxy_cache_path /tmp/diff-cache levels=1:2 keys_zone=diffcache:10m max_size=16m inactive=60s use_temp_path=off;
    access_log off;
    map $request_method $c3_backend {
        "POST"  172.23.0.12:80;
        default 172.23.0.13:80;
    }
    map $http_x_beta $c4_backend {
        "on"    172.23.0.12:80;
        default 172.23.0.13:80;
    }
    map $http_x_beta $c5_backend {
        "on"    172.23.0.12:80;
        default 172.23.0.11:80;
    }
    server {
        listen 8080;
        server_name special.example.com;
        location /c2/ { proxy_pass http://172.23.0.12:80; }
        location / { proxy_pass http://172.23.0.13:80; }
    }
    server {
        listen 8080 default_server;
        server_name _;
        gzip on;
        gzip_types text/plain;
        gzip_min_length 0;
        location = /cache/nostore {
            proxy_cache diffcache;
            proxy_cache_valid 200 60s;
            proxy_pass http://172.23.0.11:80;
        }
        location /cache/ {
            proxy_cache diffcache;
            proxy_cache_valid 200 60s;
            proxy_pass http://172.23.0.11:80;
        }
        location /c1/ { proxy_pass http://172.23.0.11:80; }
        location /c3/ { proxy_pass http://$c3_backend; }
        location /c4/ { proxy_pass http://$c4_backend; }
        location /c5/ { proxy_pass http://$c5_backend; }
        location = /c6/old { return 301 /c6/new; }
        location = /c7/ping { return 200 "pong"; }
        location /e1/ {
            proxy_pass http://172.23.0.11:1;
            proxy_intercept_errors on;
            error_page 502 = @custom502;
        }
        location @custom502 { return 503 "custom-503"; }
        location /c8/v1/ { rewrite ^/c8/v1/(.*)$ /c8/api/$1 break; proxy_pass http://172.23.0.11:80; }
        location ~* ^/c11/.*\.json$ { proxy_pass http://172.23.0.12:80; }
        location /h4 {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_pass http://172.23.0.11:80;
        }
        location /h5 {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Beta on;
            proxy_pass http://172.23.0.11:80;
        }
        location /h6 {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Gone "";
            proxy_pass http://172.23.0.11:80;
        }
        location /h7 {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Added "$http_x_added, extra";
            proxy_pass http://172.23.0.11:80;
        }
        location /h8 {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_pass http://172.23.0.11:80;
            add_header x-marker v06;
        }
        location /h {
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_pass http://172.23.0.11:80;
        }
        location / { proxy_pass http://172.23.0.13:80; }
    }
    server {
        listen 8443 ssl;
        server_name _;
        ssl_certificate /tmp/diff-cert.pem;
        ssl_certificate_key /tmp/diff-key.pem;
        location = /tls/ping { return 200 "tls-ok"; }
        location / { return 404; }
    }
    server {
        listen 8081;
        server_name _;
        location /h {
            proxy_set_header X-Forwarded-For $remote_addr;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_pass http://172.23.0.11:80;
        }
        location / { proxy_pass http://172.23.0.13:80; }
    }
}
"#
    .to_owned()
}

/// EQProxy: the same corpus as typed routing rules (the format under
/// test), backends on the published host ports of the backend containers.
fn eqproxy_toml(
    api: u16,
    mgmt: u16,
    fallback: u16,
    cert: &std::path::Path,
    key: &std::path::Path,
) -> String {
    let cert = cert.display();
    let key = key.display();
    format!(
        r#"schema_version = 1
[listener.web]
listen = "127.0.0.1:0"
mode = "http"
backend = "fallback"
forwarded_headers = "append"

# Compression cases (z1-z3): the body below the default 1024-byte gate is
# still eligible, and only text/plain is compressed.
[listener.web.compression]
gzip = true
min_size = 0
types = ["text/plain"]

[[listener.web.error_pages]]
codes = [502]
status = 503
body = "custom-503"


[[listener.web.rules]]
name = "cache"
path_prefix = "/cache/"
use_backend = "api"

[[listener.web.rules]]
name = "e1"
path_prefix = "/e1/"
use_backend = "dead"

[[listener.web.rules]]
name = "c1"
path_prefix = "/c1/"
use_backend = "api"

[[listener.web.rules]]
name = "c2"
path_prefix = "/c2/"
host = "special.example.com"
use_backend = "mgmt"

[[listener.web.rules]]
name = "c3"
path_prefix = "/c3/"
methods = ["POST"]
use_backend = "mgmt"

[[listener.web.rules]]
name = "c4"
path_prefix = "/c4/"
headers = [{{ name = "x-beta", exact = "on" }}]
use_backend = "mgmt"

[[listener.web.rules]]
name = "c5a"
path_prefix = "/c5/"
headers = [{{ name = "x-beta", exact = "on" }}]
use_backend = "mgmt"

[[listener.web.rules]]
name = "c5b"
path_prefix = "/c5/"
use_backend = "api"

[[listener.web.rules]]
name = "c6"
path = "/c6/old"
[listener.web.rules.redirect]
status = 301
location = "/c6/new"

[[listener.web.rules]]
name = "c7"
path = "/c7/ping"
[listener.web.rules.direct_response]
status = 200
body = "pong"

[[listener.web.rules]]
name = "c8"
path_prefix = "/c8/v1/"
use_backend = "api"
[listener.web.rules.rewrite]
path_prefix_rewrite = "/c8/api/"

[[listener.web.rules]]
name = "c11"
path_regex = "^/c11/.*\\.json$"
use_backend = "mgmt"

[[listener.web.rules]]
name = "h5"
path_prefix = "/h5"
use_backend = "api"
set_headers = [{{ name = "x-beta", value = "on" }}]

[[listener.web.rules]]
name = "h6"
path_prefix = "/h6"
use_backend = "api"
del_headers = ["x-gone"]

[[listener.web.rules]]
name = "h7"
path_prefix = "/h7"
use_backend = "api"
add_headers = [{{ name = "x-added", value = "extra" }}]

[[listener.web.rules]]
name = "h8"
path_prefix = "/h8"
use_backend = "api"
response_headers = [{{ name = "x-marker", value = "v06" }}]

[[listener.web.rules]]
name = "h"
path_prefix = "/h"
use_backend = "api"

[listener.replace]
listen = "127.0.0.1:0"
mode = "http"
backend = "api"
forwarded_headers = "replace"

# Cache case (k1): a dedicated listener so the cache never serves a
# different case's response (the corpus reuses paths across headers).
[listener.cache]
listen = "127.0.0.1:0"
mode = "http"
backend = "api"

[listener.cache.cache]
default_ttl = "60s"

[[listener.cache.rules]]
name = "cache"
path_prefix = "/cache/"
use_backend = "api"

[listener.secure]
listen = "127.0.0.1:0"
mode = "http"
backend = "fallback"
[listener.secure.tls]
cert = "{cert}"
key = "{key}"
[[listener.secure.rules]]
name = "tls"
path = "/tls/ping"
[listener.secure.rules.direct_response]
status = 200
body = "tls-ok"

[backend.api]
server = "127.0.0.1:{api}"
[backend.mgmt]
server = "127.0.0.1:{mgmt}"
[backend.fallback]
server = "127.0.0.1:{fallback}"
[backend.dead]
server = "127.0.0.1:1"
"#
    )
}

// --- evidence helpers -----------------------------------------------------

fn sha256(path: &str) -> String {
    if let Ok(out) = Command::new("shasum").args(["-a", "256", path]).output()
        && out.status.success()
    {
        return String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("unavailable")
            .to_owned();
    }
    if let Ok(out) = Command::new("sha256sum").arg(path).output()
        && out.status.success()
    {
        return String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("unavailable")
            .to_owned();
    }
    "unavailable".to_owned()
}

fn image_id(tag: &str) -> String {
    docker(&["image", "inspect", "--format", "{{.Id}}", tag])
        .trim()
        .to_owned()
}

/// Keeps the EQProxy child from leaking when the test panics.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let pid = self.0.id();
        let _ = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            if Instant::now() >= deadline {
                let _ = self.0.kill();
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

// --- the harness ----------------------------------------------------------

#[cfg_attr(
    miri,
    ignore = "needs processes, TCP sockets and Docker: miri does not support them (rust-lang/miri#3449)"
)]
#[ignore = "differential harness: needs Docker + reference images; run: cargo test -p eqproxy-cli --test differential -- --ignored --nocapture"]
#[test]
fn differential_shared_semantics_corpus() {
    let work = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("differential-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("create work dir");
    let write = |name: &str, contents: &str| {
        let path = work.join(name);
        std::fs::write(&path, contents).expect("write config");
        path
    };

    // Configs, written once (the corpus doc references these shapes).
    let backend_confs: Vec<_> = ["api", "mgmt", "fallback"]
        .iter()
        .map(|id| write(&format!("{id}.conf"), &backend_conf(id)))
        .collect();
    let haproxy_cfg = write("haproxy.cfg", &haproxy_cfg());
    let envoy_yaml = write("envoy.yaml", &envoy_yaml());
    let nginx_conf = write("ingress-nginx.conf", &ingress_nginx_conf());
    let haproxy_error = write(
        "haproxy-503.http",
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain\r\nContent-Length: 10\r\n\r\ncustom-503",
    );
    let tls_cert = PathBuf::from(env!("OUT_DIR")).join("fixtures/cert.pem");
    let tls_key = PathBuf::from(env!("OUT_DIR")).join("fixtures/key.pem");
    let mut hap_tls_bytes = std::fs::read(&tls_cert).expect("read TLS certificate");
    hap_tls_bytes.extend(std::fs::read(&tls_key).expect("read TLS key"));
    let haproxy_tls = work.join("haproxy-tls.pem");
    std::fs::write(&haproxy_tls, hap_tls_bytes).expect("write combined HAProxy PEM");

    let mut scope = Scope::new();

    // Backends on fixed container IPs (references) + published host
    // ports (native EQProxy reaches them through 127.0.0.1).
    let mut backend_host_ports = Vec::new();
    for (id, ip, conf) in [
        ("api", "172.23.0.11", &backend_confs[0]),
        ("mgmt", "172.23.0.12", &backend_confs[1]),
        ("fallback", "172.23.0.13", &backend_confs[2]),
    ] {
        let name = format!("eqdiff-{id}-{}", std::process::id());
        scope.run(
            &name,
            BACKEND_IMAGE,
            &[
                "--ip",
                ip,
                "-p",
                "127.0.0.1::80",
                "-v",
                &format!("{}:/etc/nginx/conf.d/default.conf:ro", conf.display()),
            ],
            &[],
        );
        let port = published_port(&name, 80);
        wait_tcp(
            &format!("127.0.0.1:{port}"),
            &format!("backend {id}"),
            &container_logs(&name),
        );
        backend_host_ports.push(port);
    }

    // Reference implementations. The ingress-nginx container overrides
    // the entrypoint (static config, like «Envoy estático»), so its nginx
    // arguments come after the image name.
    let hap_name = format!("eqdiff-hap-{}", std::process::id());
    scope.run(
        &hap_name,
        HAPROXY_IMAGE,
        &[
            "--ip",
            "172.23.0.21",
            "-p",
            "127.0.0.1::8080",
            "-p",
            "127.0.0.1::8081",
            "-p",
            "127.0.0.1::8443",
            "-v",
            &format!(
                "{}:/usr/local/etc/haproxy/haproxy.cfg:ro",
                haproxy_cfg.display()
            ),
            "-v",
            &format!(
                "{}:/usr/local/etc/haproxy/tls.pem:ro",
                haproxy_tls.display()
            ),
            "-v",
            &format!(
                "{}:/usr/local/etc/haproxy/errors/503.http:ro",
                haproxy_error.display()
            ),
        ],
        &[],
    );
    let env_name = format!("eqdiff-env-{}", std::process::id());
    scope.run(
        &env_name,
        ENVOY_IMAGE,
        &[
            "--ip",
            "172.23.0.22",
            "-p",
            "127.0.0.1::8080",
            "-p",
            "127.0.0.1::8081",
            "-p",
            "127.0.0.1::8082",
            "-p",
            "127.0.0.1::8443",
            "-v",
            &format!("{}:/etc/envoy/envoy.yaml:ro", envoy_yaml.display()),
            "-v",
            &format!("{}:/tmp/diff-cert.pem:ro", tls_cert.display()),
            "-v",
            &format!("{}:/tmp/diff-key.pem:ro", tls_key.display()),
        ],
        &[],
    );
    let ing_name = format!("eqdiff-ing-{}", std::process::id());
    let ing_mount = format!("{}:/tmp/diff.conf:ro", nginx_conf.display());
    scope.run(
        &ing_name,
        INGRESS_NGINX_IMAGE,
        &[
            "--ip",
            "172.23.0.23",
            "-p",
            "127.0.0.1::8080",
            "-p",
            "127.0.0.1::8081",
            "-p",
            "127.0.0.1::8443",
            "-v",
            &ing_mount,
            "-v",
            &format!("{}:/tmp/diff-cert.pem:ro", tls_cert.display()),
            "-v",
            &format!("{}:/tmp/diff-key.pem:ro", tls_key.display()),
            "--entrypoint",
            "nginx",
        ],
        &["-c", "/tmp/diff.conf", "-g", "daemon off;"],
    );

    let hap_port = published_port(&hap_name, 8080);
    let env_port = published_port(&env_name, 8080);
    let ing_port = published_port(&ing_name, 8080);
    let hap2_port = published_port(&hap_name, 8081);
    let env2_port = published_port(&env_name, 8081);
    let env_cache_port = published_port(&env_name, 8082);
    let hap_tls_port = published_port(&hap_name, 8443);
    let env_tls_port = published_port(&env_name, 8443);
    let ing_tls_port = published_port(&ing_name, 8443);
    let ing2_port = published_port(&ing_name, 8081);
    wait_tcp(
        &format!("127.0.0.1:{hap_port}"),
        "haproxy",
        &container_logs(&hap_name),
    );
    wait_tcp(
        &format!("127.0.0.1:{env_port}"),
        "envoy",
        &container_logs(&env_name),
    );
    wait_tcp(
        &format!("127.0.0.1:{ing_port}"),
        "ingress-nginx",
        &container_logs(&ing_name),
    );
    wait_tcp(
        &format!("127.0.0.1:{env_cache_port}"),
        "envoy cache listener",
        &container_logs(&env_name),
    );
    for (port, label) in [
        (hap_tls_port, "haproxy TLS"),
        (env_tls_port, "envoy TLS"),
        (ing_tls_port, "ingress-nginx TLS"),
    ] {
        wait_tcp(
            &format!("127.0.0.1:{port}"),
            label,
            &container_logs(&hap_name),
        );
    }

    // EQProxy: the freshly built binary, natively.
    let eq_config = write(
        "eqproxy.toml",
        &eqproxy_toml(
            backend_host_ports[0],
            backend_host_ports[1],
            backend_host_ports[2],
            &tls_cert,
            &tls_key,
        ),
    );
    let eq_bin = env!("CARGO_BIN_EXE_eqproxy");
    let child = Command::new(eq_bin)
        .args(["run", "--config"])
        .arg(&eq_config)
        .args(["--metrics-interval", "1h"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn eqproxy run");
    let mut guard = ChildGuard(child);
    let mut stderr = BufReader::new(guard.0.stderr.take().expect("eqproxy stderr piped"));
    let mut main_addr = String::new();
    let mut replace_addr = String::new();
    let mut cache_addr = String::new();
    let mut secure_addr = String::new();
    let mut seen = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        let read = stderr.read_line(&mut line).expect("read eqproxy stderr");
        assert!(
            read > 0,
            "eqproxy closed stderr without announcing bind\n--- eqproxy stderr ---\n{seen}"
        );
        seen.push_str(&line);
        if let Some(addr) = line.split("listening on ").nth(1) {
            let addr = addr.trim().to_owned();
            if line.contains("listener web listening on") {
                main_addr = addr;
            } else if line.contains("listener replace listening on") {
                replace_addr = addr;
            } else if line.contains("listener cache listening on") {
                cache_addr = addr;
            } else if line.contains("listener secure listening on") {
                secure_addr = addr;
            }
            if !main_addr.is_empty()
                && !replace_addr.is_empty()
                && !cache_addr.is_empty()
                && !secure_addr.is_empty()
            {
                break;
            }
        }
    }
    assert!(
        !main_addr.is_empty()
            && !replace_addr.is_empty()
            && !cache_addr.is_empty()
            && !secure_addr.is_empty(),
        "eqproxy did not announce all four listeners\n--- eqproxy stderr ---\n{seen}"
    );

    // --- run the corpus against every implementation --------------------
    // Two endpoints per implementation: "main" (forwarded headers
    // append) and "replace" (anti-spoofing). The client address each one
    // sees is topology-dependent (host loopback vs docker gateway), so
    // it is calibrated at run time and substituted as `CLIENT`.
    let hap_addr = format!("127.0.0.1:{hap_port}");
    let ing_addr = format!("127.0.0.1:{ing_port}");
    let env_addr = format!("127.0.0.1:{env_port}");
    let hap2_addr = format!("127.0.0.1:{hap2_port}");
    let ing2_addr = format!("127.0.0.1:{ing2_port}");
    let env2_addr = format!("127.0.0.1:{env2_port}");
    let env_cache_addr = format!("127.0.0.1:{env_cache_port}");
    let hap_tls_addr = format!("127.0.0.1:{hap_tls_port}");
    let env_tls_addr = format!("127.0.0.1:{env_tls_port}");
    let ing_tls_addr = format!("127.0.0.1:{ing_tls_port}");
    let main_addrs: [(&str, &str); 4] = [
        ("eqproxy", main_addr.as_str()),
        ("haproxy", hap_addr.as_str()),
        ("ingress-nginx", ing_addr.as_str()),
        ("envoy", env_addr.as_str()),
    ];
    let replace_addrs: [(&str, &str); 4] = [
        ("eqproxy", replace_addr.as_str()),
        ("haproxy", hap2_addr.as_str()),
        ("ingress-nginx", ing2_addr.as_str()),
        ("envoy", env2_addr.as_str()),
    ];
    // Cache endpoints: EQProxy and Envoy use a dedicated listener so the
    // cache cannot serve a different case's response; HAProxy and nginx
    // scope their cache to /cache/ on the main endpoint.
    let cache_addrs: [(&str, &str); 4] = [
        ("eqproxy", cache_addr.as_str()),
        ("haproxy", hap_addr.as_str()),
        ("ingress-nginx", ing_addr.as_str()),
        ("envoy", env_cache_addr.as_str()),
    ];
    let tls_addrs: [(&str, &str); 4] = [
        ("eqproxy", secure_addr.as_str()),
        ("haproxy", hap_tls_addr.as_str()),
        ("ingress-nginx", ing_tls_addr.as_str()),
        ("envoy", env_tls_addr.as_str()),
    ];
    let client_ips: Vec<String> = main_addrs
        .iter()
        .map(|(_, addr)| probe_client_ip(addr))
        .collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut exclusions: Vec<String> = Vec::new();
    let mut agreed = 0usize;
    for case in CASES {
        let addrs: &[(&str, &str)] = match case.endpoint {
            "replace" => &replace_addrs,
            "tls" => &tls_addrs,
            _ => &main_addrs,
        };
        let mut row = vec![case.id.to_owned(), case.why.to_owned()];
        let mut case_ok = true;
        for (idx, (impl_name, addr)) in addrs.iter().enumerate() {
            if let Some((_, reason)) = case.skip.iter().find(|(i, _)| *i == *impl_name) {
                row.push(format!("SKIP: {reason}"));
                exclusions.push(format!("{} @ {impl_name}: {reason}", case.id));
                continue;
            }
            let got = if case.endpoint == "tls" {
                normalize(https_request(addr, case.request), &client_ips[idx])
            } else {
                normalize(http_request(addr, case.request), &client_ips[idx])
            };
            match check(case, &got) {
                Ok(()) => row.push(format!("OK {}", summary(&got, case.response_headers))),
                Err(why) => {
                    row.push(format!("DIVERGED {}", summary(&got, case.response_headers)));
                    case_ok = false;
                    failures.push(format!(
                        "{} @ {impl_name}: {why} (got {})",
                        case.id,
                        summary(&got, case.response_headers)
                    ));
                }
            }
        }
        if case_ok {
            agreed += 1;
        }
        rows.push(row);
    }

    // Cache cases: two requests per case. The backend answers with a fresh
    // UUID for every origin fetch, so identical bodies prove the second
    // response came from the cache without reaching the origin.
    let mut cache_rows: Vec<Vec<String>> = Vec::new();
    for case in CACHE_CASES {
        let mut row = vec![case.id.to_owned(), case.why.to_owned()];
        let mut case_ok = true;
        for (idx, (impl_name, addr)) in cache_addrs.iter().enumerate() {
            let first = normalize(http_request(addr, case.request), &client_ips[idx]);
            let second = normalize(http_request(addr, case.request), &client_ips[idx]);
            let origin_id = first.body.strip_prefix(case.body_prefix);
            let first_reached_origin = origin_id.is_some_and(|id| id.len() >= 32);
            let bodies_agree = second.body == first.body;
            if first.status == 200
                && second.status == 200
                && first_reached_origin
                && bodies_agree == case.expect_hit
            {
                let verdict = if case.expect_hit { "HIT" } else { "MISS" };
                // Both bodies are printed: the HIT/MISS verdict is enforced
                // above, and the evidence shows the UUIDs it was decided on.
                row.push(format!(
                    "OK {verdict} first={} second={}",
                    first.body, second.body
                ));
            } else {
                row.push(format!(
                    "DIVERGED first {} second {}",
                    summary(&first, &[]),
                    summary(&second, &[])
                ));
                case_ok = false;
                failures.push(format!(
                    "{} @ {impl_name}: first {} second {} (a fresh origin id and {} bodies are required)",
                    case.id,
                    summary(&first, &[]),
                    summary(&second, &[]),
                    if case.expect_hit { "equal" } else { "different" }
                ));
            }
        }
        if case_ok {
            agreed += 1;
        }
        cache_rows.push(row);
    }

    // --- report ---------------------------------------------------------
    let mut report = String::new();
    report.push_str("# Differential report — shared-semantics corpus\n\n");
    report.push_str(&format!(
        "- date (UTC epoch): {}\n",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_secs()
    ));
    report.push_str(&format!(
        "- eqproxy binary: {eq_bin} (sha256 {})\n",
        sha256(eq_bin)
    ));
    report.push_str(&format!(
        "- haproxy: {HAPROXY_IMAGE} ({})\n",
        image_id(HAPROXY_IMAGE)
    ));
    report.push_str(&format!(
        "- ingress-nginx data plane: {INGRESS_NGINX_IMAGE} ({})\n",
        image_id(INGRESS_NGINX_IMAGE)
    ));
    report.push_str(&format!(
        "- envoy: {ENVOY_IMAGE} ({})\n",
        image_id(ENVOY_IMAGE)
    ));
    report.push_str(&format!(
        "- backends: {BACKEND_IMAGE} ({})\n",
        image_id(BACKEND_IMAGE)
    ));
    report.push_str(
        "- normalization: status code only (reason phrase ignored); `Location` reduced to\n  path + query when absolute; body compared only when the case declares one;\n  `chunked` bodies decoded and `content-encoding: gzip` bodies gunzipped before\n  the comparison; an empty declared header value means the header must be\n  absent; comma-joined header values normalized to the `\", \"` separator; the\n  client address of each implementation is calibrated at run time and\n  substituted as `CLIENT`.\n\n",
    );
    report.push_str("| case | shared semantics | eqproxy | haproxy | ingress-nginx | envoy |\n");
    report.push_str("|---|---|---|---|---|---|\n");
    for row in &rows {
        report.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    report.push_str(
        "\nCache cases send the same request twice. The backend answers with a fresh\nUUID for every origin fetch, so an identical body on the second request proves\na HIT.\n\n",
    );
    report.push_str(
        "| cache case | shared semantics | eqproxy | haproxy | ingress-nginx | envoy |\n",
    );
    report.push_str("|---|---|---|---|---|---|\n");
    for row in &cache_rows {
        report.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    report.push_str(&format!(
        "\nverdict: {agreed}/{} cases agree across their applicable implementations ({} documented exclusion(s)).\n",
        CASES.len() + CACHE_CASES.len(),
        exclusions.len()
    ));
    if !failures.is_empty() {
        report.push_str("\n## Divergences\n\n");
        for failure in &failures {
            report.push_str(&format!("- {failure}\n"));
        }
    }
    if !exclusions.is_empty() {
        report.push_str("\n## Documented exclusions (not asserted there)\n\n");
        for exclusion in &exclusions {
            report.push_str(&format!("- {exclusion}\n"));
        }
    }

    let report_path = work.join("differential-report.md");
    std::fs::write(&report_path, &report).expect("write report");
    println!("{report}");
    println!("report written to {}", report_path.display());

    assert!(
        failures.is_empty(),
        "{} of {} cases diverged from the declared corpus:\n{}\nreport: {}",
        failures.len(),
        CASES.len(),
        failures.join("\n"),
        report_path.display()
    );
}
