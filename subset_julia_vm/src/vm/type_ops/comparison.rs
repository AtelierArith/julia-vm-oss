//! Type comparison and subtype checking.

use crate::rng::RngLike;
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Check if a struct is an instance of a user-defined abstract type.
    /// Uses the pre-computed `type_ancestors` map (Issue #3356) for O(1) lookup.
    pub(in crate::vm) fn check_isa_with_abstract_resolved(
        &self,
        struct_name_opt: &Option<String>,
        target_type: &str,
    ) -> bool {
        let struct_name = match struct_name_opt {
            Some(name) => name,
            None => return false,
        };

        if let Some(ancestor_list) = self.type_ancestors.get(struct_name.as_str()) {
            return ancestor_list.iter().any(|a| a == target_type);
        }

        false
    }

    /// Check if left_type <: right_type (left is a subtype of right).
    ///
    /// This is the **runtime** (string-based) counterpart of
    /// `JuliaType::is_subtype_of()` in `types/julia_type.rs` (compile-time, enum-based).
    /// Both implementations must cover the same type hierarchy. When adding new
    /// types, update both and run `test_check_subtype_parity_with_julia_type`. (Issue #2494)
    pub(in crate::vm) fn check_subtype(&self, left_type: &str, right_type: &str) -> bool {
        // Exact match
        if left_type == right_type {
            return true;
        }

        // Any is the top type - everything is a subtype of Any
        if right_type == "Any" {
            return true;
        }

        // Nothing/Union{} is the bottom type - Nothing is a subtype of everything
        if left_type == "Nothing" || left_type == "Union{}" {
            return true;
        }

        // Handle Union types on the right: T <: Union{A, B} iff T <: A or T <: B
        if right_type.starts_with("Union{") && right_type.ends_with('}') {
            let inner = &right_type[6..right_type.len() - 1];
            if inner.is_empty() {
                // T <: Union{} is false (except for Nothing/Union{} handled above)
                return false;
            }
            // Parse union members and check if left_type is subtype of any
            let members = parse_union_members(inner);
            return members.iter().any(|m| self.check_subtype(left_type, m));
        }

        // Handle Union types on the left: Union{A, B} <: T iff A <: T and B <: T
        if left_type.starts_with("Union{") && left_type.ends_with('}') {
            let inner = &left_type[6..left_type.len() - 1];
            if inner.is_empty() {
                // Union{} <: T is always true (handled above)
                return true;
            }
            let members = parse_union_members(inner);
            return members.iter().all(|m| self.check_subtype(m, right_type));
        }

        // Check Julia's built-in type hierarchy
        //
        // Julia numeric type hierarchy:
        //   Number
        //   ├── Complex{T}          (struct, <: Number but NOT <: Real)
        //   └── Real
        //       ├── AbstractFloat
        //       │   ├── Float16
        //       │   ├── Float32
        //       │   ├── Float64
        //       │   └── BigFloat
        //       ├── Rational{T}     (struct, <: Real <: Number)
        //       └── Integer
        //           ├── Signed
        //           │   ├── Int8
        //           │   ├── Int16
        //           │   ├── Int32
        //           │   ├── Int64
        //           │   ├── Int128
        //           │   └── BigInt
        //           ├── Unsigned
        //           │   ├── UInt8
        //           │   ├── UInt16
        //           │   ├── UInt32
        //           │   ├── UInt64
        //           │   └── UInt128
        //           └── Bool        (Bool <: Integer, but NOT Signed or Unsigned)
        //
        // When adding new numeric types, update ALL relevant arms below and add
        // to the test_check_subtype_all_numeric_types unit test.
        match (left_type, right_type) {
            // Signed integers: Int* <: Signed <: Integer <: Real <: Number
            (
                "Int64" | "Int32" | "Int16" | "Int8" | "Int128" | "BigInt",
                "Signed" | "Integer" | "Real" | "Number",
            ) => true,
            // Unsigned integers: UInt* <: Unsigned <: Integer <: Real <: Number
            (
                "UInt64" | "UInt32" | "UInt16" | "UInt8" | "UInt128",
                "Unsigned" | "Integer" | "Real" | "Number",
            ) => true,
            // Bool <: Integer <: Real <: Number (NOT Signed or Unsigned)
            ("Bool", "Integer" | "Real" | "Number") => true,
            // Floats: Float* <: AbstractFloat <: Real <: Number
            (
                "Float64" | "Float32" | "Float16" | "BigFloat",
                "AbstractFloat" | "Real" | "Number",
            ) => true,
            // Abstract type chains
            ("Integer", "Real" | "Number") => true,
            ("Signed", "Integer" | "Real" | "Number") => true,
            ("Unsigned", "Integer" | "Real" | "Number") => true,
            ("AbstractFloat", "Real" | "Number") => true,
            ("Real", "Number") => true,

            // Complex{T} <: Number (but NOT <: Real)
            // Handle all parametric variants: Complex{Bool}, Complex{Float32}, etc.
            (val, "Number") if val.starts_with("Complex") => true,

            // Rational{T} <: Real <: Number
            (val, "Real" | "Number") if val.starts_with("Rational") => true,

            // String hierarchy
            ("String", "AbstractString") => true,

            // Array hierarchy
            (val, "AbstractArray" | "AbstractVector" | "AbstractMatrix")
                if val.starts_with("Vector{")
                    || val.starts_with("Matrix{")
                    || val.starts_with("Array{") =>
            {
                true
            }

            // Range hierarchy
            (val, "AbstractRange")
                if val.starts_with("UnitRange") || val.starts_with("StepRange") =>
            {
                true
            }

            // IO hierarchy: IOBuffer <: IO
            ("IOBuffer", "IO") => true,

            // Type hierarchy: DataType <: Type
            // In Julia, all type objects are instances of Type
            ("DataType", "Type") => true,

            // Tuple covariant subtyping (Issue #2524):
            // Tuple{Int64} <: Tuple{Any}, Tuple{Int64} <: Tuple{Number}
            _ if left_type.starts_with("Tuple{") && right_type.starts_with("Tuple{") => {
                self.check_tuple_covariant_subtype(left_type, right_type)
            }

            // Tuple{...} <: Tuple (any parametric tuple is subtype of bare Tuple)
            (val, "Tuple") if val.starts_with("Tuple{") => true,

            // Check user-defined abstract type hierarchy
            _ => {
                // Check if left_type has right_type as an ancestor in the abstract type hierarchy
                self.check_abstract_type_hierarchy(left_type, right_type)
            }
        }
    }

    /// Check covariant Tuple subtyping (Issue #2524).
    /// Tuple{T1, T2} <: Tuple{S1, S2} iff T1 <: S1 AND T2 <: S2 (element-wise).
    fn check_tuple_covariant_subtype(&self, left_type: &str, right_type: &str) -> bool {
        let left_params = crate::vm::util::parse_parametric_params(left_type);
        let right_params = crate::vm::util::parse_parametric_params(right_type);

        // Arity must match
        if left_params.len() != right_params.len() {
            return false;
        }

        // Element-wise covariant check
        left_params
            .iter()
            .zip(right_params.iter())
            .all(|(l, r)| self.check_subtype(l, r))
    }

    /// Check if left_type has right_type as an ancestor in the abstract type hierarchy.
    /// Supports parametric abstract types (Issue #2523): e.g., `Container{Int64} <: Container`.
    ///
    /// Uses pre-computed `type_ancestors` map (Issue #3356) for O(1) lookup.
    pub(in crate::vm) fn check_abstract_type_hierarchy(&self, left_type: &str, right_type: &str) -> bool {
        fn base_name(s: &str) -> &str {
            s.find('{').map(|idx| &s[..idx]).unwrap_or(s)
        }

        let left_base = base_name(left_type);

        // Parametric base match: Complex{Bool} <: Complex
        if left_type != left_base && left_base == right_type {
            return true;
        }

        // Check pre-computed ancestors for the exact type name first,
        // then fall back to the base name (handles parametric structs like "Complex{Bool}")
        if let Some(ancestor_list) = self
            .type_ancestors
            .get(left_type)
            .or_else(|| self.type_ancestors.get(left_base))
        {
            return ancestor_list.iter().any(|a| a == right_type);
        }

        false
    }
}


