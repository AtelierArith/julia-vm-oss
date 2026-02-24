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
}

impl FunctionValue {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

use super::Value;

/// Closure value - represents a Julia closure with captured variables.
/// A closure is a function that "closes over" variables from its enclosing scope.
#[derive(Debug, Clone)]
pub struct ClosureValue {
    /// The function name (typically an inner function name)
    pub name: String,
    /// Captured variables from outer scope: (variable_name, captured_value)
    /// These values are "frozen" at closure creation time.
    pub captures: Vec<(String, Value)>,
}

impl ClosureValue {
    pub fn new(name: impl Into<String>, captures: Vec<(String, Value)>) -> Self {
        Self {
            name: name.into(),
            captures,
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
    }

    #[test]
    fn test_function_value_qualified_name() {
        let f = FunctionValue::new("Base.sqrt");
        assert_eq!(f.name, "Base.sqrt");
    }

    // ── ClosureValue ──────────────────────────────────────────────────────────

    #[test]
    fn test_closure_value_new_stores_name_and_captures() {
        let c = ClosureValue::new(
            "inner",
            vec![("x".to_string(), Value::I64(10))],
        );
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
        assert!(c.get_capture("z").is_none(), "missing capture should return None");
    }

    #[test]
    fn test_closure_empty_captures() {
        let c = ClosureValue::new("f", vec![]);
        assert!(c.captures.is_empty());
        assert!(c.get_capture("x").is_none());
    }
}
