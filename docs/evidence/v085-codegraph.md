# CodeGraph Rust on `fccc747` — graph and vector index complete

**Scope.** The increment `fccc747` (*docs: record the published v0.8.5 artifacts
and the state they describe*) changes no Rust: README, the release checklist and
`docs/evidence/`. The graph therefore describes the same source the release tag
`v0.8.5` was built from, and the two agent queries below target the module the
new documentation describes — the per-listener HTTP response cache — so their
answers can be checked against that source.

**Why this page exists.** An earlier run of the same tree ended with
`Stages: graph=true, embeddings=pending, semantic=off` (`vector_index_ready:
false`), which is a graph without vectors, not a finished leg. This page records
the completed index: 8 406 embedded chunks, the vector index searchable, and the
two agent queries answered and fact-checked.

## What was run

```bash
# the store carried a stuck HNSW build (see "Stuck compaction" below), so the
# database was moved aside and rebuilt into a fresh one
mv .codegraph/db .codegraph/db.stale-20261031T101632Z

CODEGRAPH_EMBEDDING_POLICY=sync \
CODEGRAPH_SEMANTIC_RESOLUTION=sync \
codegraph-rust index . --force --batch-size 64 \
  --stats-json /tmp/cg-v085-final.json

codegraph-rust agent impact  "what breaks if the per-listener HTTP response cache \
  in eqproxy-net/src/cache.rs changes" --format text     # exit 0
codegraph-rust agent context "where is the per-listener HTTP response cache \
  configured from TOML and wired into a listener at startup" --format text  # exit 0
```

Tree: `fccc74790609b1c88b46a8d2790a06559133541c`, `main`, working tree clean
before the run. Toolchain `codegraph 0.1.0` (`codegraph-rust`), config
`~/.codegraph/config.toml` (embedder `qwen3-embedding:0.6b` on Ollama, 1 024 dims;
LLM `deepseek-v4.1-flash:1017-cloud`), indexing tier `fast`. macOS aarch64.

**Exit status.** The index run was launched as a background process and its exit
status was **not captured**; completion is attested by the stats file the CLI
writes at the very end (`complete: true`) and by the summary block in
[raw/20261031T101600Z/index.log.txt](v085-codegraph/raw/20261031T101600Z/index.log.txt).
Both agent queries were run in the foreground and **exited 0**.

The incremental run that produced the first `not ready` error is kept:
`/tmp/cg-v085-final.log` (outside the repo; the transcript inside
`raw/` is the rebuild).

## Numbers from `--stats-json`

Raw file: [raw/20261031T101600Z/index-stats.json](v085-codegraph/raw/20261031T101600Z/index-stats.json)

| Field | Value |
|---|---|
| `files` / `lines` | 79 / 76 889 |
| `nodes` | 8 406 (`functions` 2 066, `structs` 244, `traits` 3) |
| `edges` | 14 816 resolved, 12 827 unresolved, `resolution_rate` 53.6 % |
| `embeddings` / `chunks` | 8 406 / 8 406 |
| `embedding_dimension` / `provider` | 1 024 / `ollama` |
| `embedding_status` / `semantic_status` | `ready` / `ready` |
| `vector_index_ready` | **`true`** |
| `graph_complete` / `complete` / `errors` | `true` / `true` / 0 |
| `index_ms` / `startup_ms` | 94 881 / 3 621 (reported total 98.55 s, 10:16:39 → 10:18:18 UTC) |
| `embeddings_and_writes` | 22 873 ms |
| `resolution_symbol_embeddings` | 10 634 ms |
| `writer_rows_acked` / `source_read_operations` | 36 266 / 79 |

Of 27 643 relationships, 14 816 resolved: 9 442 exact/contextual, 1 816 lexical,
**3 558 semantic** (445 of 740 unresolved names matched, 14 187 comparisons).
The 12 827 that remain unresolved are the honest remainder, not a regression:
the deferred run before the rebuild reported 40.7 % because it had no semantic
pass at all.

**Where the vectors came from.** `inference_texts: 0`, `inference_tokens: 0`,
`embedding_cache_hits: 12 965`: the rebuild did not call the model. The 8 406
chunk vectors were produced by `qwen3-embedding:0.6b` for these same node texts
in the 09:38–09:55 UTC pass over this tree (log
`/tmp/cg-v085-final.log`, `Semantic embedding generation results: 8 406 nodes
embedded`), and the rebuild materialised them into the fresh store from the
embedding cache. No vector in the store predates the current sources.

**Store.** Fresh `.codegraph/db` is 639 MB (the previous 2.9 GB store, whose
compaction was stuck, is kept as `.codegraph/db.stale-20261031T101632Z`).
`.codegraph/` is git-ignored in full (`.codegraph/.gitignore` is `*`), so the
store stays local and only this page and its transcripts are published.

## Stuck compaction, and what was actually wrong

Two runs over the *existing* store finished indexing and then died at the final
status step:

```
Error: Database error: Vector index idx_chunks_embedding_1024 not ready:
{"building":{"compacting":true,"initial":0,"pending":0,"status":"ready","updated":0}}
```

