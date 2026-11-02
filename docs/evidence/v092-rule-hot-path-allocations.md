# Rule hot-path allocations (C-3a)

Measured on 2026-11-02 on the maintainer's macOS host, with the release
profile and the workspace's pinned Rust 1.99.1. The probe is a standalone,
measurement-only crate under the local-only `scripts/evidence/v092-rule-alloc/`
tree: the published workspace forbids `unsafe_code`, and the counting
`GlobalAlloc` wrapper requires an `unsafe impl`.

## Command

```bash
cargo run --manifest-path scripts/evidence/v092-rule-alloc/Cargo.toml --release
```

## Result

```text
exact: 1000000 matches, 0 allocations
prefix: 1000000 matches, 0 allocations
regex: 1000000 matches, 1000000 allocations
```

The exact and prefix host matchers perform **no heap allocation per
`Rule::matches` call** after the rule is compiled. The regex matcher still
allocates one lowercased `String` per call, because regex semantics are
defined over the lowercased, port-free host (Q8). C-3a removed the allocation
from the exact/prefix hot paths; it did not change regex semantics.

## Limits

- This is an allocation-count probe, not a latency benchmark. No throughput or
  latency improvement is claimed from it.
- It measures the local macOS host and system allocator, not the published
  musl/Linux binary. The allocation count is a property of the code path; the
  allocator's cost is not measured here.
- The probe is not shipped, is not part of `cargo test --workspace`, and its
  `unsafe` block is confined to the local measurement harness.
