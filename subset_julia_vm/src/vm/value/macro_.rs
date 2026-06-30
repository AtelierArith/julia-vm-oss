//! Macro system types for Julia metaprogramming.
//!
//! These types support Julia's macro and quoting system:
//! - `SymbolValue`: Quoted identifiers (`:foo`)
//! - `LineNumberNodeValue`: Source location debug info
//! - `GlobalRefValue`: References to global variables

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

thread_local! {
    /// Thread-local Symbol intern table (Issue #5174).
    ///
    /// Holds the canonical `Rc<str>` for every interned name on this thread.
    /// `SymbolValue::new` looks a name up here and clones the canonical
    /// `Rc<str>` (a refcount bump, zero allocation) when it already exists,
    /// so repeated `:sym` construction — hot in macro / kwarg-dispatch /
    /// `Val{:sym}` paths — no longer allocates a fresh `String`.
    static SYMBOL_INTERNER: RefCell<HashSet<Rc<str>>> = RefCell::new(HashSet::new());
}

/// Intern `name`, returning the canonical shared `Rc<str>`.
///
/// Equal names always return clones of the same allocation, which makes
/// `SymbolValue` equality decidable by pointer comparison on the fast path.
fn intern(name: &str) -> Rc<str> {
    SYMBOL_INTERNER.with(|cell| {
        let mut set = cell.borrow_mut();
        if let Some(existing) = set.get(name) {
            return Rc::clone(existing);
        }
        let rc: Rc<str> = Rc::from(name);
        set.insert(Rc::clone(&rc));
        rc
    })
}

/// Julia Symbol - a quoted identifier (interned string)
///
/// In Julia: `:foo`, `Symbol("foo")`
/// Symbols are used as keys in Expr nodes and for metaprogramming.
///
/// Backed by an interned `Rc<str>` (Issue #5174): construction reuses a
/// shared allocation per distinct name and `Clone` is a refcount bump, so
/// Symbol-heavy paths avoid per-use `String` allocation. `PartialEq` takes a
/// pointer-equality fast path before falling back to a content compare.
#[derive(Debug, Clone, Eq)]
pub struct SymbolValue(Rc<str>);

impl SymbolValue {
    pub fn new(s: impl AsRef<str>) -> Self {
        Self(intern(s.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0.to_string()
    }

    /// True when both symbols share the same interned backing allocation.
    ///
    /// For interned symbols this is equivalent to value equality but is a
    /// single pointer comparison.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for SymbolValue {
    fn eq(&self, other: &Self) -> bool {
        // Fast path: interned equal names share one allocation, so a pointer
        // compare settles the common case without touching the bytes. Fall
        // back to a content compare to stay correct across threads (each
        // thread has its own interner) and for any non-interned construction.
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl std::hash::Hash for SymbolValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash by content so it stays consistent with the content-based
        // `PartialEq` fallback (and with hashing a plain `&str`).
        self.0.hash(state);
    }
}

impl std::fmt::Display for SymbolValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{}", self.0)
    }
}

/// LineNumberNode - debug information for source locations
///
/// In Julia: `LineNumberNode(42, :file.jl)`
#[derive(Debug, Clone, PartialEq)]
pub struct LineNumberNodeValue {
    pub line: i64,
    pub file: Option<String>,
}

impl LineNumberNodeValue {
    pub fn new(line: i64, file: Option<String>) -> Self {
        Self { line, file }
    }
}

impl std::fmt::Display for LineNumberNodeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "#= {}:{} =#", file, self.line),
            None => write!(f, "#= line {} =#", self.line),
        }
    }
}

/// GlobalRef - reference to a global variable in a specific module
///
/// In Julia: `GlobalRef(Main, :x)` references the variable `x` in module `Main`
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalRefValue {
    pub module: String,    // Module name (e.g., "Base", "Main")
    pub name: SymbolValue, // Variable name
}

impl GlobalRefValue {
    pub fn new(module: impl Into<String>, name: SymbolValue) -> Self {
        Self {
            module: module.into(),
            name,
        }
    }
}

impl std::fmt::Display for GlobalRefValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GlobalRef({}, :{})", self.module, self.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SymbolValue ───────────────────────────────────────────────────────────

    #[test]
    fn test_symbol_value_new_stores_string() {
        let s = SymbolValue::new("foo");
        assert_eq!(s.as_str(), "foo");
    }

    #[test]
    fn test_symbol_value_as_str_returns_inner() {
        let s = SymbolValue::new("hello");
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn test_symbol_value_into_string_consumes() {
        let s = SymbolValue::new("bar");
        assert_eq!(s.into_string(), "bar".to_string());
    }

    #[test]
    fn test_symbol_value_display_prefixes_colon() {
        let s = SymbolValue::new("xyz");
        assert_eq!(format!("{}", s), ":xyz");
    }

    #[test]
    fn test_symbol_value_equality() {
        let a = SymbolValue::new("eq");
        let b = SymbolValue::new("eq");
        let c = SymbolValue::new("neq");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── Interning (Issue #5174) ──────────────────────────────────────────────

    /// Two `SymbolValue`s built from the same name must share one backing
    /// allocation: interning makes construction allocation-free after the
    /// first occurrence.
    #[test]
    fn test_symbol_value_new_interns_same_name() {
        let a = SymbolValue::new("interned_name");
        let b = SymbolValue::new(String::from("interned_name"));
        assert!(
            a.ptr_eq(&b),
            "equal symbol names must share the interned backing allocation"
        );
    }

    /// Cloning an interned symbol is a refcount bump, not a fresh allocation.
    #[test]
    fn test_symbol_value_clone_shares_allocation() {
        let a = SymbolValue::new("clone_me");
        let b = a.clone();
        assert!(
            a.ptr_eq(&b),
            "cloning a symbol must share the same interned allocation"
        );
    }

    /// Distinct names must not collide on the same allocation.
    #[test]
    fn test_symbol_value_distinct_names_not_shared() {
        let a = SymbolValue::new("name_one");
        let b = SymbolValue::new("name_two");
        assert!(!a.ptr_eq(&b));
        assert_ne!(a, b);
    }

    // ── LineNumberNodeValue ───────────────────────────────────────────────────

    #[test]
    fn test_line_number_node_with_file_display() {
        let n = LineNumberNodeValue::new(42, Some("main.jl".to_string()));
        assert_eq!(format!("{}", n), "#= main.jl:42 =#");
    }

    #[test]
    fn test_line_number_node_without_file_display() {
        let n = LineNumberNodeValue::new(7, None);
        assert_eq!(format!("{}", n), "#= line 7 =#");
    }

    #[test]
    fn test_line_number_node_stores_line_and_file() {
        let n = LineNumberNodeValue::new(10, Some("src.jl".to_string()));
        assert_eq!(n.line, 10);
        assert_eq!(n.file.as_deref(), Some("src.jl"));
    }

    // ── GlobalRefValue ────────────────────────────────────────────────────────

    #[test]
    fn test_global_ref_value_new_stores_module_and_name() {
        let sym = SymbolValue::new("x");
        let g = GlobalRefValue::new("Main", sym);
        assert_eq!(g.module, "Main");
        assert_eq!(g.name.as_str(), "x");
    }

    #[test]
    fn test_global_ref_value_display() {
        let sym = SymbolValue::new("sqrt");
        let g = GlobalRefValue::new("Base", sym);
        assert_eq!(format!("{}", g), "GlobalRef(Base, :sqrt)");
    }
}
