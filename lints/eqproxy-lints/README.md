# EQProxy project lints

`eqproxy_lints` contains project-specific Dylint lints. The workspace registers
the library in `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [{ path = "lints/eqproxy-lints" }]
```

Run it from the repository root with:

```bash
cargo dylint --all
```

## `EQPROXY_NO_DBG`

`dbg!` writes to stderr and is a temporary debugging aid. In a proxy it can
leak request data into logs and bypass the configured access-log path.

The lint checks each AST item's source span and reports:

```text
remove `dbg!`; use the configured logger or access log instead
```

The UI test under `ui/` is a negative control: it contains a `dbg!` call and
asserts the full diagnostic in `ui/main.stderr`. A lint that stops firing makes
that test fail.

The crate uses the `nightly-2026-05-28` toolchain pinned in
`rust-toolchain.toml`, matching the toolchain used to build the Dylint driver.
