//! Thread-local flag marking the `@generated` syntactic-unquote lowering path.
//!
//! The `@generated` unquote helpers in `stmt::control_if` lower a quote's inner
//! expression as *plain code* (so `:($N + 1)` becomes the runnable `N + 1`).
//! Plain-code lowering normally rejects the interpolation operator `$` with
//! `UnsupportedOperator("$")` because `$ident` is only valid inside a quote.
//!
//! Within the unquote path, however, `$N` must resolve to the bound type/value
//! parameter `N` (matching upstream Julia, where `:($N + 1)` evaluates to
//! `N + 1` at staging time — Issue #5934). This flag lets `lower_unary_expr`
//! detect the `$ident` shape and lower it to the bare inner identifier *only*
//! while the unquote helpers are active, so `$N` written outside a quote keeps
//! erroring exactly as before.
//!
//! Scope: `$ident` only. Compound `$(expr)`, `$(esc(...))`, and `$(p...)` splat
//! forms remain unsupported here (a separate follow-up — the real staging
//! engine).

use std::cell::Cell;

thread_local! {
    static IN_GENERATED_UNQUOTE: Cell<bool> = const { Cell::new(false) };
}

/// True while lowering inside a `@generated` syntactic-unquote body.
pub(crate) fn is_active() -> bool {
    IN_GENERATED_UNQUOTE.with(|f| f.get())
}

/// RAII guard that marks the generated-unquote lowering path active for its
/// lifetime and restores the previous state on drop (so nested lowering — e.g.
/// a non-generated function lowered while the flag happens to be set — sees the
/// correct value).
pub(crate) struct UnquoteScope {
    prev: bool,
}

impl UnquoteScope {
    /// Enter the generated-unquote lowering path, returning a guard that
    /// restores the previous flag value on drop.
    pub(crate) fn enter() -> Self {
        let prev = IN_GENERATED_UNQUOTE.with(|f| f.replace(true));
        UnquoteScope { prev }
    }
}

impl Drop for UnquoteScope {
    fn drop(&mut self) {
        IN_GENERATED_UNQUOTE.with(|f| f.set(self.prev));
    }
}
