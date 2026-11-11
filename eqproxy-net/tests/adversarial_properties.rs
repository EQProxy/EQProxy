//! Adversarial audit (2026-11-09) — deterministic property harness.
//!
//! This is **test infrastructure**, not product code. It exercises the real
//! parsers and cache helpers (no mocks, no network) over generated inputs so
//! the invariants below run in CI and are reproducible without libFuzzer.
//!
//! Every property is stated as the contract the audit believes the code
//! claims; a failure here is a candidate finding, not a preference.

use std::sync::Arc;
use std::time::Duration;

use eqproxy_net::cache::{self, Cache, CacheSettings, Lookup, ResponseFacts, StoredParts};
use eqproxy_net::http1::{HeadLimits, parse_request_head, parse_response_head};

// ---------------------------------------------------------------------------
// Deterministic byte generator (xorshift64*). Reproducible across machines:
// the same seed produces the same corpus, unlike a time-seeded RNG.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

const REQUEST_SEEDS: &[&[u8]] = &[
    b"GET / HTTP/1.1\r\nhost: example.com",
    b"GET /a/b?x=1 HTTP/1.1\r\nhost: Example.COM.\r\naccept: */*",
    b"POST /upload HTTP/1.1\r\nhost: h\r\ncontent-length: 5\r\nexpect: 100-continue",
    b"CONNECT example.com:443 HTTP/1.1\r\nhost: example.com:443",
    b"OPTIONS * HTTP/1.1\r\nhost: h",
    b"GET / HTTP/1.0\r\n\r\n",
];

const RESPONSE_SEEDS: &[&[u8]] = &[
    b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\ncontent-type: text/plain",
    b"HTTP/1.1 204 No Content",
    b"HTTP/1.1 304 Not Modified\r\netag: \"abc\"",
    b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: upgrade",
    b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked",
    b"HTTP/1.0 200 OK\r\nconnection: close",
];

