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
    /// Whether this module sees Base's exports through implicit or explicit
    /// non-selective `using Base`.
    pub base_exports_visible: bool,
    /// Whether ordinary `module` syntax installed implicit `eval`/`include`.
    pub implicit_standard_bindings: bool,
}

impl ModuleValue {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            exports: Vec::new(),
            publics: Vec::new(),
            base_exports_visible: true,
            implicit_standard_bindings: true,
        }
    }

    pub fn with_exports_publics(
        name: impl Into<String>,
        exports: Vec<String>,
        publics: Vec<String>,
        base_exports_visible: bool,
        implicit_standard_bindings: bool,
    ) -> Self {
        Self {
            name: name.into(),
            exports,
            publics,
            base_exports_visible,
            implicit_standard_bindings,
        }
    }
}

/// Stable identity of one Julia callable singleton type.
///
/// Upstream allocates a distinct singleton datatype for every generic function
/// (`jl_new_generic_function_with_supertype`), so the source spelling alone is
/// not an identity: a compiler-private lowering helper may legally have the
/// same spelling as a user generic (Issue #11685). Candidate indices cannot be
/// used either because REPL rebuilds relocate them. Keep the stable declaration
/// owners plus source/helper provenance, while retaining the spelling used to
/// display the value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallableSingletonIdentity {
    surface_name: String,
    owner_names: Vec<String>,
    is_lowering_helper: bool,
}

impl CallableSingletonIdentity {
    const INTERNAL_PREFIX: &'static str = "#<sjulia-callable>:";

    pub fn source(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            surface_name: name.clone(),
            owner_names: vec![name],
            is_lowering_helper: false,
        }
    }

    pub fn from_provenance(name: impl Into<String>, is_lowering_helper: bool) -> Self {
        let name = name.into();
        Self {
            surface_name: name.clone(),
            owner_names: vec![name],
            is_lowering_helper,
        }
    }

    pub fn with_owners(
        surface_name: impl Into<String>,
        mut owner_names: Vec<String>,
        is_lowering_helper: bool,
    ) -> Self {
        let surface_name = surface_name.into();
        owner_names.sort();
        owner_names.dedup();
        if owner_names.is_empty() {
            owner_names.push(surface_name.clone());
        }
        Self {
            surface_name,
            owner_names,
            is_lowering_helper,
        }
    }

    pub fn name(&self) -> &str {
        &self.surface_name
    }

    pub fn is_lowering_helper(&self) -> bool {
        self.is_lowering_helper
    }

    pub fn owner_names(&self) -> &[String] {
        &self.owner_names
    }

    /// Inner name used by the existing `typeof(...)` JuliaType projection.
    /// Ordinary source callables preserve their historical spelling. Internal
    /// identities use a tagged, length-delimited spelling; a source identifier
    /// beginning with the reserved prefix is tagged too, keeping the encoding
    /// injective for every reachable source name.
    pub fn encoded_name(&self) -> String {
        if self.is_lowering_helper {
            format!(
                "{}helper:{}:{}",
                Self::INTERNAL_PREFIX,
                self.surface_name.len(),
                self.surface_name
            )
        } else if self.surface_name.starts_with(Self::INTERNAL_PREFIX) {
            format!(
                "{}source:{}:{}",
                Self::INTERNAL_PREFIX,
                self.surface_name.len(),
                self.surface_name
            )
        } else {
            self.surface_name.clone()
        }
    }

    pub fn type_name(&self) -> String {
        format!("typeof({})", self.encoded_name())
    }

    pub fn same_callable(&self, other: &Self) -> bool {
        self.is_lowering_helper == other.is_lowering_helper && self.owner_names == other.owner_names
    }

    /// Stable dispatch-cache key. Owner names, not relocated candidate indices,
    /// distinguish same-spelled functions declared by different modules.
    pub fn dispatch_key(&self) -> String {
        let mut key = String::from("callable:");
        key.push(if self.is_lowering_helper { 'h' } else { 's' });
        for owner in &self.owner_names {
            key.push(':');
            key.push_str(&owner.len().to_string());
            key.push(':');
            key.push_str(owner);
        }
        key
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
    /// Position-independent callable singleton authority. This remains stable
    /// when `candidate_indices` are rebased after a REPL rebuild.
    singleton_identity: Rc<CallableSingletonIdentity>,
}

