# ADR-0036: own HTTP/1.1 parser, no hyper

- State: **Accepted** (2026-10-31; supersedes the HTTP/1–2 row of
  [ADR-0004](0004-network-stack-and-crate-layout.md))
- Date: 2026-10-31
- Related: [ADR-0004](0004-network-stack-and-crate-layout.md) (Phase-0 stack
  research), [THREAT_MODEL.md](../../THREAT_MODEL.md) T01/T02 (smuggling and
  framing ambiguity), [v085-docs-truth](../evidence/v085-docs-truth.md) (the
  review that found the documentation still naming hyper)

## Context

ADR-0004 chose `hyper` for HTTP/1–2 and recorded it as the stack. The Phase-2
dataplane did not ship on it: `eqproxy-net/src/http1.rs` carries the HTTP/1.1
parser and engine, HTTP/2 uses the `h2` crate directly (ADR-0006/0007), and
`hyper` is absent from `Cargo.lock` and from every manifest — it was never a
dependency of the shipped tree. Until 2026-10-31 the README, this ADR and
`THREAT_MODEL.md` still described the researched stack, which is exactly the
kind of unverified claim this project forbids.

Why the parser is ours: the dataplane has to forward **the exact bytes it
validated** and refuse any message with two readings (duplicated
`Content-Length`, `Content-Length` + `Transfer-Encoding`, obs-fold, spaces in
field names). A typed library layer parses a message into its own
representation and hands back parts, not the original framing; it also owns
the rejection policy. Those bytes and that policy are the anti-smuggling
contract (T01/T02), so they live in code this project controls.

## Decision

Keep the HTTP/1.1 parser and engine in `eqproxy-net/src/http1.rs`. `hyper` is
not a dependency and is not planned as one. HTTP/2 keeps `h2`; HTTP/3 keeps
`quinn` + `h3`/`h3-quinn`.

## Consequences

- The framing security surface is ours: it is covered by the rejection corpus,
  the differential corpus against the reference proxy and the framing fuzz
  target (`eqproxy-net/tests/framing_fuzz_corpus.rs`,
  `fuzz/fuzz_targets/framing.rs`). A parser bug is our bug; there is no
  upstream patch to wait for.
- Body relay uses `bytes` and explicit length/chunked handling instead of the
  `http-body` abstraction; HEAD/204/304 and interim responses are handled
  explicitly by the engine.
- ADR-0004's HTTP/1–2 and bodies rows are historical research, not the shipped
  stack. Reopening this decision requires a new ADR, not an edit here.
- If a maintained library ever exposes byte-exact framing plus a
  caller-controlled rejection policy, the trade-off can be re-evaluated with
  the corpus as the acceptance test.
