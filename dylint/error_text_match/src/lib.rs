//! `error_text_match`: no decision is taken from an error's text, in production code or in a
//! test. Match on the error's type or variant instead, or downcast to it.

#![feature(rustc_private)]

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

#[path = "../../config.rs"]
mod config;

use std::ops::ControlFlow;

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::macros::{macro_backtrace, root_macro_call};
use clippy_utils::paths::{PathNS, lookup_path_str};
use clippy_utils::res::MaybeResPath;
use clippy_utils::ty::implements_trait;
use clippy_utils::visitors::for_each_expr;
use clippy_utils::{find_binding_init, is_in_test, sym};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::DefId;
use rustc_hir::{BinOpKind, Expr, ExprKind, UnOp};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty::{self, Ty, TypeckResults};
use rustc_span::Span;

use config::Config;

const MAX_DEPTH: u8 = 8;

/// Methods that hand on the text they are called on, so the result is still an error's text.
const PASS_THROUGH: &[&str] = &[
    "as_str",
    "as_ref",
    "borrow",
    "clone",
    "to_owned",
    "to_string",
    "to_lowercase",
    "to_uppercase",
    "to_ascii_lowercase",
    "to_ascii_uppercase",
    "trim",
    "trim_start",
    "trim_end",
    "as_deref",
    "unwrap",
    "unwrap_or_default",
    "expect",
];

/// Methods that decide something from the text they are called on.
const TEXT_TESTS: &[&str] = &[
    "contains",
    "starts_with",
    "ends_with",
    "find",
    "rfind",
    "matches",
    "strip_prefix",
    "strip_suffix",
    "split",
    "split_once",
    "rsplit_once",
    "eq",
    "ne",
    "eq_ignore_ascii_case",
    "parse",
];

/// Methods whose closure argument makes the result, as in `result.err().map(|e| e.to_string())`.
const CLOSURE_MAPS: &[&str] = &["map", "and_then"];

/// Text tests that also decide from their argument, as in `list.contains(&error.to_string())`.
const ARGUMENT_TESTS: &[&str] = &["contains", "eq", "ne", "eq_ignore_ascii_case"];

const COMPARE_MACROS: &[&str] = &[
    "assert_eq_macro",
    "assert_ne_macro",
    "debug_assert_eq_macro",
    "debug_assert_ne_macro",
];

dylint_linting::impl_late_lint! {
    /// Refuses a decision taken from an error's text: a comparison, a `match`, an `assert_eq!`
    /// or a string test such as `contains` or `starts_with` on what `to_string()` or `format!`
    /// made of a value whose type is an error (it implements `std::error::Error`, or is
    /// `anyhow::Error` or a boxed error). The text is followed through `let` bindings and
    /// through methods that keep it text (`as_str`, `to_lowercase`, `trim`, ...).
    ///
    /// Test code is checked too: a test asserts the error's type or variant, not its wording.
    /// Match on the error's type or variant instead, or downcast to it.
    pub ERROR_TEXT_MATCH,
    Warn,
    "a decision taken from the text of an error",
    ErrorTextMatch::new()
}

pub struct ErrorTextMatch {
    config: Config,
    anyhow_errors: Vec<DefId>,
}

