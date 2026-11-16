# ADR-0051: One listener per address, SNI certificates and `host` rules for vhosts

Language: **English**.

- State: **Accepted** (2026-11-14, roadmap v0.9.6, nginx multi-vhost importer)
- Date: 2026-11-14
- Related: [`eqproxy-config/src/schema.rs`](../../eqproxy-config/src/schema.rs),
  [`eqproxy-config/src/validate.rs`](../../eqproxy-config/src/validate.rs),
  [`eqproxy-config/src/import/nginx.rs`](../../eqproxy-config/src/import/nginx.rs),
  [`eqproxy-net/src/tls.rs`](../../eqproxy-net/src/tls.rs),
  [`eqproxy-net/src/proxy.rs`](../../eqproxy-net/src/proxy.rs),
  [`eqproxy-core/src/rules.rs`](../../eqproxy-core/src/rules.rs),
  [ADR-0025](0025-certificate-renewal-without-restart.md) (certificate resolver),
  [ADR-0035](0035-hot-secrets-and-drift-runbook.md) (no operator cipher suites),
  [ADR-0046](0046-tls-version-range-and-minor-gaps.md) (TLS version range)

## Context

nginx, HAProxy and Envoy let several virtual servers share one `IP:port`. The
client's TLS SNI selects the certificate during the handshake and the HTTP
`Host` / `:authority` selects the site after it. EQProxy's schema started with
one certificate and one rule set per listener, so the nginx importer translated
one `server` block and reported the rest as a `Gap`; the schema now has
`[listener.*.tls].certs[]` and `[[listener.*.rules]].host`, and the converter
uses both.

That leaves a representation question that a converter cannot answer on its own:
is a vhost a listener, a certificate entry, a rule, or all three? Without a fixed
answer, two converters can emit different topologies for the same source, and a
config with two listeners on one address would fail only at bind time. This ADR
fixes the representation before more sources depend on it.

## Decision

### A listener is a socket; a vhost is not

There is exactly **one `[listener.<name>]` per socket address**. The listener
owns the accept loop and every setting that cannot vary per name on one wire
endpoint: `listen`, `mode`, the default `backend`, TLS policy (`cert`, `key`,
`certs`, `min_version`, `max_version`, `client_ca`, `alpn`), `quic`, timeouts,
`maxconn`, compression, cache and error pages.

Two listeners on the same `SocketAddr` are a configuration error, not a merge:
`check_listener_addresses` (`eqproxy-config/src/validate.rs`) rejects them and
names both tables. A wildcard address (`0.0.0.0` or `[::]`) and a concrete
address on the same port overlap too: the wildcard claims the concrete
listener's traffic even where `SO_REUSEADDR` lets both binds succeed. `[::]`
covers IPv4 as well because EQProxy leaves `IPV6_V6ONLY` at the OS default
(dual-stack on Linux and macOS). Both cases are rejected with the field of the
later listener. Port `0` is exempt because the kernel assigns a different port
per listener, so ephemeral test listeners do not collide.

A vhost is represented by two things inside that listener:

1. **Its certificate**, as a `[[listener.<name>.tls.certs]]` entry with `cert`,
   `key` and `names`.
2. **Its routing**, as one or more `[[listener.<name>.rules]]` with `host`.

The listener's own `tls.cert`/`key` is the **default certificate**: it answers a
ClientHello without SNI and one whose name no entry claims. That is normally the
source's `default_server` certificate, and its backend is normally the source's
`default_server` backend.

### Certificate selection is per handshake

`SniResolver` (`eqproxy-net/src/tls.rs`) normalizes the SNI name (lowercase,
one trailing root dot removed) and picks, in this order:

1. an exact `names` match;
2. the longest matching wildcard `*.domain` — exactly one extra label, so
   `*.example.com` matches `a.example.com` but not `example.com` or
   `a.b.example.com`;
3. the listener's default certificate.

An unknown name or a client without SNI is not an error: it gets the default
certificate, like nginx's default server. The handshake fails only if the
listener has no certificate at all, which the config gate rejects.

### Host selection is per request

`host` is an exact match against the request's parsed `Host` / `:authority`:
case-insensitive, port-free, with one trailing DNS dot removed
(`eqproxy-core/src/rules.rs`). Rules are evaluated in declaration order and the
**first match wins**. A rule with no `host` condition is not restricted by
name: with a lone path condition it is the catch-all. The listener's `backend`
is the fallback when no rule matches at all.

Converters emit one copy of a vhost's rules per exact `server_name`, tag each
copy with `host`, and keep a hostless catch-all last. A source name that is not
exact (wildcard or regex) needs `host_prefix`/`host_regex`; a converter must not
widen an exact name into a prefix to avoid the extra work, because the wider
rule would shadow other vhosts.

### Why not one listener per vhost

- **Bind collision.** One `IP:port` is one TCP socket and one UDP endpoint. Two
  `[listener.*]` tables cannot both own it; the second bind fails (or, with
  `SO_REUSEPORT`, silently splits traffic). EQProxy does not use
  `SO_REUSEPORT`, and it would not help: the kernel chooses the accepting socket
  when the SYN arrives, before the ClientHello carries SNI and before the
  request carries `Host`. The kernel hashes a 4-tuple; it does not know names.
