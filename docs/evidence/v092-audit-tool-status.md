# Audit tool status (v0.9.2 development)

Date: 2026-11-02
Branch: `harden/config-gate-and-errors`
Base: `64a1d77`

This note records what was actually executed. Test counts, commands and
limitations are copied from the command output; nothing here turns an
unresolved finding into a pass.

## Dylint

The workspace registers `lints/eqproxy-lints` in `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [{ path = "lints/eqproxy-lints" }]
```

The library defines `EQPROXY_NO_DBG` and has a UI negative control containing a
real `dbg!` call with the full expected diagnostic in
`lints/eqproxy-lints/ui/main.stderr`.

Commands and results:

```text
$ cd lints/eqproxy-lints && cargo test
test [ui] ui/main.rs ... ok
test result: ok. 1 passed; 0 failed

$ cd lints/eqproxy-lints && cargo clippy --all-targets -- -D warnings
Finished `dev` profile ... (exit 0)

$ cargo dylint --all
Finished `dev` profile ... (exit 0)
```

The lint is configured and firing against a real fixture. The earlier empty
`.stderr` was a vacuous test; it is no longer used.

## cargo-semver-checks

A real Git baseline exists: tag `v0.9.1`. The final check against it is:

```text
$ cargo semver-checks check-release --manifest-path Cargo.toml \
    --workspace --baseline-rev v0.9.1 --release-type patch
Checked 223 checks: 223 pass, 0 fail, 0 warn, 31 skip
Summary no semver update required
exit 0
```

The earlier `221 pass, 2 fail` result was real: `MapConditionSpec.header` had
been removed and `HeaderSpec` / `CookieSpec` had changed from structs to enums.

The fix keeps the published `0.9.1` representations:

- `HeaderSpec` and `CookieSpec` are structs with `name`, `exact`, `regex` and
  `present` again.
- `MapConditionSpec` again has `name`, `key: Option<String>`,
  `header: Option<String>` and `equals`.
- `Rule::compile` still rejects zero or several match kinds, and zero or several
  map key sources, with field-qualified errors. The invalid shape is now
  rejected at compile time instead of making valid `0.9.1` client code fail to
  compile.
- `eqproxy-config` maps its parsed condition tables back to those public
  shapes; the core compiler remains the single validator.

The two checks that previously failed now pass. A bumped `0.10.0` was tested
and rejected as evidence: `cargo-semver-checks` classified it as a major change
and returned `0 checks: 0 pass, 254 skip`, which would be a false green.

Veyra 0.7.0's semver leg is now configured, not skipped. The local Veyra
source (`/Volumes/Lexar/Cursor/Veyra-Proof`) gained `SemverConfig` and passes
`--baseline-rev` / `--release-type`; the project declares:

```toml
[semver]
baseline = "v0.9.1"
release_type = "patch"
```

The real Veyra leg was executed with only that tool selected:

```text
$ veyra audit . --profile standard --include semver-checks --format json
decision: ACCEPTED, exit 0
semver-checks: OK, exit 0, 25.2 s
command: cargo semver-checks check-release --baseline-rev v0.9.1 --release-type patch
evidence: .veyra/evidence/20261102T213724Z/
```

The full Veyra profile and its other legs were not re-run after this change;
their previous status is not claimed as green here. The Veyra source patch is
local, like the Geiger fork.

## cargo-geiger

A local patched fork of `cargo-geiger 0.14.0` and `geiger 0.5.0` was built from
the crates.io sources:

```text
target/audit-tools/cargo-geiger-0.14.0/
target/audit-tools/geiger-0.5.0/
```

The fixes are behavioral, not warning suppression:

1. `CustomExecutor` records the `.d` files emitted by the current rustc
   invocation instead of walking every old file under `target/`. The previous
   run counted 972 stale registry files and 3 generated files from earlier
   builds.
2. Non-target `.rs` files with a cargo-script `---cargo` frontmatter are not
   parsed as Rust modules (`aws-lc-rs/util/process-criterion-csv.rs`).
3. The legacy bare trait-object spelling used by
   `signal-hook-registry 1.4.8`
   (`type Action = Fn(&siginfo_t) + Send + Sync;`) is re-parsed with an inserted
   `dyn` only for type aliases; unsafe counters are unchanged. This removes the
   `syn: expected ';'` parse failure without skipping the file.
4. Generated `.rs` files used by the current build (for example serde's
   `out/private.rs`) are scanned and attributed to the package named by their
   build directory. They are no longer reported as "never scanned".

Final command and result:

```text
$ PATH="$PWD/target/audit-tools/cargo-geiger-0.14.0/target/release:$PATH" \
    cargo geiger --color never --manifest-path "$PWD/eqproxy-core/Cargo.toml"
exit 0
0 "Dependency file was never scanned" warnings
0 "Failed to parse file" diagnostics
```

The stdout safety report is in
`target/audit-tools/logs/geiger-final.out`; stderr is in
`target/audit-tools/logs/geiger-final.err`. The patched binary is local-only
under `target/` and is not committed.

## Verification run in this increment

```text
$ cargo check --workspace --all-targets
exit 0

$ cargo test -p eqproxy-core --lib
59 passed; 0 failed

$ cargo test -p eqproxy-core --test rules_fuzz_corpus
4 passed; 0 failed

$ cargo test -p eqproxy-config --lib
111 passed; 0 failed

$ cargo clippy --workspace --all-targets -- -D warnings
exit 0

$ cargo fmt --all -- --check
exit 0
```

The E2E `eqproxy-net --test e2e_rules` run was started but exceeded the
five-minute command window while compiling; it is not claimed as executed.
`cargo test --workspace` was not run in this increment.