impl ErrorTextMatch {
    /// A pass with the lint's config, read from `rusty-lints.toml` through Dylint.
    fn new() -> Self {
        Self {
            config: Config::load(env!("CARGO_PKG_NAME")),
            anyhow_errors: Vec::new(),
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for ErrorTextMatch {
    fn check_crate(&mut self, cx: &LateContext<'tcx>) {
        self.anyhow_errors = lookup_path_str(cx.tcx, PathNS::Type, "anyhow::Error");
    }

    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if !self.config.enabled {
            return;
        }
        match expr.kind {
            ExprKind::MethodCall(segment, receiver, arguments, _)
                if !expr.span.from_expansion() =>
            {
                let name = segment.ident.name.as_str();
                if !TEXT_TESTS.contains(&name) {
                    return;
                }
                let from_argument = ARGUMENT_TESTS.contains(&name)
                    && arguments
                        .iter()
                        .any(|argument| self.is_error_text(cx, cx.typeck_results(), argument, 0));
                if from_argument || self.is_error_text(cx, cx.typeck_results(), receiver, 0) {
                    report(cx, &self.config, expr, expr.span);
                }
            }
            ExprKind::Binary(operator, left, right)
                if !expr.span.from_expansion()
                    && matches!(operator.node, BinOpKind::Eq | BinOpKind::Ne) =>
            {
                if self.is_error_text(cx, cx.typeck_results(), left, 0)
                    || self.is_error_text(cx, cx.typeck_results(), right, 0) {
                    report(cx, &self.config, expr, expr.span);
                }
            }
            ExprKind::Match(scrutinee, ..) if !expr.span.from_expansion() => {
                if self.is_error_text(cx, cx.typeck_results(), scrutinee, 0) {
                    report(cx, &self.config, expr, scrutinee.span);
                }
            }
            ExprKind::Match(scrutinee, ..) => {
                let Some(call) = root_macro_call(expr.span) else {
                    return;
                };
                let compares = cx
                    .tcx
                    .get_diagnostic_name(call.def_id)
                    .is_some_and(|name| COMPARE_MACROS.contains(&name.as_str()));
                if !compares {
                    return;
                }
                let ExprKind::Tup(sides) = scrutinee.kind else {
                    return;
                };
                if sides.iter().any(|side| self.is_error_text(cx, cx.typeck_results(), side, 0)) {
                    report(cx, &self.config, expr, call.span);
                }
            }
            _ => {}
        }
    }
}

impl ErrorTextMatch {
    /// Whether `expr`, typed by `typeck`, holds text made from an error by `to_string()` or
    /// `format!`. The text is followed through `let` bindings, methods that keep it, closures
    /// that map it and the return value of a function of the same crate.
    fn is_error_text<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        typeck: &'tcx TypeckResults<'tcx>,
        expr: &'tcx Expr<'tcx>,
        depth: u8,
    ) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        let expr = peel(expr);
        if macro_backtrace(expr.span)
            .any(|call| cx.tcx.is_diagnostic_item(sym::format_macro, call.def_id))
        {
            return for_each_expr(cx.tcx, expr, |part| {
                if self.is_error_type(cx, typeck.expr_ty(part)) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_some();
        }
        match expr.kind {
            ExprKind::MethodCall(segment, receiver, arguments, _) => {
                let name = segment.ident.name.as_str();
                if name == "to_string"
                    && arguments.is_empty()
                    && self.is_error_type(cx, typeck.expr_ty(receiver))
                {
                    return true;
                }
                if let ([argument], true) = (arguments, CLOSURE_MAPS.contains(&name))
                    && let ExprKind::Closure(closure) = argument.kind
                {
                    let body = cx.tcx.hir_body(closure.body);
                    return self.is_error_text(cx, typeck, body.value, depth + 1);
                }
                PASS_THROUGH.contains(&name) && self.is_error_text(cx, typeck, receiver, depth + 1)
            }
            ExprKind::Call(callee, _) => {
                let ExprKind::Path(qpath) = &callee.kind else {
                    return false;
                };
                let Res::Def(DefKind::Fn | DefKind::AssocFn, id) =
                    typeck.qpath_res(qpath, callee.hir_id)
                else {
                    return false;
                };
                let Some(local) = id.as_local() else {
                    return false;
                };
                cx.tcx.hir_maybe_body_owned_by(local).is_some_and(|body| {
                    self.is_error_text(cx, cx.tcx.typeck(local), body.value, depth + 1)
                })
            }
            ExprKind::Block(block, _) => block
                .expr
                .is_some_and(|tail| self.is_error_text(cx, typeck, tail, depth + 1)),
            ExprKind::Path(_) => expr
                .res_local_id()
                .and_then(|id| find_binding_init(cx, id))
                .is_some_and(|init| self.is_error_text(cx, typeck, init, depth + 1)),
            _ => false,
        }
    }

    /// Whether `ty`, behind any references and boxes, is an error type.
    fn is_error_type<'tcx>(&self, cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        match ty.kind() {
            ty::Adt(adt, arguments) => {
                if adt.is_box() {
                    return self.is_error_type(cx, arguments.type_at(0));
                }
                if self.anyhow_errors.contains(&adt.did()) {
                    return true;
                }
            }
            ty::Dynamic(predicates, ..) => {
                return predicates
                    .principal_def_id()
                    .is_some_and(|id| cx.tcx.is_diagnostic_item(sym::Error, id));
            }
            _ => {}
        }
        cx.tcx
            .get_diagnostic_item(sym::Error)
            .is_some_and(|error_trait| implements_trait(cx, ty, error_trait, &[]))
    }
}

/// Strips borrows, dereferences and temporaries-scope wrappers from `expr`.
fn peel<'tcx>(mut expr: &'tcx Expr<'tcx>) -> &'tcx Expr<'tcx> {
    loop {
        expr = match expr.kind {
            ExprKind::AddrOf(_, _, inner)
            | ExprKind::Unary(UnOp::Deref, inner)
            | ExprKind::DropTemps(inner) => inner,
            _ => return expr,
        };
    }
}

/// Reports `span`, with a message for test code or for the rest by where `expr` is, unless its
/// file is excluded.
fn report(cx: &LateContext<'_>, config: &Config, expr: &Expr<'_>, span: Span) {
    if config.excluded(cx, span) {
        return;
    }
    let location = cx.sess().source_map().span_to_diagnostic_string(span);
    let in_test = is_in_test(cx.tcx, expr.hir_id)
        || location.starts_with("tests/")
        || location.contains("/tests/");
    let (message, help) = if in_test {
        (
            "test checks an error by its text",
            "assert the error's type or variant, or downcast to it",
        )
    } else {
        (
            "decision taken from the text of an error",
            "match on the error's type or variant, or downcast to it",
        )
    };
    span_lint_and_help(cx, ERROR_TEXT_MATCH, span, message, None, help);
}

/// Runs the UI tests: the lint's cases, then the lint switched off, then a file excluded. They run
/// in one test, in turn, because each sets the config through the process's environment.
#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui_disabled")
        .dylint_toml(
            r"
            [error_text_match]
            enabled = false
            ",
        )
        .run();
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui_excluded")
        .dylint_toml(
            r#"
            exclude = ["ui_excluded/excluded.rs"]
            "#,
        )
        .run();
}
