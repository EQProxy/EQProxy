# v0.9.2 — cache family in the differential corpus

**Objective.** Extend the shared-semantics corpus (roadmap 0.9.2, casilla 17)
with the response cache and prove the declared outcome against the three real
references. The cases observe **whether a response is reused**, not the cache
implementation, its key layout or its eviction policy.

## Change

| Area | Change |
|---|---|
| harness | a second table of cases sends the same request twice and decides HIT/MISS on the origin UUID: identical bodies on the second request = HIT, different = MISS. A first response whose body does not carry a 32-character UUID is a failure, so a broken backend cannot masquerade as a cache hit. |
| corpus | two cases: `k1-miss-then-hit` and `k2-no-store-bypass` (`docs/differential-corpus.md`). |
| backends | `/cache/` answers `cache <id> <uuid>` with `Cache-Control: public, max-age=60`; `/cache/nostore` answers `cache-nostore <id> <uuid>` with `Cache-Control: no-store`. The UUID is nginx `$request_id`, fresh on every origin fetch. |
| HAProxy | `cache diffcache` (8 MB, 64 KiB objects, `max-age 60`) + `http-request cache-use diffcache if { path_beg /cache/ }` / `http-response cache-store diffcache`. |
| ingress-nginx | `proxy_cache_path … keys_zone=diffcache:10m` + `proxy_cache diffcache` / `proxy_cache_valid 200 60s` on the `/cache/` locations. |
| Envoy | `envoy.filters.http.cache` (`CacheConfig` + `SimpleHttpCacheConfig`) on a dedicated listener at `:8082`, routed to the real `api` cluster. |
| EQProxy | `[listener.cache.cache]` with `default_ttl = "60s"` on a dedicated listener; its rule sends `/cache/` to the `api` backend. |

## Decisions

1. **Identical UUID, not an `X-Cache` header.** None of the four
   implementations emits a common cache-status header; the shared
   observable is whether the origin was contacted again. A fresh UUID per
   fetch makes that visible in the body. `k1` requires equality, `k2`
   requires difference.
2. **`k2` is the control.** A corpus with only `k1` would show that some
   cache stores something, not that the origin's `no-store` opt-out is
   respected. It also proves the UUID is fresh — its two responses differ on
   every column — so `k1`'s identical pair cannot be explained by a constant
   backend. Both cases must pass on every column for the family to count.
3. **Dedicated cache endpoints.** EQProxy and Envoy run the cache on their
   own listener; HAProxy and nginx scope the cache to `/cache/`. The corpus
   sends other cases through the same proxies, and a stored response left
   reachable from the main endpoint could answer a later request and mask a
   routing divergence. Isolation is part of the harness, not a per-case
   exclusion.
4. **A real backend, not a local response.** A probe cached an Envoy
   `direct_response`: both requests answered `404` instead of the configured
   `200`, and Envoy logged an `envoy_bug` assertion
   ([raw probe](v092-differential-cache/raw/envoy-cache-direct-response/probe.txt)).
   The case therefore caches a proxied response, which is the semantics
   under test. That Envoy filter interaction is declared out of scope, not
   hidden.
5. **No performance claim.** This increment proves store/hit and opt-out
   behavior, not hit ratio, latency or throughput.

## Execution

Command, run from the repository root on 2026-11-02:

```bash
cargo test -p eqproxy-cli --test differential -- --ignored --nocapture
```

Result: **exit 0**, `1 passed; 0 failed`, 16.58 s. Report:
[raw/differential-report.md](v092-differential-cache/raw/differential-report.md)
(SHA-256 `c6ce86cf846452a6ba8db0c0d92bd4618f12638b09b63d4993bf4a0c880cc11c`).
Full command output:
[raw/cargo-test.txt](v092-differential-cache/raw/cargo-test.txt).
Environment:
[raw/00-environment.txt](v092-differential-cache/raw/00-environment.txt).

