# ADR-0052: the 0.10.0 version step, public API compatibility and the Dylint lint crate

Language: **English**.

- State: **Accepted** (2026-11-16)
- Date: 2026-11-16
- Related: [ADR-0030](0030-audit-tool-applicability.md) (superseded in part:
  §1 dylint and §2 semver-checks), [ADR-0051](0051-multi-vhost-one-listener-per-address.md)
  (the redirect-only listener that adds the field), `docs/release-0.10.0.md`,
  `lints/eqproxy-lints/`, [veyra.toml](../../veyra.toml)

## Context

Two audit legs changed state with the v0.9.6 work, and both need a decision
rather than a flag change.

1. **`cargo-semver-checks` now runs.** Veyra compares each crate against the
   `v0.9.1` tag. On the tree prepared as the 0.9.6 patch it reported:

   | Crate | Verdict | Finding |
   |---|---|---|
   | `eqproxy-config` | requires a breaking-class bump | `constructible_struct_adds_field`: `ListenerConfig.redirect` (schema.rs:1149) |
   | `eqproxy-config` | — | `function_must_use_added`: `validate::check` (validate.rs:41) |
   | `eqproxy-net` | requires a minor bump | `inherent_method_must_use_added`: `RateLimiter::new` (proxy.rs:356), `RouteDecision::{rate_limiter, mirror, auth}` (proxy.rs:473/485/497) |
   | `eqproxy-core` | no change required | 223/223 checks pass |

   Four of the five findings are `#[must_use]` attributes the pedantic
   migration added; the fifth is the field the redirect-only listener needs.

2. **Dylint is now applicable.** ADR-0030 §1 recorded dylint as *not adopted*
   because EQProxy owned no lint crate. `lints/eqproxy-lints/` now exists and
   is wired by `[workspace.metadata.dylint]`, so Veyra's dylint leg runs for
   real. It then failed to compile the workspace:
   `error[E0658]: use of unstable library feature 'result_option_map_or_default'`
   at `eqproxy-core/src/maps.rs:491`. The cause was not a typo: the tree used
   `Option::map_or_default` in **60** places across 16 files, an API stabilized
   well after the lint crate's pinned `nightly-2026-05-28` *and* after the
   `1.85` MSRV that `CONTRIBUTING.md` documents.

## Decision

### 1. The API delta ships as 0.10.0, a breaking-class version step

The `#[must_use]` attributes and `ListenerConfig.redirect` stay. They are
correct API hygiene (`clippy::pedantic` asks for the attributes) and the field
is the redirect-only listener's only representation.

The version moves instead. An `#[must_use]` addition is minor-class and a new
public field on an exhaustively constructible struct is breaking-class, so no
patch can carry either: **0.9.6 was not semver-valid**, and the leg was right
to reject it. cargo-semver-checks applies the pre-1.0 convention where the
breaking-class bump is the middle digit (`1.2.3 to 2.0.0 or 0.5.2 to 0.6.0` in
its own documentation), so `0.9.1 → 0.10.0` is the step that absorbs the delta.

What the tool needs to be told is the class, not a version number:
`veyra.toml` sets `[semver] release_type = "major"`, which is
cargo-semver-checks' name for the breaking class. Forcing
`release_type = "minor"` fails by construction (a minor-class release cannot
carry a major-class change), and omitting the flag makes the tool derive the
same class on its own
(`Checking eqproxy-config v0.9.1 -> v0.10.0 (major change)`), which is the
cross-check that the explicit value is not a relaxation.

Verified on the 0.10.0 tree:
`cargo semver-checks check-release --baseline-rev v0.9.1 --release-type major`
exits **0**, with `no semver update required` for `eqproxy-config`,
`eqproxy-core` and `eqproxy-net`.

The five findings remain listed in `docs/release-0.10.0.md`, so the delta is
inventory, not memory. If any crate is ever published to a registry, its
published version becomes the baseline and the same rules apply from there.

### 2. Dylint is adopted, and the code moves back to its pinned toolchain

`lints/eqproxy-lints` stays at `nightly-2026-05-28` — the nightly its
`cargo-dylint 6.3.2` build requires. Bumping the pin was tried first and
rejected with evidence: `cargo dylint --all` on `nightly-2026-10-22` fails while
compiling dylint's own rustc-private dependencies
(`error[E0797]: default field values are unstable` in
`rustc_lint_defs/src/lib.rs:695`), so a newer nightly needs a rebuilt dylint,
not a one-line pin change.

The tree moves instead: every `Option::map_or_default` became
`map_or_else(Default::default, …)` (and the `Result`/`LockResult` sites
`map_or_else(|_| String::default(), …)`), which is the same behaviour on an API
that has been stable since long before `1.85`. That is the real defect the leg
exposed — the workspace was using post-1.85 std APIs while documenting `1.85` as
its MSRV — and it is fixed in the code, not behind an `#[allow]`.

ADR-0030 §1 is superseded: the project now owns a lint crate, so the leg is
real and must stay green.

## Consequences

- Veyra's `semver-checks` and `dylint` legs must be green on this tree; both are
  cited with their evidence directory in `docs/release-0.10.0.md`.
- The version and the tag move together: the release workflow reads the version
  from `Cargo.toml` and refuses a tag that does not match it, so the release is
  `v0.10.0` or nothing. The stale local `v0.9.6` tag (commit `0f03bf4`) stays
  where it is; it is not the tag for this tree.
- New code must stay inside the documented MSRV; a std API newer than the
  pinned toolchain breaks the dylint leg and is a code defect, not a tool
  problem.
- Clippy (`--workspace --all-targets -D warnings`) and the suite remain
  blocking and unchanged by this decision.
