use std::collections::HashMap;

use crate::rng::RngInstance;
use crate::vm::{
    ArrayRef, ComposedFunctionValue, DictValue, ExprValue, FunctionValue, LineNumberNodeValue,
    NamedTupleValue, RangeValue, SymbolValue, TupleValue, TypedArrayRef, Value,
};

/// Persistent storage for REPL globals.
/// Stores variables that persist across REPL evaluations.
#[derive(Debug, Clone, Default)]
pub struct REPLGlobals {
    /// Integer variables
    pub i64_vars: HashMap<String, i64>,
    /// Float variables
    pub f64_vars: HashMap<String, f64>,
    /// Complex variables (re, im)
    pub complex_vars: HashMap<String, (f64, f64)>,
    /// String variables
    pub str_vars: HashMap<String, String>,
    /// Array variables (legacy ArrayValue)
    pub array_vars: HashMap<String, ArrayRef>,
    /// TypedArray variables (new element-typed arrays)
    pub typed_array_vars: HashMap<String, TypedArrayRef>,
    /// Range variables
    pub range_vars: HashMap<String, RangeValue>,
    /// Tuple variables
    pub tuple_vars: HashMap<String, TupleValue>,
    /// Named tuple variables
    pub named_tuple_vars: HashMap<String, NamedTupleValue>,
    /// Dict variables
    pub dict_vars: HashMap<String, Box<DictValue>>,
    /// RNG variables
    pub rng_vars: HashMap<String, RngInstance>,
    /// StructRef variables (heap indices)
    pub struct_ref_vars: HashMap<String, usize>,
    /// Function variables (first-class functions)
    pub function_vars: HashMap<String, FunctionValue>,
    /// ComposedFunction variables (f ∘ g)
    pub composed_function_vars: HashMap<String, ComposedFunctionValue>,
    /// Expr variables (metaprogramming)
    pub expr_vars: HashMap<String, ExprValue>,
    /// Symbol variables (metaprogramming)
    pub symbol_vars: HashMap<String, SymbolValue>,
    /// QuoteNode variables (metaprogramming)
    pub quotenode_vars: HashMap<String, Box<Value>>,
    /// LineNumberNode variables (metaprogramming)
    pub linenumbernode_vars: HashMap<String, LineNumberNodeValue>,
    /// Catch-all for Value variants without a dedicated typed slot.
    /// Introduced in Issue #3254; Closure added in Issue #3283.
    ///
    /// IMPORTANT: Every type stored here MUST also be handled by `value_to_literal()`
    /// in `repl/converters.rs` (which enables REPL re-injection), OR be handled
    /// specially in `inject_globals()` (like Closure via `callable_value_to_expr()`).
    /// Without this, the variable will be stored but **silently dropped** on the next
    /// REPL evaluation. See `test_all_other_vars_injectable_types_return_some()` in
    /// `repl/converters.rs` for the completeness contract (Issue #3298).
    ///
    /// Currently injectable via `value_to_literal()`:
    ///   Bool, I8–I128, U8–U128, F16 (preserved as Float16, Issue #3309), F32, Char, Regex, Enum
    /// Injected via `callable_value_to_expr()` (not `value_to_literal()`):
    ///   Closure
    /// NOT yet injectable (no Literal representation):
    ///   GlobalRef, Pairs, Set, RegexMatch, Memory (Issue #3301)
    pub other_vars: HashMap<String, Value>,
}