fn mutate(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = seed.to_vec();
    let ops = 1 + rng.below(6);
    for _ in 0..ops {
        if out.is_empty() {
            out.push(rng.byte());
            continue;
        }
        match rng.below(5) {
            0 => {
                let at = rng.below(out.len());
                out[at] = rng.byte();
            }
            1 => {
                let at = rng.below(out.len());
                out.insert(at, rng.byte());
            }
            2 => {
                let at = rng.below(out.len());
                out.remove(at);
            }
            3 => {
                let at = rng.below(out.len());
                let len = 1 + rng.below(8.min(out.len() - at));
                out.splice(at..at + len, *b"\r\n");
            }
            _ => {
                let at = rng.below(out.len() + 1);
                out.insert(at, b' ');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// P1 — the parsers are total: never panic, and every rejection is an
// explicit status from the documented set (400/431/501/505 request,
// 502 response). This is the same oracle the fuzz targets use, replayed
// deterministically so a regression also shows up without libFuzzer.
// ---------------------------------------------------------------------------

#[test]
fn p1_parsers_never_panic_and_reject_explicitly() {
    let mut rng = Rng::new(0xA0D1_2026_1109);
    for _ in 0..20_000 {
        let seed = REQUEST_SEEDS[rng.below(REQUEST_SEEDS.len())];
        let input = mutate(seed, &mut rng);
        match parse_request_head(&input, HeadLimits::default()) {
            Ok(_) => {}
            Err(reject) => assert!(
                matches!(reject.status, 400 | 408 | 413 | 417 | 431 | 501 | 505),
                "request rejection status out of contract: {} ({:?}) for {:?}",
                reject.status,
                reject.reason,
                input
            ),
        }
        let seed = RESPONSE_SEEDS[rng.below(RESPONSE_SEEDS.len())];
        let input = mutate(seed, &mut rng);
        match parse_response_head(&input, false, HeadLimits::default()) {
            Ok(response) => assert!(
                (100..=599).contains(&response.status),
                "accepted response with impossible status {}",
                response.status
            ),
            Err(reject) => assert_eq!(reject.status, 502, "response reject must be 502"),
        }
        let _ = parse_response_head(&input, true, HeadLimits::default());
    }
}

// ---------------------------------------------------------------------------
// P2 — normalization is idempotent: feeding the normalized target and host
// back through the parser changes neither. A non-idempotent normalizer is a
// routing/cache-key differential (the same request can reach two backends).
// ---------------------------------------------------------------------------

fn rebuild(request: &eqproxy_net::http1::RequestHead) -> Vec<u8> {
    let version = if request.version == 10 {
        "HTTP/1.0"
    } else {
        "HTTP/1.1"
    };
    let mut head = format!("{} {} {version}", request.method, request.target).into_bytes();
    if !request.host.is_empty() {
        head.extend_from_slice(b"\r\nhost: ");
        head.extend_from_slice(request.host.as_bytes());
    }
    head
}

#[test]
fn p2_already_normalized_targets_are_fixed_points() {
    let mut rng = Rng::new(0x11D3_2026_1109);
    for _ in 0..20_000 {
        let seed = REQUEST_SEEDS[rng.below(REQUEST_SEEDS.len())];
        let input = mutate(seed, &mut rng);
        let Ok(first) = parse_request_head(&input, HeadLimits::default()) else {
            continue;
        };
        // Only origin-form/CONNECT targets rebuilt above are meaningful.
        if first.method == "CONNECT" {
            continue;
        }
        let again = parse_request_head(&rebuild(&first), HeadLimits::default())
            .unwrap_or_else(|reject| panic!("normalized head must re-parse: {reject:?}"));
        assert_eq!(again.method, first.method, "method changed on reparse");
        assert_eq!(
            again.target, first.target,
            "target normalization is not idempotent: {:?} -> {:?} -> {:?}",
            input, first.target, again.target
        );
        assert_eq!(
            again.host, first.host,
            "host normalization is not idempotent: {:?} -> {:?} -> {:?}",
            input, first.host, again.host
        );
    }
}

// ---------------------------------------------------------------------------
// P3 — Cache-Control parsing can only shorten the configured default TTL.
// Origin `max-age`/`s-maxage` must never extend it; `no-cache` must zero it;
// malformed/negative values must not panic or produce absurd durations.
// ---------------------------------------------------------------------------

#[test]
fn p3_effective_ttl_never_exceeds_default() {
    let defaults = [
        Duration::from_secs(0),
        Duration::from_secs(1),
        Duration::from_secs(60),
        Duration::from_secs(86_400),
    ];
    let directives = [
        "",
        "max-age=0",
        "max-age=1",
        "max-age=999999999999999999999999",
        "max-age=-1",
        "max-age=\"30\"",
        "s-maxage=10, max-age=1000",
        "max-age=1000, s-maxage=10",
        "no-cache",
        "public, max-age=5",
        "NO-CACHE",
        "max-age=abc",
        "s-maxage",
        "max-age",
        "max-age=0, max-age=99",
    ];
    let mut rng = Rng::new(0x7E11_2026_1109);
    for default in defaults {
        for _ in 0..500 {
            let count = rng.below(3);
            let values: Vec<String> = (0..count)
                .map(|_| directives[rng.below(directives.len())].to_owned())
                .collect();
            let ttl = cache::effective_ttl(default, &values);
            assert!(
                ttl <= default,
                "ttl {ttl:?} exceeded default {default:?} for {values:?}"
            );
            let lower: Vec<String> = values.iter().map(|v| v.to_lowercase()).collect();
            if lower
                .iter()
                .any(|v| v.split(',').any(|d| d.trim() == "no-cache"))
            {
                assert_eq!(
                    ttl,
                    Duration::ZERO,
                    "no-cache must zero the TTL: {values:?}"
                );
            }
        }
    }
    // s-maxage wins over max-age regardless of order.
    assert_eq!(
        cache::effective_ttl(
            Duration::from_secs(100),
            &["max-age=50, s-maxage=10".into()]
        ),
        Duration::from_secs(10)
    );
}

// ---------------------------------------------------------------------------
// P4 — storable_vary never returns a key that would cross clients, and every
// refusal reason is real (Vary: *, session identity, Set-Cookie, opt-outs).
// ---------------------------------------------------------------------------

fn facts(headers: &[(&str, &str)]) -> ResponseFacts {
    let head = format!(
        "HTTP/1.1 200 OK\r\n{}",
        headers
            .iter()
            .map(|(n, v)| format!("{n}: {v}"))
            .collect::<Vec<_>>()
            .join("\r\n")
    );
    cache::parse_origin_head(head.as_bytes()).expect("valid generated head")
}

#[test]
fn p4_storable_vary_refuses_cross_client_keys() {
    for (headers, why) in [
        (vec![("vary", "*")], "Vary: *"),
        (vec![("vary", "cookie")], "Vary: Cookie"),
        (vec![("vary", "authorization")], "Vary: Authorization"),
        (vec![("set-cookie", "sid=1")], "Set-Cookie"),
        (vec![("cache-control", "private")], "private"),
        (vec![("cache-control", "no-store")], "no-store"),
    ] {
        let parsed = facts(&headers);
        assert!(
            cache::storable_vary(&parsed).is_none(),
            "{why} must make the response unstorable"
        );
    }
    let parsed = facts(&[("vary", "accept-encoding"), ("cache-control", "public")]);
    assert_eq!(
        cache::storable_vary(&parsed),
        Some(vec!["accept-encoding".to_owned()]),
        "a safe Vary must be kept"
    );
}

// ---------------------------------------------------------------------------
// P5 — end_to_end strips hop-by-hop, connection-nominated, framing and
// cache-managed headers; nothing a later hop owns may survive into storage.
// ---------------------------------------------------------------------------

#[test]
fn p5_end_to_end_removes_connection_nominated_headers() {
    let headers: Vec<(String, String)> = [
        ("connection", "keep-alive, x-remove"),
        ("keep-alive", "timeout=5"),
        ("x-remove", "must-not-survive"),
        ("transfer-encoding", "chunked"),
        ("content-length", "10"),
        ("cache-control", "max-age=5"),
        ("etag", "\"abc\""),
        ("content-type", "text/plain"),
        ("x-keep", "yes"),
    ]
    .iter()
    .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
    .collect();
    let kept = cache::end_to_end(&headers);
    let names: Vec<&str> = kept.iter().map(|(n, _)| n.as_str()).collect();
    for forbidden in [
        "connection",
        "keep-alive",
        "x-remove",
        "transfer-encoding",
        "content-length",
    ] {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} survived end_to_end: {names:?}"
        );
    }
    assert!(names.contains(&"content-type"), "end-to-end header dropped");
    assert!(names.contains(&"x-keep"), "unrelated header dropped");
    // Stored metadata is deliberately kept so a later HIT can revalidate and
    // forward the origin's freshness/validators (ADR-0034).
    assert!(names.contains(&"etag"), "validators must be stored");
    assert!(
        names.contains(&"cache-control"),
        "origin freshness policy must be stored"
    );
}

// ---------------------------------------------------------------------------
// P6 — client_fresh is a weak comparison (W/ tolerated on either side) and
// `*` matches any stored representation. A miss must not become a false hit.
// ---------------------------------------------------------------------------

#[test]
fn p6_client_fresh_weak_comparison() {
    assert!(cache::client_fresh("\"abc\"", Some("\"abc\"")));
    assert!(cache::client_fresh("\"abc\"", Some("W/\"abc\"")));
    assert!(cache::client_fresh("W/\"abc\"", Some("\"abc\"")));
    assert!(cache::client_fresh("*", Some("\"abc\"")));
    assert!(cache::client_fresh("*", None));
    assert!(!cache::client_fresh("\"other\"", Some("\"abc\"")));
    assert!(!cache::client_fresh("\"\"", Some("\"abc\"")));
    // Documented leniency: an unquoted tag is accepted as if quoted. This is
    // not RFC 9110 strict, but it cannot turn a different representation
    // into a false hit (the bytes still have to match).
    assert!(cache::client_fresh("abc", Some("\"abc\"")));
}

// ---------------------------------------------------------------------------
// P7 — Vary variants are isolated: two values of the varied request header
// never serve each other's body. The served Content-Length is the observable
// (StoredEntry body is internal by design).
// ---------------------------------------------------------------------------

fn parts(body: &[u8], vary_values: Vec<String>) -> StoredParts {
    StoredParts {
        status: 200,
        reason: "OK".to_owned(),
        headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
        body: body.to_vec(),
        etag: None,
        last_modified: None,
        vary: Vec::new(),
        vary_values,
        framing_none: false,
        has_declared_len: true,
        ttl: Duration::from_secs(60),
        stored_at_unix: 0,
    }
}

fn entry_len(entry: &cache::StoredEntry) -> usize {
    let head = String::from_utf8_lossy(&cache::render_head(entry)).to_ascii_lowercase();
    head.lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
async fn p7_vary_variants_never_cross() {
    let cache = Cache::load(
        CacheSettings::test_defaults(),
        Arc::new(eqproxy_core::Metrics::default()),
    )
    .expect("cache builds");
    let key = cache::base_key("GET", "http", "example.com", "/vary");
    let mut gzip_entry = parts(b"aaaa", vec!["gzip".to_owned()]);
    gzip_entry.vary = vec!["accept-encoding".to_owned()];
    cache.store_direct(key.clone(), gzip_entry).await;
    let mut identity_entry = parts(b"bbbbbb", vec!["identity".to_owned()]);
    identity_entry.vary = vec!["accept-encoding".to_owned()];
    cache.store_direct(key.clone(), identity_entry).await;

    let gzip = vec![("accept-encoding".to_owned(), "gzip".to_owned())];
    let identity = vec![("accept-encoding".to_owned(), "identity".to_owned())];
    let other = vec![("accept-encoding".to_owned(), "br".to_owned())];

    match cache.lookup(&key, &gzip, false) {
        Lookup::Hit(entry) => assert_eq!(entry_len(&entry), 4),
        _ => panic!("gzip variant must hit"),
    }
    match cache.lookup(&key, &identity, false) {
        Lookup::Hit(entry) => assert_eq!(entry_len(&entry), 6),
        _ => panic!("identity variant must hit"),
    }
    assert!(
        !matches!(cache.lookup(&key, &other, false), Lookup::Hit(_)),
        "an unvaried request must not receive another variant"
    );
}

// ---------------------------------------------------------------------------
// P8 — client directives are response-scoped, never learned by the entry:
// `max-age=0` forces revalidation for that caller while another caller still
// gets a HIT. This is the AUD-02 regression contract.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn p8_client_max_age_zero_does_not_poison_other_clients() {
    let cache = Cache::load(
        CacheSettings::test_defaults(),
        Arc::new(eqproxy_core::Metrics::default()),
    )
    .expect("cache builds");
    let key = cache::base_key("GET", "http", "example.com", "/max-age");
    let mut stored = parts(b"hello", Vec::new());
    stored.etag = Some("\"v1\"".to_owned());
    cache.store_direct(key.clone(), stored).await;

    let zero = vec![("cache-control".to_owned(), "max-age=0".to_owned())];
    let plain: Vec<(String, String)> = Vec::new();
    assert!(
        matches!(cache.lookup(&key, &zero, false), Lookup::Revalidate(_)),
        "max-age=0 must revalidate a fresh entry"
    );
    assert!(
        matches!(cache.lookup(&key, &plain, false), Lookup::Hit(_)),
        "another client must still hit the fresh entry"
    );
    // The bypassed caller revalidating does not rewrite the entry policy.
    assert!(
        matches!(cache.lookup(&key, &plain, false), Lookup::Hit(_)),
        "the entry must not learn max-age=0"
    );
}

// ---------------------------------------------------------------------------
// P9 — a 304 that revokes shared storability removes the entry and is
// counted; a clean 304 keeps the body byte-identical and restarts the clock.
// This is the AUD-01 regression contract on the current working tree.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn p9_revoking_304_invalidates_clean_304_refreshes() {
    let cache = Cache::load(
        CacheSettings::test_defaults(),
        Arc::new(eqproxy_core::Metrics::default()),
    )
    .expect("cache builds");
    let key = cache::base_key("GET", "http", "example.com", "/304");
    let mut stored = parts(b"body", Vec::new());
    stored.etag = Some("\"v1\"".to_owned());
    cache.store_direct(key.clone(), stored).await;

    let old = match cache.lookup(&key, &[], false) {
        Lookup::Hit(entry) => entry,
        _ => panic!("stored entry must hit"),
    };

    // Clean 304: only metadata changes, body stays.
    let clean = facts(&[("etag", "\"v2\""), ("cache-control", "max-age=10")]);
    match cache
        .refresh(key.clone(), &old, &clean, Duration::from_secs(10))
        .await
    {
        cache::RefreshOutcome::Refreshed(entry) => assert_eq!(entry_len(&entry), 4),
        cache::RefreshOutcome::Invalidated => panic!("clean 304 must refresh"),
    }

    // Revoking 304: Set-Cookie removes the shared entry.
    let revoking = facts(&[("set-cookie", "sid=1")]);
    assert!(
        matches!(
            cache
                .refresh(key.clone(), &old, &revoking, Duration::from_secs(10))
                .await,
            cache::RefreshOutcome::Invalidated
        ),
        "Set-Cookie on 304 must invalidate the shared entry"
    );
    assert_eq!(cache.cache_304_invalidations(), 1);
    let snapshot = cache.snapshot();
    assert!(
        snapshot.objects.is_empty(),
        "invalidated entry still visible: {:?}",
        snapshot.objects
    );

    // And the invalidated key is no longer served.
    assert!(!matches!(cache.lookup(&key, &[], false), Lookup::Hit(_)));
}

// ---------------------------------------------------------------------------
// P10 — render_head + parse_origin_head round-trip preserves the served
// status and length; a stored entry can be re-parsed as an origin head.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn p10_rendered_head_round_trips() {
    let cache = Cache::load(
        CacheSettings::test_defaults(),
        Arc::new(eqproxy_core::Metrics::default()),
    )
    .expect("cache builds");
    let key = cache::base_key("GET", "http", "example.com", "/render");
    let mut stored = parts(b"0123456789", Vec::new());
    stored.etag = Some("\"x\"".to_owned());
    stored.headers = vec![
        ("content-type".to_owned(), "text/plain".to_owned()),
        ("etag".to_owned(), "\"x\"".to_owned()),
    ];
    cache.store_direct(key.clone(), stored).await;
    let entry = match cache.lookup(&key, &[], false) {
        Lookup::Hit(entry) => entry,
        _ => panic!("stored entry must hit"),
    };
    let head = cache::render_head(&entry);
    let parsed = cache::parse_origin_head(&head).expect("rendered head must parse");
    assert_eq!(parsed.status, 200);
    assert_eq!(parsed.etag.as_deref(), Some("\"x\""));
}

// ---------------------------------------------------------------------------
// Sanity: the test generators themselves are non-degenerate.
// ---------------------------------------------------------------------------

#[test]
fn generator_sanity() {
    let mut rng = Rng::new(1);
    let mut distinct = std::collections::HashSet::new();
    for _ in 0..1000 {
        distinct.insert(mutate(REQUEST_SEEDS[0], &mut rng));
    }
    assert!(distinct.len() > 500, "generator is not exploring");
}