impl FunctionValue {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            singleton_identity: Rc::new(CallableSingletonIdentity::source(name.clone())),
            name,
            candidate_indices: None,
        }
    }

    pub fn with_candidates(name: impl Into<String>, candidate_indices: Vec<usize>) -> Self {
        let name = name.into();
        Self {
            singleton_identity: Rc::new(CallableSingletonIdentity::source(name.clone())),
            name,
            candidate_indices: Some(candidate_indices),
        }
    }

    pub fn with_candidates_and_identity(
        name: impl Into<String>,
        candidate_indices: Vec<usize>,
        singleton_identity: CallableSingletonIdentity,
    ) -> Self {
        Self {
            name: name.into(),
            candidate_indices: Some(candidate_indices),
            singleton_identity: Rc::new(singleton_identity),
        }
    }

    pub fn singleton_identity(&self) -> &CallableSingletonIdentity {
        self.singleton_identity.as_ref()
    }

    pub fn singleton_type_name(&self) -> String {
        self.singleton_identity.type_name()
    }

    pub fn singleton_dispatch_key(&self) -> String {
        self.singleton_identity.dispatch_key()
    }

    pub fn same_generic_function(&self, other: &Self) -> bool {
        self.singleton_identity
            .same_callable(&other.singleton_identity)
            || (self.singleton_identity.is_lowering_helper
                == other.singleton_identity.is_lowering_helper
                && matches!(
                    (&self.candidate_indices, &other.candidate_indices),
                    (Some(left), Some(right)) if !left.is_empty() && left == right
                ))
            || (self.name == other.name
                && self.singleton_identity.is_lowering_helper
                    == other.singleton_identity.is_lowering_helper
                && (self.candidate_indices.is_none() || other.candidate_indices.is_none()))
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
    /// Candidate function indices frozen at closure creation. As with
    /// `FunctionValue`, this is an authority boundary between Julia-visible
    /// source methods and private lowering helpers that may share a spelling
    /// (Issue #9784).
    pub candidate_indices: Option<Vec<usize>>,
    /// Stable singleton authority shared by every value from this definition
    /// site, independent of capture contents and relocated candidate indices.
    singleton_identity: Rc<CallableSingletonIdentity>,
    /// Captured variables from outer scope: (variable_name, captured_value)
    /// These values are "frozen" at closure creation time. Shared via `Rc`.
    pub captures: Rc<Vec<(String, Value)>>,
}

impl ClosureValue {
    pub fn new(name: impl Into<String>, captures: Vec<(String, Value)>) -> Self {
        let name = name.into();
        Self {
            singleton_identity: Rc::new(CallableSingletonIdentity::source(name.clone())),
            name,
            candidate_indices: None,
            captures: Rc::new(captures),
        }
    }

    pub fn with_candidates(
        name: impl Into<String>,
        captures: Vec<(String, Value)>,
        candidate_indices: Vec<usize>,
    ) -> Self {
        let name = name.into();
        Self {
            singleton_identity: Rc::new(CallableSingletonIdentity::source(name.clone())),
            name,
            candidate_indices: Some(candidate_indices),
            captures: Rc::new(captures),
        }
    }

    pub fn with_candidates_and_identity(
        name: impl Into<String>,
        captures: Vec<(String, Value)>,
        candidate_indices: Vec<usize>,
        singleton_identity: CallableSingletonIdentity,
    ) -> Self {
        Self {
            name: name.into(),
            candidate_indices: Some(candidate_indices),
            singleton_identity: Rc::new(singleton_identity),
            captures: Rc::new(captures),
        }
    }

    pub fn singleton_identity(&self) -> &CallableSingletonIdentity {
        self.singleton_identity.as_ref()
    }

    pub fn singleton_type_name(&self) -> String {
        self.singleton_identity.type_name()
    }

    pub fn singleton_dispatch_key(&self) -> String {
        self.singleton_identity.dispatch_key()
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
        assert!(m.base_exports_visible);
        assert!(m.implicit_standard_bindings);
    }

    #[test]
    fn test_module_value_with_exports_publics() {
        let m = ModuleValue::with_exports_publics(
            "MyMod",
            vec!["foo".to_string(), "bar".to_string()],
            vec!["baz".to_string()],
            false,
            false,
        );
        assert_eq!(m.name, "MyMod");
        assert_eq!(m.exports, vec!["foo", "bar"]);
        assert_eq!(m.publics, vec!["baz"]);
        assert!(!m.base_exports_visible);
        assert!(!m.implicit_standard_bindings);
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
        assert_eq!(c.candidate_indices, None);
        assert_eq!(c.captures.len(), 1);
    }

    #[test]
    fn test_closure_value_with_candidates_stores_indices() {
        let c = ClosureValue::with_candidates(
            "inner",
            vec![("x".to_string(), Value::I64(10))],
            vec![7, 9],
        );
        assert_eq!(c.candidate_indices, Some(vec![7, 9]));
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
