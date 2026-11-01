# v0.9.1-a — scope decisions before code (E3)

**Objective.** v0.9.1 opens with the boxes that needed a verdict, not an
implementation: generic UDP, FastCGI/AJP, transparent proxying, WAF, Lua,
snippets, service discovery and the Unix-socket/source-bind half of B-H14.
This increment records eight public ADRs, closes seven of the nineteen v0.9.1
boxes by decision, and sketches the equivalence matrix. **No product code
changed.**

## 1. Decisions

| Box (IDs) | Decision | ADR | Alternative recorded |
|---|---|---|---|
| UDP (H7 · B-I18 · B-E13) | **E3** | [ADR-0037](../adr/0037-udp-proxying-not-adopted.md) | UDP hop on HAProxy/dedicated balancer; TCP syslog (RFC 6587) |
| AJP (B-I12 · B-N17) | **E3** | [ADR-0038](../adr/0038-ajp-gateway-not-adopted.md) (extends [ADR-0032](../adr/0032-fastcgi-uwsgi-scgi-decision.md) for FastCGI/uwsgi/scgi) | Tomcat HTTP connector / an AJP front end |
| TPROXY · `original_dst` (B-E14) | **E3** | [ADR-0039](../adr/0039-transparent-proxying-not-adopted.md) | PROXY protocol v1/v2 (shipped); transparent front end |
| WAF · ModSecurity · CRS (B-I13) | **E3** | [ADR-0040](../adr/0040-waf-not-adopted.md) | external WAF; narrow WASM plugin |
| Lua (B-E20) | **E3** | [ADR-0041](../adr/0041-lua-not-adopted.md) | WASM plugin ABI v1 (ADR-0031) |
| nginx snippets (B-I5) | **E3** + mapping table | [ADR-0042](../adr/0042-snippets-not-adopted.md) | native rules; WASM for the rest |
| Unix sockets · source bind (B-H14 · B-N15) | **adopted** (code in 0.9.1-b) | [ADR-0043](../adr/0043-unix-sockets-and-source-bind.md) | — |
| Kubernetes/Consul watch (B-I3) | **E3** | [ADR-0044](../adr/0044-no-native-service-discovery.md) | DNS + `resolve_interval`; admin API writes |

## 2. What was verified before deciding

| Claim | Check | Result |
|---|---|---|
| No generic UDP path | `grep -rniE "udpsocket\|udp" eqproxy-net/src eqproxy-config/src/schema.rs` | only the QUIC/H3 endpoint (`http3.rs`, `proxy.rs`); no UDP listener or relay (ADR-0013) |
| No AJP | `grep -rniE "ajp" eqproxy-config/src/import` | no implementation; `backend-protocol` values outside HTTP/HTTPS/GRPC fall to `gap` in `import::ingress` |
| No transparent proxying | `grep -rniE "ip_transparent\|original_dst\|tproxy" eqproxy-*/src` | only the importer's "TPROXY: sin soporte" message |
| No Lua/WAF dependency | manifests and lockfile | no `mlua`, no ModSecurity, no CRS loader |
| Snippets are refused, not executed | `import::ingress` classification | `review` with "snippet de nginx: no se ejecuta ni se traduce (sustituto: plugin WASM o reglas)" |
| Backends are IP/name only | `ServerTarget` in `eqproxy-config/src/schema.rs` | `Ip(SocketAddr) \| Name { host, port }`; the Unix socket in the product is the admin plane's (ADR-0021) |
| No Kubernetes/Consul client | `grep -ci "kube\|consul" Cargo.lock` | `0` |
| Container is unprivileged | `docker/Dockerfile:79` | `USER 65532:65532` (ADR-0005/0016) |

## 3. Roadmap and matrix

- Roadmap 0.9.1 (local-only: `ROADMAP EQPROXY/ROADMAP/Roadmap 0.9.1.md`): **7/19**
  boxes checked, each marked `[x]` only because its ADR is the acceptance
  criterion for an E3 decision.
- Matriz de equivalencia 0.9.1 (local-only: `ROADMAP EQPROXY/ROADMAP/Matriz de equivalencia 0.9.1.md`):
  working sketch (local-only) mapping all nineteen boxes and the decision
  status; the public, complete matrix remains the v0.9.1 gate.
- Index ROADMAP/README.md (local-only: `ROADMAP EQPROXY/ROADMAP/README.md`):
  96 + 7 = **103/115** boxes.

A script counted the boxes in the six roadmap files: 17 + 32 + 11 + 14 + 22 +
7 = 103 done of 17 + 32 + 11 + 14 + 22 + 19 = 115. A relative-link check over
`ROADMAP EQPROXY/ROADMAP/*.md` and the eight new ADRs reports **0 missing**.

## 4. Audits of this increment

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `veyra audit . --profile standard` | **NOT EXECUTED for this increment** — it adds only Markdown and changes no Rust, manifest or lockfile; the last full pass over this code tree is ACCEPTED, exit 0 | `.veyra/evidence/20261031T215715Z/report.md` (previous increment; the delta since is one `println!` literal plus documentation) |
| Rust Audit | `python3.13 <rust-ai-audit>/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | **exit 1**; 0 critical / 7 high / 397 medium / 107 low — the same seven locations, all triaged as false positives (3 R16, 4 R09 in the fuzz target) | `report/rust-audit-20261031-232523.md` (local-only) + [v085-rust-audit-triage](v085-rust-audit-triage.md) |
| CodeGraph Rust | — | **NOT EXECUTED for this increment** — no code structure changed; the last successful index (80 files, 8 438 nodes, 14 876 edges) predates the `main.rs` literal and the later re-index hit the 600 s limit | `/tmp/cg-docs-truth.json` |
| Hindsight | `hindsight-note --title ... --project EQProxy` | **NOT EXECUTED** — no note was written | — |

## 5. Limits

- These are decisions, not behaviour. The E3 features remain absent from the
  product; the matrix's E3 cells say "excluded by ADR", never "equivalent".
- The matrix is a sketch: the full E1/E2/E3 classification of every reference
  feature, the benchmark plan, the differential corpus, the external review
  and the production verdict are still the v0.9.1 gate.
- `0.9.1-b` is next: Unix backend sockets and source bind (ADR-0043), custom
  error pages (B-H15/B-E18/B-N14), TLS leftovers (B-I16/B-N16/B-H16) and the
  B-E9 close-out.
