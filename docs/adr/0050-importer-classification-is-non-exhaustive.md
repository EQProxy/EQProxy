# ADR-0050: The importer's classification enum is `#[non_exhaustive]`

- State: **Accepted** (2026-11-13, roadmap v0.9.6, box N-2)
- Date: 2026-11-13
- Related: [`docs/converters.md`](../converters.md), `eqproxy-config/src/import/mod.rs`,
  `eqproxy-config/Cargo.toml`, `veyra.toml` (`[semver] baseline`)

## Context

`eqproxy_config::import::Severity` is the public classification of every source
directive: `Done`, `Info`, `Review`, `Gap`, `Rejected`. N-2 added a sixth state —
`Degraded`, «converted, but with less security than the source declared» —
because a lost TLS policy must never read as converted.

The enum was public, exhaustive and **not** `#[non_exhaustive]`, so adding a
variant is a breaking change for any downstream exhaustive `match`. The semver
gate runs against the published baseline `v0.9.1` with `release-type patch`
(`veyra.toml`), and it flagged three things in the same release:

```
enum_variant_added              Severity:Degraded
enum_no_repr_variant_discriminant_changed   Gap 3->4, Rejected 4->5
partial_ord_enum_variants_reordered          Gap, Rejected
```

The last two were fixed by appending the variant after the existing ones
(discriminants and `Ord` positions unchanged). The first one is structural: an
exhaustive public enum cannot gain a variant in a patch release.

Measured alternatives (all with `cargo semver-checks check-release
--baseline-rev v0.9.1 --release-type patch`):

| option | result |
|---|---|
| keep it exhaustive, allow `enum_variant_added` forever | exit 0, but the lint never protects this crate again: an accidental variant would pass silently |
| keep it exhaustive, set the lint to `warn` | exit 0 with `warning enum_variant_added` — visible, but CI stays green on a real break |
| mark it `#[non_exhaustive]` | `enum_marked_non_exhaustive` fires **once**, for the transition from the pre-attribute baseline |
| keep five states and move degradation into a private type alongside | no lint change, but two sources of truth: the public enum would describe a classification the pipeline no longer produces |

## Decision

`Severity` (and `Degradation`, which is new and therefore free) is
`#[non_exhaustive]`: the classification is a contract that grows, and a reader
outside the crate must leave a wildcard arm. Adding a variant is no longer a
semver break, so **`enum_variant_added` is enabled again in this crate** — proven
by removing the attribute: the lint fires with exit 100.

Exactly one lint stays allowed, and only for the historical transition:

```toml
[package.metadata.cargo-semver-checks.lints]
enum_marked_non_exhaustive = "allow"   # expires with the v0.9.1 baseline
```

That baseline predates the attribute, so marking the enum non-exhaustive is a
break *once*. The allowance is scoped to that fact, not to the crate: when
`veyra.toml`'s `[semver] baseline` moves to a tag that already carries the
attribute, both sides of the comparison agree and the line must be deleted.
`eqproxy-config/tests/semver_contract.rs` is the tripwire: it requires the
allowance to be `"allow"` exactly while the baseline is `v0.9.1`, so it fails
if the baseline moves without deleting it, if the allowance is removed too
early, or if anyone reintroduces an `enum_variant_added` allowance.

## Consequences

- Downstream readers of `Severity` need a wildcard arm; inside the crate the
  matches stay exhaustive, so the report and the counters still cover every
  state explicitly.
- A future severity is a normal, additive change again — and if someone adds one
  without meaning to, the gate says so.
- The remaining allowance is a one-release exception with a written removal
  condition and an automated reminder, not a lint switched off for good.
- The SemVer-correct long-term move — bumping the crate to `0.10.0` and dropping
  the transition allowance at the same time — belongs to the release that
  changes the baseline, not to this patch cut.
