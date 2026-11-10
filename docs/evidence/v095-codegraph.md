# CodeGraph Rust — v0.9.5 / AUD-01 final tree

Date: 2026-11-09. Tree: branch `release/v0.9.5`, HEAD `ecda55a` plus the
uncommitted AUD-01 changes. The index was rebuilt after the last code edit.

## Index

The default global config selects `embeddinggemma-2:740m`, but that model
has no tokenizer in CodeGraph's known-model table and the first index attempt
aborted with:

```text
Vector error: Unknown Ollama embedding tokenizer. Set
CODEGRAPH_TOKENIZER_PATH or CODEGRAPH_TOKENIZER_REPO; generic token
counting is unsafe.
```

The retry used a supported, already-installed embedding model **without
changing the global config or the product**:

```bash
CODEGRAPH_EMBEDDING_MODEL=nomic-embed-text \
  codegraph-rust index . --force --stats-json /tmp/v095-cg-index.json
```

`index` log: `/tmp/v095-codegraph2.log`. The stats JSON is preserved as
`docs/evidence/v095-codegraph-index.json`.

| Metric | Value |
|---|---:|
| files | 93 |
| lines | 87 969 |
| nodes | 9 644 |
| edges | 19 354 |
| resolved edges | 19 357 |
| resolution rate | 61.87 % |
| embeddings | 9 778 |
| embedding dimension | 768 |
| provider | ollama (`nomic-embed-text`) |
| errors | 0 |
| `graph_complete` | true |
| `vector_index_ready` | true |
| `complete` | true |
| `index_ms` | 207 147 (~3 min 27 s) |

The index is in `.codegraph/` (local-only, 10 GB store). The index was built
in `fast` tier (tree-sitter, LSP off, no dataflow); it is a structural map,
not a test run.

## `agent impact`

```bash
CODEGRAPH_EMBEDDING_MODEL=nomic-embed-text \
  codegraph-rust agent impact "what breaks if Cache::refresh changes" \
  --format text
# exit 0
```

Saved as `docs/evidence/v095-codegraph-impact.log`.

Source-checked facts used:

- The one real caller is `OriginHeadPhase::read` → `Cache::refresh` at
  `eqproxy-net/src/http1.rs:3571`; `Cache::refresh` starts at
  `eqproxy-net/src/cache.rs:476`.
- `refresh` calls `refresh_storable` (`cache.rs:573`), returns
  `RefreshOutcome` (`cache.rs:322`), and calls `refresh_store`
  (`cache.rs:691`) after the guard is gone.
- `OriginHead.invalidated` is declared at `http1.rs:3434` and consumed by
  the `serve_http` retry at `http1.rs:4853`; the second-`304` refusal is at
  `http1.rs:4880`.

**Do not trust these parts of the answer:** it reports an incoming
`refresh_store → refresh` edge; the source shows the reverse
(`refresh → refresh_store`), so that edge is mis-resolved. It also gives
stale line ranges for some tests. The agent itself marks the depth-2/3 walk
unreliable because name collisions dominate and the tool budget ended.

## `agent context`

```bash
CODEGRAPH_EMBEDDING_MODEL=nomic-embed-text \
  codegraph-rust agent context \
  "where is a revoking 304 handled for the response cache" --format text
# exit 0
```

Saved as `docs/evidence/v095-codegraph-context.log`.

Source-checked facts used:

- Revocation is decided in `Cache::refresh` (`cache.rs:476`) by
  `refresh_storable` (`cache.rs:573`); on refusal it increments the
  per-listener `Cache::cache_304_invalidations()` counter (`cache.rs:357`),
  calls `complete(key, FetchOutcome::Gone)` and returns
  `RefreshOutcome::Invalidated`.
- `OriginHead.invalidated` (`http1.rs:3434`) carries the result to
  `serve_http`; the retry disarms the guard, drops the origin connection and
  re-runs the exchange unconditionally (`http1.rs:4853` onward).
- The five AUD-01 E2E journeys live in `eqproxy-net/tests/e2e_cache.rs`
  (`a_304_with_session_headers_never_pollutes_the_shared_entry`,
  `a_second_304_to_the_unconditional_retry_is_a_bad_gateway`,
  `a_304_that_changes_vary_invalidates_the_entry`,
  `a_revoking_304_never_serves_new_headers_to_a_lock_waiter`,
  `a_revoked_304_entry_does_not_resurrect_from_disk`).
- The agent could not pin the exact `http1.rs` call line; the source shows
  it at `3571`. The counter is **not** in `Metrics`: it is
  `Cache::cache_304_invalidations()` at `cache.rs:357`, a private
  `invalidations_304` atomic read through an additive public method. This is
  a source-checked correction, not an agent claim.

## Limits

- Fast tier: no LSP, build context, dataflow or docs contracts. The graph
  covers structure only.
- Resolution rate 61.87 %; unresolved edges and name-collision noise remain.
- The `agent` responses are advisory. They are never used as behavioural
  proof here; `cargo test` and the E2E journeys are.
- The index is stale the moment the Rust tree changes; it was built after
  the final code edit and before only documentation edits.