/// Parse union type members, respecting nested braces.
/// "Int64, Float64" -> vec!["Int64", "Float64"]
/// "Int64, Complex{Float64}" -> vec!["Int64", "Complex{Float64}"]
fn parse_union_members(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                let arg = s[start..i].trim();
                if !arg.is_empty() {
                    result.push(arg);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last argument
    let last = s[start..].trim();
    if !last.is_empty() {
        result.push(last);
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::Vm;

    /// Helper: create a minimal VM for testing check_subtype.
    fn make_vm() -> Vm<StableRng> {
        Vm::new(vec![], StableRng::new(0))
    }

    /// Verify concrete signed integer types are subtypes of Signed, Integer, Real, Number.
    #[test]
    fn test_check_subtype_signed_integers() {
        let vm = make_vm();
        for ty in &["Int8", "Int16", "Int32", "Int64", "Int128"] {
            assert!(vm.check_subtype(ty, "Signed"), "{ty} should be <: Signed");
            assert!(vm.check_subtype(ty, "Integer"), "{ty} should be <: Integer");
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Signed integers are NOT Unsigned or AbstractFloat
            assert!(
                !vm.check_subtype(ty, "Unsigned"),
                "{ty} should NOT be <: Unsigned"
            );
            assert!(
                !vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should NOT be <: AbstractFloat"
            );
        }
    }

    /// Verify BigInt subtype limitations.
    /// In Julia, BigInt <: Signed <: Integer <: Real <: Number, but adding
    /// any BigInt subtype relationship causes dispatch regressions in
    /// convert/promote paths. BigInt relies on exact-match dispatch.
    #[test]
    fn test_check_subtype_bigint() {
        let vm = make_vm();
        // BigInt <: Signed <: Integer <: Real <: Number (Issue #2492)
        assert!(
            vm.check_subtype("BigInt", "Signed"),
            "BigInt should be <: Signed"
        );
        assert!(
            vm.check_subtype("BigInt", "Integer"),
            "BigInt should be <: Integer"
        );
        assert!(
            vm.check_subtype("BigInt", "Real"),
            "BigInt should be <: Real"
        );
        assert!(
            vm.check_subtype("BigInt", "Number"),
            "BigInt should be <: Number"
        );
        // Negative cases
        assert!(
            !vm.check_subtype("BigInt", "Unsigned"),
            "BigInt should NOT be <: Unsigned"
        );
        assert!(
            !vm.check_subtype("BigInt", "AbstractFloat"),
            "BigInt should NOT be <: AbstractFloat"
        );
        // BigInt <: BigInt (reflexive)
        assert!(
            vm.check_subtype("BigInt", "BigInt"),
            "BigInt should be <: BigInt"
        );
        // BigInt <: Any
        assert!(vm.check_subtype("BigInt", "Any"), "BigInt should be <: Any");
    }

    /// Verify all concrete unsigned integer types are subtypes of Unsigned, Integer, Real, Number.
    #[test]
    fn test_check_subtype_unsigned_integers() {
        let vm = make_vm();
        for ty in &["UInt8", "UInt16", "UInt32", "UInt64", "UInt128"] {
            assert!(
                vm.check_subtype(ty, "Unsigned"),
                "{ty} should be <: Unsigned"
            );
            assert!(vm.check_subtype(ty, "Integer"), "{ty} should be <: Integer");
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Unsigned integers are NOT Signed or AbstractFloat
            assert!(
                !vm.check_subtype(ty, "Signed"),
                "{ty} should NOT be <: Signed"
            );
            assert!(
                !vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should NOT be <: AbstractFloat"
            );
        }
    }

    /// Verify Bool <: Integer <: Real <: Number, but NOT Signed or Unsigned.
    #[test]
    fn test_check_subtype_bool() {
        let vm = make_vm();
        assert!(
            vm.check_subtype("Bool", "Integer"),
            "Bool should be <: Integer"
        );
        assert!(vm.check_subtype("Bool", "Real"), "Bool should be <: Real");
        assert!(
            vm.check_subtype("Bool", "Number"),
            "Bool should be <: Number"
        );
        // In Julia, Bool is NOT <: Signed and NOT <: Unsigned
        assert!(
            !vm.check_subtype("Bool", "Signed"),
            "Bool should NOT be <: Signed"
        );
        assert!(
            !vm.check_subtype("Bool", "Unsigned"),
            "Bool should NOT be <: Unsigned"
        );
        assert!(
            !vm.check_subtype("Bool", "AbstractFloat"),
            "Bool should NOT be <: AbstractFloat"
        );
    }

    /// Verify all concrete float types are subtypes of AbstractFloat, Real, Number.
    #[test]
    fn test_check_subtype_floats() {
        let vm = make_vm();
        for ty in &["Float16", "Float32", "Float64", "BigFloat"] {
            assert!(
                vm.check_subtype(ty, "AbstractFloat"),
                "{ty} should be <: AbstractFloat"
            );
            assert!(vm.check_subtype(ty, "Real"), "{ty} should be <: Real");
            assert!(vm.check_subtype(ty, "Number"), "{ty} should be <: Number");
            // Floats are NOT Integer, Signed, or Unsigned
            assert!(
                !vm.check_subtype(ty, "Integer"),
                "{ty} should NOT be <: Integer"
            );
            assert!(
                !vm.check_subtype(ty, "Signed"),
                "{ty} should NOT be <: Signed"
            );
            assert!(
                !vm.check_subtype(ty, "Unsigned"),
                "{ty} should NOT be <: Unsigned"
            );
        }
    }

    /// Verify abstract type chain relationships.
    #[test]
    fn test_check_subtype_abstract_chain() {
        let vm = make_vm();
        // Signed <: Integer <: Real <: Number
        assert!(vm.check_subtype("Signed", "Integer"));
        assert!(vm.check_subtype("Signed", "Real"));
        assert!(vm.check_subtype("Signed", "Number"));
        // Unsigned <: Integer <: Real <: Number
        assert!(vm.check_subtype("Unsigned", "Integer"));
        assert!(vm.check_subtype("Unsigned", "Real"));
        assert!(vm.check_subtype("Unsigned", "Number"));
        // Integer <: Real <: Number
        assert!(vm.check_subtype("Integer", "Real"));
        assert!(vm.check_subtype("Integer", "Number"));
        // AbstractFloat <: Real <: Number
        assert!(vm.check_subtype("AbstractFloat", "Real"));
        assert!(vm.check_subtype("AbstractFloat", "Number"));
        // Real <: Number
        assert!(vm.check_subtype("Real", "Number"));
    }

    /// Verify reflexive property: T <: T for all types.
    #[test]
    fn test_check_subtype_reflexive() {
        let vm = make_vm();
        for ty in &[
            "Int8",
            "Int16",
            "Int32",
            "Int64",
            "Int128",
            "BigInt",
            "UInt8",
            "UInt16",
            "UInt32",
            "UInt64",
            "UInt128",
            "Bool",
            "Float16",
            "Float32",
            "Float64",
            "BigFloat",
            "Integer",
            "Signed",
            "Unsigned",
            "Real",
            "Number",
            "AbstractFloat",
            "String",
            "Any",
        ] {
            assert!(vm.check_subtype(ty, ty), "{ty} should be <: {ty}");
        }
    }

    /// Verify everything is <: Any and Nothing is <: everything.
    #[test]
    fn test_check_subtype_any_and_nothing() {
        let vm = make_vm();
        for ty in &["Int64", "Float64", "Bool", "String", "Integer", "Number"] {
            assert!(vm.check_subtype(ty, "Any"), "{ty} should be <: Any");
            assert!(vm.check_subtype("Nothing", ty), "Nothing should be <: {ty}");
        }
    }

    /// Verify Complex and Rational subtype relationships.
    #[test]
    fn test_check_subtype_complex_rational() {
        let vm = make_vm();
        // Complex <: Number but NOT <: Real
        assert!(vm.check_subtype("Complex", "Number"));
        assert!(vm.check_subtype("Complex{Float64}", "Number"));
        assert!(vm.check_subtype("Complex{Int64}", "Number"));
        assert!(!vm.check_subtype("Complex", "Real"));
        assert!(!vm.check_subtype("Complex{Float64}", "Real"));
        // Rational <: Real <: Number
        assert!(vm.check_subtype("Rational{Int64}", "Real"));
        assert!(vm.check_subtype("Rational{Int64}", "Number"));
        assert!(vm.check_subtype("Rational{Int32}", "Real"));
    }

    /// Verify String <: AbstractString.
    #[test]
    fn test_check_subtype_string() {
        let vm = make_vm();
        assert!(vm.check_subtype("String", "AbstractString"));
        assert!(!vm.check_subtype("String", "Number"));
    }

    /// Verify Union type handling.
    #[test]
    fn test_check_subtype_union() {
        let vm = make_vm();
        // Int64 <: Union{Int64, Float64}
        assert!(vm.check_subtype("Int64", "Union{Int64, Float64}"));
        // Float64 <: Union{Int64, Float64}
        assert!(vm.check_subtype("Float64", "Union{Int64, Float64}"));
        // String is NOT <: Union{Int64, Float64}
        assert!(!vm.check_subtype("String", "Union{Int64, Float64}"));
        // Nothing <: Union{} is false, but Nothing <: Union{Int64} is true
        assert!(vm.check_subtype("Nothing", "Union{Int64}"));
        // Union{} <: T is always true (bottom type)
        assert!(vm.check_subtype("Union{}", "Int64"));
    }

    /// Parity test: verify check_subtype() (runtime, string-based) agrees with
    /// JuliaType::is_subtype_of() (compile-time, enum-based) for ALL numeric
    /// type pairs. This catches divergence when new types are added to one
    /// implementation but not the other. (Issue #2494)
    #[test]
    fn test_check_subtype_parity_with_julia_type() {
        use crate::types::JuliaType;
        let vm = make_vm();

        // All concrete and abstract numeric types that both implementations handle
        let type_pairs: Vec<(&str, JuliaType)> = vec![
            // Signed integers
            ("Int8", JuliaType::Int8),
            ("Int16", JuliaType::Int16),
            ("Int32", JuliaType::Int32),
            ("Int64", JuliaType::Int64),
            ("Int128", JuliaType::Int128),
            ("BigInt", JuliaType::BigInt),
            // Unsigned integers
            ("UInt8", JuliaType::UInt8),
            ("UInt16", JuliaType::UInt16),
            ("UInt32", JuliaType::UInt32),
            ("UInt64", JuliaType::UInt64),
            ("UInt128", JuliaType::UInt128),
            // Bool
            ("Bool", JuliaType::Bool),
            // Floats
            ("Float16", JuliaType::Float16),
            ("Float32", JuliaType::Float32),
            ("Float64", JuliaType::Float64),
            ("BigFloat", JuliaType::BigFloat),
            // Abstract types
            ("Signed", JuliaType::Signed),
            ("Unsigned", JuliaType::Unsigned),
            ("Integer", JuliaType::Integer),
            ("AbstractFloat", JuliaType::AbstractFloat),
            ("Real", JuliaType::Real),
            ("Number", JuliaType::Number),
        ];

        // Check all pairs: for each (left, right), verify both implementations agree
        for (left_name, left_jtype) in &type_pairs {
            for (right_name, right_jtype) in &type_pairs {
                let runtime_result = vm.check_subtype(left_name, right_name);
                let compile_result = left_jtype.is_subtype_of(right_jtype);
                assert_eq!(
                    runtime_result, compile_result,
                    "Parity mismatch: check_subtype({left_name}, {right_name}) = {runtime_result}, \
                     is_subtype_of({left_name}, {right_name}) = {compile_result}"
                );
            }
        }
    }
}

