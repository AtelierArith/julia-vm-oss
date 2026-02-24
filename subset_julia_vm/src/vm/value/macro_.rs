//! Macro system types for Julia metaprogramming.
//!
//! These types support Julia's macro and quoting system:
//! - `SymbolValue`: Quoted identifiers (`:foo`)
//! - `LineNumberNodeValue`: Source location debug info
//! - `GlobalRefValue`: References to global variables

/// Julia Symbol - a quoted identifier (interned string)
///
/// In Julia: `:foo`, `Symbol("foo")`
/// Symbols are used as keys in Expr nodes and for metaprogramming.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolValue(String);

impl SymbolValue {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
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
