# Known gaps

Language: **English**.

**What this page answers:** what v0.9.1 does not do, or does not prove yet, so a
trial on a staging config starts from an honest list. It is not a roadmap; the
reasons and the rejected alternatives are in the linked ADRs.

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
- **No users outside the author yet.** v0.9.1 is not declared production ready;
  the Phase 9 gate is pending.
- The macOS binaries are not static; only the Linux ones are.

## Limits of the evidence

- The migration matrix is **13 declared cases × 5 implementations = 65/65**,
  not a proof for arbitrary configurations
  ([method and limits](docs/migration/README.md)).
- The differential corpus covers declared shared semantics; a green corpus is
  the absence of a counterexample, not equivalence
  ([corpus](docs/differential-corpus.md)).
- The published source tarball is inspected and its file list checked, but it is
  not rebuilt in CI ([release checklist §5.2](docs/release-0.9.1.md#52-not-verified-by-hand)).

## Report a gap

Open an issue with a sanitised config and the command that failed:
<https://github.com/EQProxy/EQProxy/issues>.