impl REPLGlobals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a variable by name, returning it as a Value if found.
    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(&v) = self.i64_vars.get(name) {
            return Some(Value::I64(v));
        }
        if let Some(&v) = self.f64_vars.get(name) {
            return Some(Value::F64(v));
        }
        if let Some(&(re, im)) = self.complex_vars.get(name) {
            // Use 0 as type_id for REPL session (runtime handles dispatch by struct_name)
            return Some(Value::new_complex(0, re, im));
        }
        if let Some(v) = self.str_vars.get(name) {
            return Some(Value::Str(v.clone()));
        }
        if let Some(v) = self.array_vars.get(name) {
            return Some(Value::Array(v.clone()));
        }
        if let Some(v) = self.typed_array_vars.get(name) {
            return Some(Value::Array(v.clone()));
        }
        if let Some(v) = self.range_vars.get(name) {
            return Some(Value::Range(v.clone()));
        }
        if let Some(v) = self.tuple_vars.get(name) {
            return Some(Value::Tuple(v.clone()));
        }
        if let Some(v) = self.named_tuple_vars.get(name) {
            return Some(Value::NamedTuple(v.clone()));
        }
        if let Some(v) = self.dict_vars.get(name) {
            return Some(Value::Dict(v.clone()));
        }
        if let Some(v) = self.rng_vars.get(name) {
            return Some(Value::Rng(v.clone()));
        }
        if let Some(&idx) = self.struct_ref_vars.get(name) {
            return Some(Value::StructRef(idx));
        }
        if let Some(v) = self.function_vars.get(name) {
            return Some(Value::Function(v.clone()));
        }
        if let Some(v) = self.composed_function_vars.get(name) {
            return Some(Value::ComposedFunction(v.clone()));
        }
        // Metaprogramming types
        if let Some(v) = self.expr_vars.get(name) {
            return Some(Value::Expr(v.clone()));
        }
        if let Some(v) = self.symbol_vars.get(name) {
            return Some(Value::Symbol(v.clone()));
        }
        if let Some(v) = self.quotenode_vars.get(name) {
            return Some(Value::QuoteNode(v.clone()));
        }
        if let Some(v) = self.linenumbernode_vars.get(name) {
            return Some(Value::LineNumberNode(v.clone()));
        }
        // Catch-all: Bool, I8–I128, U8–U128, F16/F32, GlobalRef, Pairs, Set,
        //            Regex, RegexMatch, Enum, Memory (Issue #3254)
        if let Some(v) = self.other_vars.get(name) {
            return Some(v.clone());
        }
        None
    }

    /// Set a variable by name from a Value.
    pub fn set(&mut self, name: &str, value: Value) {
        // Remove from all maps first to avoid type conflicts
        self.remove(name);

        // Handle Complex struct first before generic struct handling
        if let Some((re, im)) = value.as_complex_parts() {
            if value.is_complex() {
                self.complex_vars.insert(name.to_string(), (re, im));
                return;
            }
        }
        match value {
            Value::I64(v) => { self.i64_vars.insert(name.to_string(), v); }
            Value::F64(v) => { self.f64_vars.insert(name.to_string(), v); }
            Value::Str(v) => { self.str_vars.insert(name.to_string(), v); }
            Value::Array(v) => { self.array_vars.insert(name.to_string(), v); }
            Value::Range(v) => { self.range_vars.insert(name.to_string(), v); }
            Value::Tuple(v) => { self.tuple_vars.insert(name.to_string(), v); }
            Value::NamedTuple(v) => { self.named_tuple_vars.insert(name.to_string(), v); }
            Value::Dict(v) => { self.dict_vars.insert(name.to_string(), v); }
            Value::Rng(v) => { self.rng_vars.insert(name.to_string(), v); }
            Value::StructRef(idx) => { self.struct_ref_vars.insert(name.to_string(), idx); }
            Value::Function(v) => { self.function_vars.insert(name.to_string(), v); }
            // Closures are persisted via other_vars (Issue #3283).
            // callable_value_to_expr() converts them back to FunctionRef on next injection;
            // captured variables are already stored as separate globals and re-establish
            // the closure when the VM re-executes the FunctionRef in their scope.
            Value::Closure(_) => {
                self.other_vars.insert(name.to_string(), value);
            }
            Value::ComposedFunction(v) => { self.composed_function_vars.insert(name.to_string(), v); }
            // Metaprogramming types - store for REPL persistence
            Value::Expr(v) => { self.expr_vars.insert(name.to_string(), v); }
            Value::Symbol(v) => { self.symbol_vars.insert(name.to_string(), v); }
            Value::QuoteNode(v) => { self.quotenode_vars.insert(name.to_string(), v); }
            Value::LineNumberNode(v) => { self.linenumbernode_vars.insert(name.to_string(), v); }
            // Persisted via the catch-all other_vars slot (Issue #3254, #3293)
            Value::Bool(_) |
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I128(_) |
            Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) |
            Value::F16(_) | Value::F32(_) |
            Value::Char(_) |  // Literal::Char exists; value_to_literal handles it (Issue #3293)
            Value::GlobalRef(_) | Value::Pairs(_) | Value::Set(_) |
            Value::Regex(_) | Value::RegexMatch(_) |
            Value::Enum { .. } | Value::Memory(_) => {
                self.other_vars.insert(name.to_string(), value);
            }
            // These types are intentionally NOT stored as globals (Issue #3287, #3295):
            // - Nothing, Missing: Julia singletons with no user state to preserve
            // - SliceAll: internal sentinel for `a[:]` indexing; not a user variable
            // - Struct: stored as StructRef via struct_ref_vars; struct heap manages lifecycle
            // - Ref: mutable reference wrapper; cannot safely re-create across REPL steps
            // - Generator: exhaustible iterator; cannot be safely re-created
            // - DataType, Module: no Literal representation in IR; cannot inject
            // - BigInt, BigFloat: no Literal::BigInt/BigFloat injection pipeline yet (Issue #3301)
            // - Undef: compiler-internal sentinel for uninitialized variables
            // - IO: I/O handles cannot be serialized
            Value::Nothing | Value::Missing | Value::SliceAll | Value::Struct(_) |
            Value::Ref(_) | Value::Generator(_) | Value::DataType(_) |
            Value::Module(_) |
            Value::BigInt(_) | Value::BigFloat(_) | Value::Undef | Value::IO(_) => {
            }
        }
    }

    /// Remove a variable by name from all maps.
    fn remove(&mut self, name: &str) {
        self.i64_vars.remove(name);
        self.f64_vars.remove(name);
        self.complex_vars.remove(name);
        self.str_vars.remove(name);
        self.array_vars.remove(name);
        self.typed_array_vars.remove(name);
        self.range_vars.remove(name);
        self.tuple_vars.remove(name);
        self.named_tuple_vars.remove(name);
        self.dict_vars.remove(name);
        self.rng_vars.remove(name);
        self.struct_ref_vars.remove(name);
        self.function_vars.remove(name);
        self.composed_function_vars.remove(name);
        // Metaprogramming types
        self.expr_vars.remove(name);
        self.symbol_vars.remove(name);
        self.quotenode_vars.remove(name);
        self.linenumbernode_vars.remove(name);
        // Catch-all (Issue #3254)
        self.other_vars.remove(name);
    }

    /// Clear all variables.
    pub fn clear(&mut self) {
        self.i64_vars.clear();
        self.f64_vars.clear();
        self.complex_vars.clear();
        self.str_vars.clear();
        self.array_vars.clear();
        self.typed_array_vars.clear();
        self.range_vars.clear();
        self.tuple_vars.clear();
        self.named_tuple_vars.clear();
        self.dict_vars.clear();
        self.rng_vars.clear();
        self.struct_ref_vars.clear();
        self.function_vars.clear();
        self.composed_function_vars.clear();
        // Metaprogramming types
        self.expr_vars.clear();
        self.symbol_vars.clear();
        self.quotenode_vars.clear();
        self.linenumbernode_vars.clear();
        // Catch-all (Issue #3254)
        self.other_vars.clear();
    }

    /// Get all variable names.
    pub fn variable_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        names.extend(self.i64_vars.keys().cloned());
        names.extend(self.f64_vars.keys().cloned());
        names.extend(self.complex_vars.keys().cloned());
        names.extend(self.str_vars.keys().cloned());
        names.extend(self.array_vars.keys().cloned());
        names.extend(self.typed_array_vars.keys().cloned());
        names.extend(self.range_vars.keys().cloned());
        names.extend(self.tuple_vars.keys().cloned());
        names.extend(self.named_tuple_vars.keys().cloned());
        names.extend(self.dict_vars.keys().cloned());
        names.extend(self.rng_vars.keys().cloned());
        names.extend(self.struct_ref_vars.keys().cloned());
        names.extend(self.function_vars.keys().cloned());
        names.extend(self.composed_function_vars.keys().cloned());
        // Metaprogramming types
        names.extend(self.expr_vars.keys().cloned());
        names.extend(self.symbol_vars.keys().cloned());
        names.extend(self.quotenode_vars.keys().cloned());
        names.extend(self.linenumbernode_vars.keys().cloned());
        // Catch-all (Issue #3254)
        names.extend(self.other_vars.keys().cloned());
        names.sort();
        names.dedup();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_other_vars_bool_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("flag", Value::Bool(true));
        assert!(matches!(globals.get("flag"), Some(Value::Bool(true))));
    }

    #[test]
    fn test_other_vars_i32_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::I32(42));
        assert!(matches!(globals.get("x"), Some(Value::I32(42))));
    }

    #[test]
    fn test_other_vars_u64_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("n", Value::U64(u64::MAX));
        assert!(matches!(globals.get("n"), Some(Value::U64(v)) if v == u64::MAX));
    }

    #[test]
    fn test_other_vars_f32_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("f32_val", Value::F32(1.25_f32));
        assert!(matches!(globals.get("f32_val"), Some(Value::F32(v)) if (v - 1.25_f32).abs() < 1e-5));
    }

    #[test]
    fn test_other_vars_enum_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("color", Value::Enum { type_name: "Color".to_string(), value: 1 });
        let result = globals.get("color");
        assert!(
            matches!(result, Some(Value::Enum { ref type_name, value: 1 }) if type_name == "Color"),
            "Expected Enum(Color, 1), got {:?}",
            result
        );
    }

    #[test]
    fn test_other_vars_global_ref_round_trip() {
        use crate::vm::GlobalRefValue;
        let gref = GlobalRefValue {
            module: "Base".to_string(),
            name: crate::vm::SymbolValue::new("sqrt".to_string()),
        };
        let mut globals = REPLGlobals::new();
        globals.set("gref", Value::GlobalRef(gref));
        assert!(matches!(globals.get("gref"), Some(Value::GlobalRef(_))));
    }

    #[test]
    fn test_other_vars_type_change_replaces_existing() {
        // Storing a different type at the same name should replace old value
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::Bool(false));
        globals.set("x", Value::I64(99));
        // Bool is gone, I64 takes over
        assert!(matches!(globals.get("x"), Some(Value::I64(99))));
    }

    #[test]
    fn test_other_vars_appears_in_variable_names() {
        let mut globals = REPLGlobals::new();
        globals.set("flag", Value::Bool(true));
        globals.set("n", Value::I32(0));
        let names = globals.variable_names();
        assert!(names.contains(&"flag".to_string()));
        assert!(names.contains(&"n".to_string()));
    }

    #[test]
    fn test_other_vars_removed_by_remove() {
        let mut globals = REPLGlobals::new();
        globals.set("flag", Value::Bool(true));
        // Setting the same name to Nothing triggers remove + no-op
        globals.set("flag", Value::Nothing);
        assert!(globals.get("flag").is_none());
    }

    #[test]
    fn test_other_vars_cleared_by_clear() {
        let mut globals = REPLGlobals::new();
        globals.set("flag", Value::Bool(true));
        globals.clear();
        assert!(globals.get("flag").is_none());
        assert!(globals.variable_names().is_empty());
    }

    #[test]
    fn test_nothing_not_stored() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::Nothing);
        assert!(globals.get("x").is_none());
    }

    // Issue #3283: Closure persistence
    #[test]
    fn test_closure_round_trip() {
        use crate::vm::ClosureValue;
        let cv = ClosureValue::new("main#anon1", vec![("y".to_string(), Value::I64(5))]);
        let mut globals = REPLGlobals::new();
        globals.set("f", Value::Closure(cv));
        assert!(
            matches!(globals.get("f"), Some(Value::Closure(_))),
            "Expected Closure, got {:?}",
            globals.get("f")
        );
    }

    #[test]
    fn test_closure_appears_in_variable_names() {
        use crate::vm::ClosureValue;
        let cv = ClosureValue::new("main#anon1", vec![]);
        let mut globals = REPLGlobals::new();
        globals.set("f", Value::Closure(cv));
        assert!(globals.variable_names().contains(&"f".to_string()));
    }

    #[test]
    fn test_closure_removed_by_clear() {
        use crate::vm::ClosureValue;
        let cv = ClosureValue::new("main#anon1", vec![]);
        let mut globals = REPLGlobals::new();
        globals.set("f", Value::Closure(cv));
        globals.clear();
        assert!(globals.get("f").is_none());
    }

    // Issue #3293: Char persistence
    #[test]
    fn test_char_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("c", Value::Char('a'));
        assert!(
            matches!(globals.get("c"), Some(Value::Char('a'))),
            "Expected Char('a'), got {:?}",
            globals.get("c")
        );
    }

    #[test]
    fn test_char_unicode_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("c", Value::Char('α'));
        assert!(
            matches!(globals.get("c"), Some(Value::Char('α'))),
            "Expected Char('α'), got {:?}",
            globals.get("c")
        );
    }

    #[test]
    fn test_char_appears_in_variable_names() {
        let mut globals = REPLGlobals::new();
        globals.set("c", Value::Char('x'));
        assert!(globals.variable_names().contains(&"c".to_string()));
    }

    // Issue #3296: narrow int + F32 must survive the storage round-trip
    // (value_to_literal + inject_globals pipeline is tested separately in converters.rs)
    #[test]
    fn test_i32_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::I32(42));
        assert!(
            matches!(globals.get("x"), Some(Value::I32(42))),
            "Expected I32(42), got {:?}",
            globals.get("x")
        );
    }

    #[test]
    fn test_u8_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::U8(255));
        assert!(
            matches!(globals.get("x"), Some(Value::U8(255))),
            "Expected U8(255), got {:?}",
            globals.get("x")
        );
    }

    #[test]
    fn test_i128_round_trip() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::I128(i128::MAX));
        assert!(
            matches!(globals.get("x"), Some(Value::I128(v)) if v == i128::MAX),
            "Expected I128(i128::MAX), got {:?}",
            globals.get("x")
        );
    }

    #[test]
    fn test_f32_round_trip_via_other_vars() {
        let mut globals = REPLGlobals::new();
        globals.set("x", Value::F32(1.25_f32));
        assert!(
            matches!(globals.get("x"), Some(Value::F32(v)) if (v - 1.25_f32).abs() < 1e-6),
            "Expected F32(1.25), got {:?}",
            globals.get("x")
        );
    }

    // Issue #3287: Exhaustive test — every Value variant must be explicitly handled
    // by REPLGlobals::set() without panicking. This catches any future Value variant
    // that is accidentally not covered by the match arms.
    #[test]
    fn test_repl_globals_set_handles_all_value_variants_without_panic() {
        use crate::vm::value::RegexValue;
        use crate::vm::{
            ClosureValue, ComposedFunctionValue, ExprValue, FunctionValue, GlobalRefValue,
            LineNumberNodeValue, RangeValue, SymbolValue, TupleValue,
        };

        let mut globals = REPLGlobals::new();

        // Typed-map values (stored in dedicated fields)
        globals.set("i64", Value::I64(1));
        globals.set("f64", Value::F64(1.0));
        globals.set("str", Value::Str("hi".to_string()));
        globals.set("range", Value::Range(RangeValue::unit_range(1.0, 10.0)));
        globals.set(
            "tuple",
            Value::Tuple(TupleValue::new(vec![Value::I64(1)])),
        );
        globals.set("structref", Value::StructRef(0));
        globals.set("func", Value::Function(FunctionValue::new("f")));
        globals.set(
            "composed",
            Value::ComposedFunction(ComposedFunctionValue::new(
                Value::Function(FunctionValue::new("f")),
                Value::Function(FunctionValue::new("g")),
            )),
        );
        globals.set(
            "expr",
            Value::Expr(ExprValue::new(
                SymbolValue::new("call".to_string()),
                vec![Value::Symbol(SymbolValue::new("x".to_string()))],
            )),
        );
        globals.set("sym", Value::Symbol(SymbolValue::new("x".to_string())));
        globals.set("qn", Value::QuoteNode(Box::new(Value::I64(1))));
        globals.set(
            "lnn",
            Value::LineNumberNode(LineNumberNodeValue::new(
                1,
                Some("test.jl".to_string()),
            )),
        );

        // other_vars values (stored in catch-all)
        globals.set("bool", Value::Bool(true));
        globals.set("i8", Value::I8(1));
        globals.set("i16", Value::I16(1));
        globals.set("i32", Value::I32(1));
        globals.set("i128", Value::I128(1));
        globals.set("u8", Value::U8(1));
        globals.set("u16", Value::U16(1));
        globals.set("u32", Value::U32(1));
        globals.set("u64", Value::U64(1));
        globals.set("u128", Value::U128(1));
        globals.set("f16", Value::F16(half::f16::from_f32(1.0)));
        globals.set("f32", Value::F32(1.0));
        globals.set("char", Value::Char('a'));
        globals.set(
            "regex",
            Value::Regex(RegexValue::new("test", "").expect("valid regex")),
        );
        globals.set(
            "enum_val",
            Value::Enum {
                type_name: "T".to_string(),
                value: 0,
            },
        );
        globals.set("closure", Value::Closure(ClosureValue::new("f", vec![])));
        globals.set(
            "gref",
            Value::GlobalRef(GlobalRefValue::new(
                "M",
                SymbolValue::new("x".to_string()),
            )),
        );

        // Intentionally not stored (should not appear in variable_names)
        globals.set("nothing", Value::Nothing);
        globals.set("missing", Value::Missing);
        globals.set("undef", Value::Undef);

        // Verify stored values are retrievable
        assert!(globals.get("i64").is_some(), "I64 should be stored");
        assert!(globals.get("bool").is_some(), "Bool should be stored");
        assert!(globals.get("char").is_some(), "Char should be stored");
        assert!(
            globals.get("enum_val").is_some(),
            "Enum should be stored"
        );
        assert!(
            globals.get("closure").is_some(),
            "Closure should be stored"
        );
        assert!(globals.get("gref").is_some(), "GlobalRef should be stored");

        // Verify intentionally not stored
        assert!(
            globals.get("nothing").is_none(),
            "Nothing should NOT be stored"
        );
        assert!(
            globals.get("missing").is_none(),
            "Missing should NOT be stored"
        );
        assert!(
            globals.get("undef").is_none(),
            "Undef should NOT be stored"
        );
    }

    // Issue #3299: Regex persistence
    #[test]
    fn test_regex_round_trip() {
        use crate::vm::value::RegexValue;
        let rv = RegexValue::new("hello", "").unwrap();
        let mut globals = REPLGlobals::new();
        globals.set("re", Value::Regex(rv));
        assert!(
            matches!(globals.get("re"), Some(Value::Regex(_))),
            "Expected Regex, got {:?}",
            globals.get("re")
        );
    }
}

/// Result of a REPL evaluation.
#[derive(Debug)]
pub struct REPLResult {
    /// Whether the evaluation succeeded
    pub success: bool,
    /// The result value (if successful and not Nothing)
    pub value: Option<Value>,
    /// Output from println/print calls
    pub output: String,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl REPLResult {
    pub fn success(value: Value, output: String) -> Self {
        let value = match &value {
            Value::Nothing => None,
            v => Some(v.clone()),
        };
        Self {
            success: true,
            value,
            output,
            error: None,
        }
    }

    pub fn error(message: String, output: String) -> Self {
        Self {
            success: false,
            value: None,
            output,
            error: Some(message),
        }
    }
}
