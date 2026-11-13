# Migration gate evidence — `migration_matrix_same_routing_as_the_originals`

Reproduction of the v0.6 gate criterion «migración demostrada»: the four real
source configs of [`docs/migration/`](../../migration/README.md) and their EQProxy
translation route the matrix identically.

## Command

```bash
cargo test -p eqproxy-cli --test migration -- --ignored --nocapture
```

- Date (UTC): 2026-11-13T16:12Z (report epoch 1794586357), on the working
  tree that carries the v0.9.6 release slice; the v0.6 gate run (13/13) was
  2026-10-20T13:14Z, epoch 1792502050.
- Environment: Darwin 25.5.0 (macOS 26.5.1), arm64, Docker 29.9.3,
  rustc 1.99.1 (58cf8f9c3 2026-10-13), cargo 1.99.1 (5f94df478 2026-08-27).
- Binary under test: `target/debug/eqproxy`,
  sha256 `364127c7f130fa03fc22c0a709c0bdfc64291667198d7913eb0a7fab574b8d24`
  (the v0.6 run used `31ef7e1…b0ed3`).
- Originals (container digests in the report): `haproxy:3.4.6`,
  `nginx:alpine`, `registry.k8s.io/ingress-nginx/controller:v1.13.0`,
  `envoyproxy/envoy:v1.34.2`; backends and the m14 HTTPS-only origin
  `nginx:alpine`.

## Result

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.69s
verdict: 14/14 cases route identically across the five implementations (70 checks).
```

Case m14 (`GET /tls/x` on `shop.example.com` → `200 tls-origin /tls/x`) is the
TLS-to-the-origin case of v0.9.6: the origin answers only on `8443` with
`listen 8443 ssl`, so a proxy that downgrades the scheme receives
`400 The plain HTTP request was sent to HTTPS port` and the case fails. Proven
two-sided: removing `[backend.tls_origin.tls]` from the translation makes
EQProxy the only implementation to fail (400 from the origin, the other four
still 200), and a v1 certificate makes it fail with 502
(`UnsupportedCertVersion` from rustls), which is why the harness issues a v3
certificate with a SAN.

The raw harness output is [report.md](report.md) (written unedited by the test;
absolute paths point at the runner's checkout and build tree).

## What this proves, and what it does not

- Proves: for the 14 matrix cases, HAProxy, operator nginx, the ingress-nginx
  data plane (controller-shaped generated config) and Envoy answer exactly what
  EQProxy answers with [eqproxy.toml](../../migration/eqproxy.toml) — asserted
  on status, `Location` (reduced to path + query) and body (which echoes the
  pool and the received path, so pool choice and rewrites are both checked).
- Declared substitutions: the EQProxy translation runs with a free local listen
  address and reaches the five pools through their published host ports; the
  containers use the fixed demo addresses. Nothing else differs.
- Does not prove: the ingress-nginx **controller** object → config translation
  (the generated shapes are what runs), TLS on the listening side, SNI, ALPN,
  WebSocket upgrades, HTTP/2, and multi-server balancing per pool. TLS *to the
  origin* is covered by m14 only (verification off, no SNI). Those keep their own slices (see the
  limitations in [docs/migration/README.md](../../migration/README.md)).