- **The name does not exist at accept time.** SNI arrives during the handshake
  on an already-accepted connection; `Host`/`:authority` arrives later, inside
  the request (HTTP/1.1) or its HEADERS frame (h2/h3). A per-vhost listener
  would have to choose its socket before either name is known. Only one accept
  loop can receive the connection and then choose the certificate and the rule.
- **Listener identity is per address.** `maxconn`, idle/connect timeouts, ALPN,
  the QUIC endpoint, compression, the response cache, metrics and the admin
  status all describe one wire endpoint. A listener per vhost would multiply
  sockets, counters and bind conflicts for one address, and the admin could no
  longer say which one is serving a connection.
- **Certificates are already per name.** The part that genuinely varies per
  vhost on one socket — the certificate — is represented by `certs[]`, so a
  second listener adds no expressiveness.

### Listener-scope values unify; TLS security losses are `Degraded`

The `default_server` (or the first `server` block on the address) supplies every
listener-scope value, on top of the `http`-level defaults. Other vhosts
contribute certificates and rules, not a second copy of the listener.

Three TLS knobs can be declared per vhost by nginx and are held once per
listener by EQProxy. When a non-default vhost declares a different value, the
listener keeps the default's value and the converter reports a **`Degraded`**
classification (`Severity::Degraded`, ADR-0050) with the existing variant:

| Knob | Source (nginx) | EQProxy field | `Degradation` |
|---|---|---|---|
| Client authentication | `ssl_client_certificate` + `ssl_verify_client` | `tls.client_ca` (present = mTLS required) | `ClientCertificate` |
| TLS versions | `ssl_protocols` | `tls.min_version` / `tls.max_version` | `ProtocolPolicy` |
| Cipher suites | `ssl_ciphers` | none — EQProxy fixes its own policy (ADR-0035) | `CipherPolicy` |

The same declaration on both sides is `Info` ("the listener already applies
it"). A difference is `Degraded` rather than `Review` because the generated
configuration still serves the vhost, but with the default's policy: the
source's per-vhost security intent is lost, and a report that called it
converted would overstate the result. A `Degraded` is never silent and is
counted in the importer report. On the default vhost, a cipher declaration is
`Review`, not `Degraded`: EQProxy has no field to put it in (ADR-0035).

ALPN/HTTP/2 and QUIC do not appear in that table: in the sources they are
listen-address properties, not per-vhost declarations, so there is no
per-vhost value to lose. Everything else listener-scope is **not** a
`Degraded` either: timeouts, maximum body size, compression, `maxconn` and
cache are behavior or capacity settings, not a security policy the source
expressed per vhost. The listener still has one value (the default's); a
converter reports a divergent per-vhost declaration at its normal severity
(`Info`, `Review` or `Gap`) and must not present it as a per-vhost setting.

## Alternatives considered

| Alternative | Why it is rejected |
|---|---|
| One listener per vhost | Bind collision on one address; the name is unavailable when the socket is chosen; duplicates maxconn, timeouts, QUIC and metrics for one endpoint |
| `SO_REUSEPORT` with one listener per vhost | The kernel selects the socket before SNI/Host and by 4-tuple hash, not by name; traffic for one vhost can land on another vhost's acceptor |
| One listener per certificate | Same socket, same collision; `certs[]` already selects per SNI during the handshake |
| Per-vhost TLS policy (mTLS, versions, ciphers) | One rustls config and one QUIC endpoint per listener; honoring it would require a socket per policy and reintroduces the bind collision |
| Widen every `server_name` into `host_prefix`/`host_regex` | Over-broad rules shadow other vhosts; exact names stay exact, non-exact names are reported instead of guessed |

## Consequences

- A vhost is added without a new socket: one certificate entry plus its `host`
  rules. A source with N vhosts on one address becomes one listener: the
  `default_server` certificate is the base `cert`/`key`, the other N−1 are
  `tls.certs[]` entries, and the vhosts contribute N rule sets.
- Duplicate addresses fail validation with both listener names and the
  `listener.<name>.listen` field, instead of failing at bind time. Importers
  must merge vhosts instead of emitting one listener each.
- A vhost cannot have its own ALPN, mTLS, TLS version range or cipher policy.
  A source that needs one of those per name must give that vhost its own
  address; on one address the default server's values apply to every name.
- Virtual hosting on one listener is **routing, not an isolation boundary**:
  SNI selects the certificate, while `Host`/`:authority` selects the backend.
  A client can complete the handshake for one name and send a request with
  another, so vhosts that must not reach each other's backends need separate
  listeners/addresses or explicit rules, not a shared one.
- Existing single-vhost configurations are unchanged: an empty `certs` list and
  hostless rules keep their old meaning (default certificate, listener
  backend), and the fields are additive.
- Certificate material is consulted per handshake from the listener's resolver
  (ADR-0025); selecting a certificate by SNI is one resolver pass per handshake
  (exact name, then the longest wildcard), not a new socket or a new TLS
  context per name.
