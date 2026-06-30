//! Module and function metadata types.

/// Module value - represents a Julia module
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleValue {
    /// Module name (e.g., "Statistics", "Base")
    pub name: String,
    /// Exported symbols (available via `using Module`)
    pub exports: Vec<String>,
    /// Public symbols (Julia 1.11+, part of public API but not auto-exported)
    pub publics: Vec<String>,
}

impl ModuleValue {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            exports: Vec::new(),
            publics: Vec::new(),
        }
    }

    pub fn with_exports_publics(
        name: impl Into<String>,
        exports: Vec<String>,
        publics: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            exports,
            publics,
        }
    }
}

/// Function value - represents a Julia function object
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionValue {
    /// Function name (e.g., "gcd", "Base.sqrt")
    pub name: String,
    /// Candidate function indices captured when the function value was created
    /// from a resolved binding. Bare names can collide across modules (for
    /// example `Base.Iterators.flatten` and `MacroTools.flatten`), so HOF/runtime
    /// calls use this set when available instead of rediscovering by name.
    pub candidate_indices: Option<Vec<usize>>,
}

impl FunctionValue {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            candidate_indices: None,
        }
    }

    pub fn with_candidates(name: impl Into<String>, candidate_indices: Vec<usize>) -> Self {
        Self {
            name: name.into(),
            candidate_indices: Some(candidate_indices),
        }
    }
}

use super::Value;
use std::rc::Rc;

/// Closure value - represents a Julia closure with captured variables.
/// A closure is a function that "closes over" variables from its enclosing scope.
///
/// Issue #5189: the captured environment lives behind an `Rc` so that cloning a
/// `ClosureValue` — which happens at every closure call site, once per HOF
/// iteration (e.g. `map(x -> a*x + b, big_arr)`) — is an O(1) refcount bump that
/// shares the capture storage, instead of deep-cloning the whole
/// `Vec<(String, Value)>`. The capture set is frozen at creation, so sharing it
/// immutably across calls is safe.
#[derive(Debug, Clone)]
pub struct ClosureValue {
    /// The function name (typically an inner function name)
    pub name: String,
    /// Captured variables from outer scope: (variable_name, captured_value)
    /// These values are "frozen" at closure creation time. Shared via `Rc`.
    pub captures: Rc<Vec<(String, Value)>>,
}

impl ClosureValue {
    pub fn new(name: impl Into<String>, captures: Vec<(String, Value)>) -> Self {
        Self {
            name: name.into(),
            captures: Rc::new(captures),
        }
    }

    /// Get a captured variable by name
    pub fn get_capture(&self, name: &str) -> Option<&Value> {
        self.captures
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ModuleValue ───────────────────────────────────────────────────────────

    #[test]
    fn test_module_value_new_stores_name() {
        let m = ModuleValue::new("Statistics");
        assert_eq!(m.name, "Statistics");
        assert!(m.exports.is_empty(), "new() should have empty exports");
        assert!(m.publics.is_empty(), "new() should have empty publics");
    }

    #[test]
    fn test_module_value_with_exports_publics() {
        let m = ModuleValue::with_exports_publics(
            "MyMod",
            vec!["foo".to_string(), "bar".to_string()],
            vec!["baz".to_string()],
        );
        assert_eq!(m.name, "MyMod");
        assert_eq!(m.exports, vec!["foo", "bar"]);
        assert_eq!(m.publics, vec!["baz"]);
    }

    // ── FunctionValue ─────────────────────────────────────────────────────────

    #[test]
    fn test_function_value_new_stores_name() {
        let f = FunctionValue::new("gcd");
        assert_eq!(f.name, "gcd");
        assert_eq!(f.candidate_indices, None);
    }

    #[test]
    fn test_function_value_with_candidates_stores_indices() {
        let f = FunctionValue::with_candidates("flatten", vec![10, 11]);
        assert_eq!(f.name, "flatten");
        assert_eq!(f.candidate_indices, Some(vec![10, 11]));
    }

    #[test]
    fn test_function_value_qualified_name() {
        let f = FunctionValue::new("Base.sqrt");
        assert_eq!(f.name, "Base.sqrt");
    }

    // ── ClosureValue ──────────────────────────────────────────────────────────

    #[test]
    fn test_closure_value_new_stores_name_and_captures() {
        let c = ClosureValue::new("inner", vec![("x".to_string(), Value::I64(10))]);
        assert_eq!(c.name, "inner");
        assert_eq!(c.captures.len(), 1);
    }

    #[test]
    fn test_closure_get_capture_existing_variable() {
        let c = ClosureValue::new(
            "f",
            vec![
                ("x".to_string(), Value::I64(42)),
                ("y".to_string(), Value::Bool(true)),
            ],
        );
        assert!(matches!(c.get_capture("x"), Some(Value::I64(42))));
        assert!(matches!(c.get_capture("y"), Some(Value::Bool(true))));
    }

    #[test]
    fn test_closure_get_capture_missing_returns_none() {
        let c = ClosureValue::new("f", vec![("x".to_string(), Value::I64(1))]);
        assert!(
            c.get_capture("z").is_none(),
            "missing capture should return None"
        );
    }

    #[test]
    fn test_closure_empty_captures() {
        let c = ClosureValue::new("f", vec![]);
        assert!(c.captures.is_empty());
        assert!(c.get_capture("x").is_none());
    }

    // Issue #5189: captures are stored behind an `Rc` so cloning a closure
    // (which happens on every closure call site, per HOF iteration) is an O(1)
    // refcount bump that shares the capture storage instead of deep-cloning the
    // whole `Vec<(String, Value)>`.
    #[test]
    fn test_closure_clone_shares_captures_via_rc() {
        let c = ClosureValue::new(
            "inner",
            vec![
                ("a".to_string(), Value::I64(2)),
                ("b".to_string(), Value::I64(3)),
            ],
        );
        // Fresh closure: exactly one strong reference to the capture storage.
        assert_eq!(std::rc::Rc::strong_count(&c.captures), 1);

        // Cloning the closure must NOT deep-copy the captures; it bumps the
        // shared `Rc` refcount and both closures point at the same allocation.
        let c2 = c.clone();
        assert_eq!(std::rc::Rc::strong_count(&c.captures), 2);
        assert!(std::rc::Rc::ptr_eq(&c.captures, &c2.captures));

        drop(c2);
        assert_eq!(std::rc::Rc::strong_count(&c.captures), 1);
    }
}