`pending: 0`, `initial: 0`, `updated: 0` with `compacting: true` and no progress
between two runs minutes apart: the flag was stuck, not the work. SurrealKV does
not implement in-place compaction (the tool's own `db-rebuild --help` says so),
so the store was moved aside and rebuilt. The same index definition on the fresh
store reached `compacting` → searchable within the same run and wrote
`vector_index_ready: true`. A first attempt at this rebuild (17 min of real
embedding, ending 09:58:55 UTC) hit the same error because it *wrote* 8 406 fresh
vectors late in the run; the second, cache-fed rebuild (98.55 s) had the whole
run to compact and succeeded.

## Facts used, checked in the source

`agent impact` (exit 0, [raw/20261031T101600Z/agent-impact.txt](v085-codegraph/raw/20261031T101600Z/agent-impact.txt)):

- `eqproxy-net/src/lib.rs:47-50` re-exports `Bypass, Cache, CacheKey,
  CacheSettings, Decision, FetchOutcome, Lookup, SharedFetch, StoredParts` —
  confirmed verbatim.
- `eqproxy-net/src/http1.rs:42` `use crate::cache;` — confirmed.
- `enum CacheAction` at `http1.rs:1741-1754` with `Off, Serve, Origin, Direct,
  Wait` — confirmed (five variants, `Direct` included).
- `resolve_cache_wait` at `http1.rs:1851`, `serve_cached` at `:1970` — confirmed;
  both `serve_cached(` call sites are `:2671` and `:3150`, inside
  `serve_http` (`:2187`), which is what the tool meant by "called by
  `serve_http` (two call sites)".
- `Cache::lookup` at `eqproxy-net/src/cache.rs:333`, `Cache::complete` at `:393`,
  `Cache::wait` at `:378`, `render_head` at `:1128`, `client_fresh` at `:1143` —
  confirmed.
- `eqproxy-net/src/proxy.rs:923` `cache: None` in `ResolvedListener::new` —
  confirmed.
- `eqproxy-net/tests/e2e_cache.rs` assertions at `:403` (MISS→HIT, `age`,
  origin hit count stays 1, `cache_hits/misses/stores == 1`), `:461`
  (`Set-Cookie` never stored), `:532` (TTL → `REVALIDATED`), `:599`
  (`authorization`/`range` bypass), `:665` (cookie affinity disables the cache) —
  confirmed, including the assertion bodies.
- In-crate unit tests: `mod tests` at `cache.rs:1423`, `miss_then_hit` at
  `:1456`, `failure_drops_the_stale_entry` at `:1472`,
  `disk_round_trip_restores_unexpired_entries` at `:1548` — confirmed.

`agent context` (exit 0, [raw/20261031T101600Z/agent-context.txt](v085-codegraph/raw/20261031T101600Z/agent-context.txt)):

- `CacheConfig` at `eqproxy-config/src/schema.rs:1201`, `impl Default` at
  `:1224`, `ListenerConfig` at `:995` — confirmed.
- `CacheSettings` at `eqproxy-net/src/cache.rs:89`, `Cache::load` at `:300` —
  confirmed.
- `ResolvedListener` at `proxy.rs:831`, `impl` at `:883`,
  `with_cache` at `:962-965` — confirmed.
- `eqproxy-cli/src/main.rs:2526` `fn resolve_with`, `:2923`
  `eqproxy_net::Cache::load(`, `:2935` `resolved.with_cache(cache)` — confirmed;
  this is the production pairing point.
- `eqproxy-config/src/import/nginx.rs:309` `cache: bool` — confirmed.

**Corrections to the tool's answer:** `CacheSettings::test_defaults` is at
`cache.rs:107`, not the `104-117` range it gave (that range opens the `impl`
block); and the `cache` field on `ListenerConfig` that it reported as unseen is
`eqproxy-config/src/schema.rs:1066` (`pub cache: Option<CacheConfig>`).

## The vector index is serving, not merely flagged ready

`vector_index_ready: true` is the tool's own verdict. To see vectors actually
retrieve, a third query deliberately avoided every identifier in the answer:
"where do concurrent clients avoid two identical slow backend fetches piling
up" (exit 0,
[raw/20261031T101600Z/agent-context-semantic.txt](v085-codegraph/raw/20261031T101600Z/agent-context-semantic.txt)).
It landed on single-flight coalescing in `cache.rs`: `Lookup` at `:207-218`
(`Hit`/`Fetch`/`Revalidate`/`Wait`), `SharedFetch` at `:221-224`
(`Notify` + `Mutex<Option<FetchResult>>`), `FetchResult` at `:228-234`, and
`Cache::lookup` at `:333-373` as the gate — all four confirmed in the source,
byte for byte. A lexical match cannot produce that path, because the question
never names `cache`, `inflight`, `SharedFetch` or `Notify`.

## What CodeGraph itself flagged as missing

The impact answer states that the graph holds **no `Calls` edges into the `Cache`
impl node**, so its caller list came from import edges and search snippets, and
that its reverse-import traversal and cycle detection returned nothing because
the result budget was exhausted. That is a pointer, not a finding: the caller
list above was re-derived from the source, and no cycle claim is made either way.

## What this leg does not prove

Structure only. It does not prove behaviour: it cannot replace `cargo test`, the
E2E journeys, Veyra or Rust Audit, and the 53.6 % resolution rate is a coverage
number for the index, not a quality score. The agent answers are advisory; every
`file:line` they contributed was opened and confirmed here, and the two that did
not match are corrected above.
