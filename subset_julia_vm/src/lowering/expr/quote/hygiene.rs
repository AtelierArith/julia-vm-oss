//! Macro hygiene context for quote expression handling.
//!
//! This module implements the HygieneContext which tracks variable renaming
//! to prevent name collisions in macro expansions.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use super::super::macros::GENSYM_COUNTER;

/// Context for macro hygiene - tracks variable renaming to prevent name collisions.
#[derive(Debug, Clone, Default)]
pub(in crate::lowering) struct HygieneContext {
    /// Map from original variable name to gensym'd name.
    /// Variables introduced by the macro (not escaped) get renamed.
    /// Wrapped in Rc for O(1) sharing in enter_escaped() via Rc::clone().
    renames: Rc<HashMap<String, String>>,
    /// Are we currently inside an escaped expression (esc(...))?
    /// Variables inside escaped expressions are NOT renamed.
    in_escaped: bool,
}

impl HygieneContext {
    pub(in crate::lowering) fn new() -> Self {
        Self::default()
    }

    /// Generate a unique symbol name using gensym.
    pub(super) fn gensym(base: &str) -> String {
        let counter = GENSYM_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("#{}#{}", base, counter)
    }

    /// Register a local variable for renaming (if not in escaped context).
    /// Returns the new name (gensym'd or original if escaped).
    pub(in crate::lowering) fn register_local(&mut self, name: &str) -> String {
        if self.in_escaped {
            name.to_string()
        } else {
            let new_name = Self::gensym(name);
            Rc::make_mut(&mut self.renames).insert(name.to_string(), new_name.clone());
            new_name
        }
    }

    /// Resolve a variable name - returns gensym'd name if it was registered.
    pub(in crate::lowering) fn resolve(&self, name: &str) -> String {
        if self.in_escaped {
            name.to_string()
        } else {
            self.renames
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.to_string())
        }
    }

    /// Enter an escaped context (for processing esc(...) expressions).
    pub(in crate::lowering) fn enter_escaped(&self) -> Self {
        Self {
            renames: Rc::clone(&self.renames),
            in_escaped: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── HygieneContext::new ───────────────────────────────────────────────────

    #[test]
    fn test_new_starts_empty_and_not_escaped() {
        let ctx = HygieneContext::new();
        assert!(!ctx.in_escaped, "New context should not be in escaped state");
        assert!(ctx.renames.is_empty(), "New context should have no renames");
    }

    // ── HygieneContext::gensym ────────────────────────────────────────────────

    #[test]
    fn test_gensym_format_contains_base_name() {
        let result = HygieneContext::gensym("x");
        assert!(
            result.starts_with("#x#"),
            "Expected gensym result to start with '#x#', got {:?}",
            result
        );
    }

    #[test]
    fn test_gensym_consecutive_calls_produce_unique_names() {
        let first = HygieneContext::gensym("y");
        let second = HygieneContext::gensym("y");
        assert_ne!(first, second, "Consecutive gensym calls should produce unique names");
    }

    // ── HygieneContext::register_local ────────────────────────────────────────

    #[test]
    fn test_register_local_not_escaped_returns_gensym_name() {
        let mut ctx = HygieneContext::new();
        let result = ctx.register_local("foo");
        assert!(
            result.starts_with("#foo#"),
            "Expected gensym'd name starting with '#foo#', got {:?}",
            result
        );
    }

    #[test]
    fn test_register_local_not_escaped_stores_rename() {
        let mut ctx = HygieneContext::new();
        let new_name = ctx.register_local("bar");
        assert_eq!(
            ctx.renames.get("bar"),
            Some(&new_name),
            "register_local should store the rename mapping"
        );
    }

    #[test]
    fn test_register_local_in_escaped_returns_original_name() {
        let mut ctx = HygieneContext::new();
        ctx.in_escaped = true;
        let result = ctx.register_local("baz");
        assert_eq!(result, "baz", "In escaped context, register_local should return original name");
        assert!(ctx.renames.is_empty(), "In escaped context, no rename should be stored");
    }

    // ── HygieneContext::resolve ───────────────────────────────────────────────

    #[test]
    fn test_resolve_registered_name_returns_gensym() {
        let mut ctx = HygieneContext::new();
        let gensym_name = ctx.register_local("abc");
        let resolved = ctx.resolve("abc");
        assert_eq!(resolved, gensym_name, "resolve should return the gensym'd name for a registered variable");
    }

    #[test]
    fn test_resolve_unregistered_name_returns_original() {
        let ctx = HygieneContext::new();
        let resolved = ctx.resolve("unknown");
        assert_eq!(resolved, "unknown", "resolve should return original name for unregistered variable");
    }

    #[test]
    fn test_resolve_in_escaped_context_returns_original_even_if_registered() {
        let mut ctx = HygieneContext::new();
        ctx.register_local("x");
        let escaped = ctx.enter_escaped();
        // Even though "x" was registered in the original context, resolving in
        // escaped context should return the original name
        let resolved = escaped.resolve("x");
        assert_eq!(resolved, "x", "In escaped context, resolve should return original name");
    }

    // ── HygieneContext::enter_escaped ─────────────────────────────────────────

    #[test]
    fn test_enter_escaped_sets_in_escaped_true() {
        let ctx = HygieneContext::new();
        let escaped = ctx.enter_escaped();
        assert!(escaped.in_escaped, "enter_escaped should set in_escaped to true");
    }

    #[test]
    fn test_enter_escaped_preserves_existing_renames() {
        let mut ctx = HygieneContext::new();
        let gensym_name = ctx.register_local("myvar");
        let escaped = ctx.enter_escaped();
        // Renames should still be present (even if not used in escaped context)
        assert_eq!(
            escaped.renames.get("myvar"),
            Some(&gensym_name),
            "enter_escaped should preserve existing renames"
        );
    }
}
