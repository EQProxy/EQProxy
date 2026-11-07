#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_errors;

use rustc_ast::Item;
use rustc_errors::DiagDecorator;
use rustc_lint::{EarlyContext, EarlyLintPass, LintContext};

dylint_linting::declare_early_lint! {
    /// ### What it does
    ///
    /// Rejects `dbg!` calls in committed source.
    ///
    /// ### Why is this bad?
    ///
    /// `dbg!` writes to stderr and is normally a temporary debugging aid. In a
    /// proxy it can leak request data into logs and bypasses the configured
    /// access-log path.
    ///
    /// ### Example
    ///
    /// ```rust
    /// let value = dbg!(compute());
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust
    /// let value = compute();
    /// tracing::debug!(?value, "computed");
    /// ```
    pub EQPROXY_NO_DBG,
    Warn,
    "`dbg!` must not be committed"
}

impl EarlyLintPass for EqproxyNoDbg {
    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &Item) {
        let Ok(source) = cx.sess().source_map().span_to_snippet(item.span) else {
            return;
        };
        if source.contains("dbg!") {
            cx.opt_span_lint(
                EQPROXY_NO_DBG,
                Some(item.span),
                DiagDecorator(|diag| {
                    diag.primary_message(
                        "remove `dbg!`; use the configured logger or access log instead",
                    );
                }),
            );
        }
    }
}

#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
}
