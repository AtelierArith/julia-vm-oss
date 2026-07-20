//! Macro-expansion seam (Issue #8656, `docs/vm/CRATE_SPLIT.md` §4.4).
//!
//! Macro bodies are executed on a real VM (`macro_runtime.rs` compiles the
//! macro with `compile_with_cache` and runs it on `vm::Vm`), which is an
//! *upward* dependency from lowering into compile/vm.  To keep lowering-core
//! free of upward edges — the precondition for moving it below the compiler
//! in the crate split — lowering only knows this object-safe trait; the
//! VM-backed implementation lives in the integration crate root
//! (`crate::macro_runtime`) and is installed at process composition roots via
//! [`install_macro_expander`].
//!
//! Installation is idempotent and cheap (a `OnceLock` set).  If a lowering
//! entry point is reached without an installed expander, macro calls fail
//! with a clear `UnsupportedFeature` error instead of panicking; programs
//! without macro calls lower unaffected.

use std::sync::OnceLock;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Stmt};
use crate::lowering::{LambdaContext, LowerResult, StoredMacroDef};
use crate::parser::cst::{CstWalker, Node};
use crate::span::Span;

/// VM-backed macro expansion contract (CRATE_SPLIT.md §4.4).
///
/// Object-safe: methods are only generic over the CST lifetime, and the
/// implementation is stateless (all state travels in the arguments), so a
/// `&'static` instance can be installed process-wide.
pub trait MacroExpander: Sync {
    /// Expands a macro call in statement position.
    fn expand_stmt<'a>(
        &self,
        walker: &CstWalker<'a>,
        macro_name: &str,
        macro_def: &StoredMacroDef,
        args: &[Node<'a>],
        span: Span,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Stmt>;

    /// Expands a macro call in expression position.
    fn expand_expr<'a>(
        &self,
        walker: &CstWalker<'a>,
        macro_name: &str,
        macro_def: &StoredMacroDef,
        args: &[Node<'a>],
        span: Span,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Expr>;

    /// Expands a macro call in expression position and returns an expression that
    /// constructs the expansion result as a runtime value instead of executing it.
    fn expand_expr_value<'a>(
        &self,
        walker: &CstWalker<'a>,
        macro_name: &str,
        macro_def: &StoredMacroDef,
        args: &[Node<'a>],
        span: Span,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Expr>;
}

static EXPANDER: OnceLock<&'static dyn MacroExpander> = OnceLock::new();

/// Installs the process-wide macro expander.  Idempotent: the first install
/// wins; later calls are no-ops (all callers install the same VM-backed
/// implementation).
pub fn install_macro_expander(expander: &'static dyn MacroExpander) {
    let _ = EXPANDER.set(expander);
}

fn expander(span: Span) -> LowerResult<&'static dyn MacroExpander> {
    EXPANDER.get().copied().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
            "macro expansion requires an installed MacroExpander \
             (install_macro_expander was not called before lowering)",
        )
    })
}

/// Expands a macro call in statement position through the installed expander.
pub fn expand_macro_to_stmt<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    expander(span)?.expand_stmt(walker, macro_name, macro_def, args, span, lambda_ctx)
}

/// Expands a macro call in expression position through the installed expander.
pub fn expand_macro_to_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    expander(span)?.expand_expr(walker, macro_name, macro_def, args, span, lambda_ctx)
}

/// Expands a macro call and re-materializes the returned AST/value at runtime.
pub fn expand_macro_to_value_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    expander(span)?.expand_expr_value(walker, macro_name, macro_def, args, span, lambda_ctx)
}
