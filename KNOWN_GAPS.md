# Known gaps

Language: **English**.

**What this page answers:** what the published v0.10.0 does not do, or does not
prove yet, so a trial on a staging config starts from an honest list. It is not
a roadmap; the reasons and the rejected alternatives are in the linked ADRs.
Last reviewed 2026-11-18 against the `release/v0.9.5` tree at `8468162`; the
published `v0.10.0` was cut from `bdf88bb` on 2026-11-17
([release record](docs/release-0.10.0.md)).

## Out of scope by decision

- **FastCGI, uwsgi and scgi** — not implemented
  ([ADR-0032](docs/adr/0032-fastcgi-uwsgi-scgi-decision.md)).
- **Cache `stale-while-revalidate` / `stale-if-error`** — the cache serves,
  revalidates and locks, but not the stale directives
  ([ADR-0034](docs/adr/0034-http-response-cache.md)).
- **Native service discovery** — no Consul/Eureka/xDS client; backends are
  configured, resolved by DNS and health-checked
  ([ADR-0044](docs/adr/0044-no-native-service-discovery.md)).
- **Kubernetes controller** — ingress-nginx objects are imported as files;
  EQProxy does not watch the API server.

## Implemented with narrower semantics

- `map` conditions support `equals` matching only.
- `error_pages` with `format = "raw"` keeps only the status, `Content-Type` and
  body of the file; the generated `Date` replaces any written in it.
- An origin that omits `Date` is relayed as-is: the proxy stamps only the
  responses it generates itself.
- The `100 Continue` interim response carries no header fields.

## Experimental or unproven

- **HTTP/3** is experimental: browsers and `curl` over h3 are unverified, and
  the QUIC adversarial tests of [THREAT_MODEL](THREAT_MODEL.md) T19 are not
  executed ([ADR-0013](docs/adr/0013-http3-quic.md)).
- **HTTP/3 is deprecated as a release-contract feature and retires in
  `0.10.8`.** It stays available behind the operator's explicit `quic = true`
  and is never announced (`Alt-Svc`) unless that flag turns it on; no test and
  no migration path depends on it being on.
- **The standalone `eqproxy_migrate.py` converter is deprecated and retires in
  `0.10.8`.** `eqproxy config import` replaces it; the script is
  maintainer-local (`ROADMAP EQPROXY/`, in `.git/info/exclude`) and is not part
  of any release artifact.
- **The published v0.10.0 carries one open defect.** The `config_import` fuzz
  target found an overflow in `convert_duration`
  (`eqproxy-config/src/import/mod.rs`): a hostile duration such as `1e400`
  saturates one part to `u64::MAX` and the next part wraps. A release build
  wraps the total — a wrong duration, not a crash — while any overflow-checked
  build aborts. The checked accumulation is on `main` (`513f6c9`) and on this
  branch (`f55c432`), not in the tagged artifact; the next patch carries it
  ([finding and evidence](docs/release-0.10.0.md#post-tag-finding-fuzz-overflow-in-convert_duration)).
- **No users outside the author yet.** v0.10.0 is not declared production
  ready; the Phase 9 gate is pending.
- **The v0.10.0 release is Linux-only:** no Darwin binary is published, so macOS
  users build from source. The macOS binaries verified locally during
  preparation were never shipped.

## Limits of the evidence

- The migration matrix is **13 declared cases × 5 implementations = 65/65**,
  not a proof for arbitrary configurations
  ([method and limits](docs/migration/README.md)).
- The differential corpus covers declared shared semantics; a green corpus is
  the absence of a counterexample, not equivalence
  ([corpus](docs/differential-corpus.md)).
- The published source tarball is not rebuilt in CI. The v0.9.4 tarball's
  SHA-256 was verified against the published `SHA256SUMS.txt`, but the file-list
  inspection belongs to the v0.9.1 cut
  ([release checklist §5.2](docs/release-0.9.1.md#52-not-verified-by-hand)).

## Report a gap

Open an issue with a sanitised config and the command that failed:
<https://github.com/EQProxy/EQProxy/issues>.