| Item | Value |
|---|---|
| host | Darwin arm64, macOS 26.5.1 (25F80) |
| toolchain | `rustc 1.99.1` / `cargo 1.99.1` |
| Docker | client/server 29.9.2 |
| source | `main` at `6ba2667`; worktree at capture time: `eqproxy-cli/tests/differential.rs` and `docs/differential-corpus.md` modified (compression increment), cache evidence untracked. The cache rows in the corpus doc and this page were written after the run. |
| EQProxy binary | `target/debug/eqproxy` **at test time**, SHA-256 `281459e5cbd07841e573ee775c53cf9ac8abf1c84eb72348e24fdeeb85cdb790` (recorded in the report). The Veyra run rebuilt the debug binary afterwards (current hash `d61cf4e1…`), so the row identifies the artifact that executed, not the current file. |
| HAProxy | `haproxy:3.4.6`, `sha256:e273915691a8eab3f105bf3e39fd4c29a4ecf334e33421903b5589d25e14db26` |
| ingress-nginx data plane | `registry.k8s.io/ingress-nginx/controller:v1.13.0`, `sha256:dc75a7baec7a3b827a5d7ab0acd10ab507904c7dad692365b3e3b596eca1afd2` |
| Envoy | `envoyproxy/envoy:v1.34.2`, `sha256:daca6a3f353ba289cc786d2162d13d4ec2b16d921c6c3f2fc57ce6f7900ab3d9` |
| backends | `nginx:alpine`, `sha256:a3554c0d0b48177b1413c26e01f695334428020d9df01243fbae3e11cfd17417` |

The harness is `differential_shared_semantics_corpus`; it runs the declared
corpus unchanged against the freshly built binary (native) and the three
reference containers, and writes the per-case report above.

## Observed results

At the time of this cache-family run the corpus was **28/28, 0 divergences,
0 exclusions**. The later full-corpus run (2026-11-04, after `z1`–`z3`, `e1`,
`t1` and `l1`) is **31/31**, with one documented HAProxy exclusion; see
[`docs/differential-corpus.md`](../differential-corpus.md). The two cache rows,
each sent twice per implementation (EQProxy, HAProxy, ingress-nginx, Envoy):

| case | request ×2 | asserted observable | eqproxy | haproxy | ingress-nginx | envoy |
|---|---|---|---|---|---|---|
| `k1-miss-then-hit` | `GET /cache/k1` | same origin UUID in both bodies | `HIT` `c58024c5…` | `HIT` `ae424e9b…` | `HIT` `9ea6812c…` | `HIT` `6235eb18…` |
| `k2-no-store-bypass` | `GET /cache/nostore` | different origin UUIDs | `MISS` `3991ffc7…` / `f7c94a4f…` | `MISS` `437e5e32…` / `73520e58…` | `MISS` `f24beafe…` / `8433065d…` | `MISS` `14adcab8…` / `c191ea14…` |

The full UUIDs and both responses of each case are printed in the raw report.
The first request of `k2` is also a `200` carrying a valid UUID, so its MISS is
a reached origin, not a failed request. `k1`'s UUID differs per
implementation, as expected: they are separate origins.

## Limits

- **Casilla 17 remains open.** This increment covers cache only; TLS, error
  pages and limits are still pending, and the matrix is not reclassified by
  this page.
- **No release artifact or image was exercised.** The comparison uses the
  debug-profile binary from `CARGO_BIN_EXE_eqproxy` and reference containers;
  the release build and the published image are separate gates.
- **Two GET shapes only.** No `Vary` variants, `ETag`/`304` revalidation,
  stale-while-revalidate, `HEAD`, byte ranges, eviction under pressure, disk
  persistence or concurrent request coalescing.
- **HTTP/1.1 plaintext only.** No TLS, h2 or h3 in this family.
- **One host architecture.** Darwin arm64; the same run on Linux/amd64 is not
  claimed here.
- **ingress-nginx is a data plane.** The controller translation is not
  exercised, by the same decision that covers static Envoy.
