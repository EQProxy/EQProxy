//! The semver transition that lets `Severity` grow without a major bump.
//!
//! `Severity` is `#[non_exhaustive]`, so adding a classification is additive.
//! Marking it non-exhaustive is itself a break against the pre-attribute
//! `v0.9.1` baseline, which is why `Cargo.toml` allows that one lint. This test
//! keeps the allowance paired with the baseline; ADR-0050 has the rationale.

/// The only baseline the non-exhaustive allowance covers.
const TRANSITION_BASELINE: &str = "v0.9.1";

#[test]
fn the_non_exhaustive_allowance_matches_the_baseline() {
    let manifest: toml::Value =
        toml::from_str(include_str!("../Cargo.toml")).expect("eqproxy-config/Cargo.toml is TOML");
    let lints = manifest["package"]["metadata"]["cargo-semver-checks"]["lints"]
        .as_table()
        .expect("Cargo.toml declares [package.metadata.cargo-semver-checks.lints]");
    let baseline: toml::Value =
        toml::from_str(include_str!("../../veyra.toml")).expect("veyra.toml is TOML");
    let baseline = baseline["semver"]["baseline"]
        .as_str()
        .expect("veyra.toml declares [semver] baseline");

    assert!(
        lints.get("enum_variant_added").is_none(),
        "`enum_variant_added` must stay enabled: Severity is #[non_exhaustive], so a new \
         variant is additive and allowing the lint would hide an accidental one"
    );
    assert_eq!(
        lints
            .get("enum_marked_non_exhaustive")
            .map(|value| value.as_str().expect("the lint setting is a string")),
        (baseline == TRANSITION_BASELINE).then_some("allow"),
        "the allowance must be exactly `allow` while the baseline is {TRANSITION_BASELINE} \
         (currently {baseline}); delete it when the baseline moves to a tag that already \
         carries #[non_exhaustive], and do not remove it before then"
    );
}
