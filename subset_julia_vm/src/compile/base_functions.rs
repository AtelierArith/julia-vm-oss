//! Base function classification and builtin operation mapping.
//!
//! This module provides functions for classifying Julia function names:
//! - Whether a function belongs to Base or a Base submodule
//! - Whether a function is a random function
//! - Whether an operator can be reduced from n-arg to binary
//! - Mapping function names to builtin operations
//! - Type expression display helpers

use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::types::TypeExpr;

use super::constants::is_math_constant;

/// Extract module path from a nested FieldAccess expression.
/// For example, Base.MathConstants returns Some("Base.MathConstants")
pub(super) fn extract_module_path_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Var(name, _) => Some(name.clone()),
        Expr::Literal(Literal::Module(name), _) => Some(name.clone()),
        Expr::FieldAccess { object, field, .. } => {
            let parent_path = extract_module_path_from_expr(object)?;
            Some(format!("{}.{}", parent_path, field))
        }
        _ => None,
    }
}

/// Check if a function belongs to a Base submodule.
/// Returns Some(function_name) if the submodule contains the function.
pub(super) fn is_base_submodule_function(submodule: &str, function: &str) -> bool {
    match submodule {
        // Note: abs, abs2 removed — now Pure Julia (base/number.jl, base/complex.jl)
        // Note: sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia (base/math.jl)
        "Math" => matches!(function, "sqrt" | "floor" | "ceil" | "round"),
        "IO" => matches!(function, "println" | "print" | "error" | "throw"),
        "Collections" => matches!(
            function,
            "push!" | "pop!" | "length" | "size" | "zeros" | "ones" | "collect" | "first" | "last" // Note: trues, falses, fill are now Pure Julia (base/array.jl)
        ),
        "Random" => matches!(function, "rand" | "randn" | "StableRNG" | "Xoshiro"),
        // Note: Complex submodule removed — all functions (complex, real, imag, conj, abs, abs2)
        // are now Pure Julia (base/complex.jl, base/number.jl) — Issue #2645
        // Note: transpose, adjoint are now Pure Julia
        // Note: svd, qr, inv, eigen, eigvals, rank, cond are handled via call.rs
        // directly, not through this path — Issue #2645
        "LinearAlgebra" => matches!(function, "lu" | "det"),
        // Note: map, filter, reduce, foreach, sum are now Pure Julia
        "Iterators" => false, // sum moved to Pure Julia
        "MathConstants" => is_math_constant(function),
        "Meta" => matches!(
            function,
            "parse"
                | "isexpr"
                | "quot"
                | "lower"
                | "isidentifier"
                | "isoperator"
                | "isunaryoperator"
                | "isbinaryoperator"
                | "ispostfixoperator"
                | "unblock"
                | "unescape"
                | "show_sexpr"
        ),
        _ => false,
    }
}

/// Check if a function name belongs to Base module.
/// This includes all built-in functions that are available without explicit import.
pub(super) fn is_base_function(name: &str) -> bool {
    matches!(
        name,
        // I/O
        "println" | "print" | "error" | "throw" |
        "IOBuffer" | "take!" | "takestring!" | "write" |
        "open" | "close" | "isopen" | "eof" | "readline" |
        "tempname" | "tempdir" | "touch" | "rm" |
        "include_dependency" | "__precompile__" |
        // Note: dirname, basename are now Pure Julia (base/path.jl) — Issue #2637
        // Math functions
        // Note: sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia (base/math.jl)
        "sqrt" | "floor" | "ceil" | "round" |
        // Integer division intrinsic (called by div() in int.jl)
        "sdiv_int" |
        // Note: gcd, lcm, factorial are now Pure Julia (base/intfuncs.jl)
        // Type promotion functions
        "big" |
        // Note: abs, abs2, real, imag, conj, complex are Pure Julia
        // Random
        "rand" | "randn" |
        // System
        "sleep" |
        // Array creation and manipulation
        // Note: trues, falses, fill are now Pure Julia (base/array.jl)
        "zeros" | "ones" |
        "length" | "size" | "ndims" |
        "push!" | "pop!" | "pushfirst!" | "popfirst!" |
        "insert!" | "deleteat!" | "collect" |
        // Note: transpose and adjoint are now Pure Julia (see base/array.jl, base/number.jl)
        // Linear algebra operations (via faer library)
        // Note: inv is NOT here because it also exists for Rational (Pure Julia)
        "lu" | "det" |
        // Higher-order functions
        // Note: map, filter, reduce, foldl, foldr, foreach are now Pure Julia
        // Note: sum is now Pure Julia (base/array.jl)
        "any" | "all" | "count" | "sprint" |
        // Dict operations (haskey/get/getkey/merge now Pure Julia via dict.jl, Issue #2572, #2573)
        "delete!" | "get!" | "empty!" |
        "keys" | "values" | "pairs" | "merge!" |
        // Tuple operations
        "first" | "last" |
        // Range
        "range" |
        // RNG constructors
        "StableRNG" | "Xoshiro" |
        // String operations
        "string" | "repr" | "sprintf" |
        // Note: uppercase, lowercase, titlecase are now Pure Julia (base/strings/unicode.jl)
        // Note: strip, lstrip, rstrip, chomp, chop are now Pure Julia (base/strings/util.jl)
        // Note: startswith, endswith, occursin, join are now Pure Julia (base/strings/search.jl)
        // Note: repeat is now Pure Julia (base/strings/basic.jl)
        // Note: split is now Pure Julia (base/strings/util.jl)
        // Note: findfirst, findlast, findnext, findprev are now Pure Julia (base/strings/search.jl)
        "ncodeunits" | "codeunit" | "Char" | "Int" |
        // Utility
        "zero" | "ifelse" | "Ref" | "time_ns" |
        // Type inspection
        "typeof" | "isa" | "eltype" | "keytype" | "valtype" | "sizeof" | "isbits" | "isbitstype" |
        "supertype" | "supertypes" | "subtypes" | "typeintersect" | "hasfield" |
        // typejoin removed - now Pure Julia (base/reflection.jl)
        // isconcretetype, isabstracttype, isprimitivetype, isstructtype, ismutabletype removed
        // now Pure Julia (base/reflection.jl)
        "ismutable" |
        // fieldcount and nameof removed - now Pure Julia (base/reflection.jl)
        // Note: isunordered is now Pure Julia (base/operators.jl, Issue #2715)
        "objectid" |
        // Reflection (method introspection)
        "methods" | "hasmethod" | "which" |
        // Module introspection (Julia 1.11+)
        "isexported" | "ispublic" |
        // Set operations (builtin - works for both Sets and Arrays)
        "in" | "union" | "intersect" | "setdiff" | "symdiff" | "issubset" | "isdisjoint" | "issetequal" |
        // Set in-place operations (builtin - works for both Sets and Arrays)
        "union!" | "intersect!" | "setdiff!" | "symdiff!" |
        // Iterator protocol (enables fallback to builtin for arrays/ranges)
        "iterate" |
        // Julia-compliant indexing
        "getindex" | "setindex!" |
        // Meta module internal builtins
        "_meta_parse" | "_meta_parse_at" | "_meta_lower" |
        // Regex internal builtins
        "_regex_replace" |
        // Internal intrinsics for Pure Julia migration (Issue #2570, #2582)
        "_hash" | "_eltype"
    )
}

pub(super) fn is_random_function(name: &str) -> bool {
    matches!(name, "seed!")
}

/// Check if an operator can be reduced from n-arg to binary calls.
/// Julia's generic: +(a, b, c, xs...) = afoldl(+, a+b, c, xs...)
/// This applies to associative operators that Julia flattens (+ and *).
pub(super) fn is_reducible_nary_operator(name: &str) -> bool {
    matches!(name, "+" | "*")
}

/// Convert Base function name to BuiltinOp for proper type handling.
/// Returns None for functions that are handled via compile_builtin_call (string-based).
pub(super) fn base_function_to_builtin_op(name: &str) -> Option<BuiltinOp> {
    match name {
        "rand" => Some(BuiltinOp::Rand),
        "sqrt" => Some(BuiltinOp::Sqrt),
        "ifelse" => Some(BuiltinOp::IfElse),
        "time_ns" => Some(BuiltinOp::TimeNs),
        "zeros" => Some(BuiltinOp::Zeros),
        "ones" => Some(BuiltinOp::Ones),
        // Note: trues, falses, fill are now Pure Julia (base/array.jl)
        "length" => Some(BuiltinOp::Length),
        // Note: sum is now Pure Julia (base/array.jl)
        "size" => Some(BuiltinOp::Size),
        "ndims" => Some(BuiltinOp::Ndims),
        "push!" => Some(BuiltinOp::Push),
        "pop!" => Some(BuiltinOp::Pop),
        "pushfirst!" => Some(BuiltinOp::PushFirst),
        "popfirst!" => Some(BuiltinOp::PopFirst),
        "insert!" => Some(BuiltinOp::Insert),
        "deleteat!" => Some(BuiltinOp::DeleteAt),
        "zero" => Some(BuiltinOp::Zero),
        // Note: adjoint and transpose are now Pure Julia (base/array.jl, base/number.jl)
        // Linear algebra operations (via faer library)
        // Note: inv is NOT here because it also exists for Rational (Pure Julia)
        "lu" => Some(BuiltinOp::Lu),
        "det" => Some(BuiltinOp::Det),
        "StableRNG" => Some(BuiltinOp::StableRNG),
        "Xoshiro" => Some(BuiltinOp::XoshiroRNG),
        "randn" => Some(BuiltinOp::Randn),
        "first" => Some(BuiltinOp::TupleFirst),
        "last" => Some(BuiltinOp::TupleLast),
        // haskey/get/getkey now Pure Julia (Issue #2572)
        "delete!" => Some(BuiltinOp::DictDelete),
        "get!" => Some(BuiltinOp::DictGetBang),
        "empty!" => Some(BuiltinOp::DictEmpty),
        "keys" => Some(BuiltinOp::DictKeys),
        "values" => Some(BuiltinOp::DictValues),
        "pairs" => Some(BuiltinOp::DictPairs),
        // merge now Pure Julia (Issue #2573)
        "merge!" => Some(BuiltinOp::DictMergeBang),
        "Ref" => Some(BuiltinOp::Ref),
        "typeof" => Some(BuiltinOp::TypeOf),
        "isa" => Some(BuiltinOp::Isa),
        "eltype" => Some(BuiltinOp::Eltype),
        "keytype" => Some(BuiltinOp::Keytype),
        "valtype" => Some(BuiltinOp::Valtype),
        "sizeof" => Some(BuiltinOp::Sizeof),
        "isbits" => Some(BuiltinOp::Isbits),
        "isbitstype" => Some(BuiltinOp::Isbitstype),
        "supertype" => Some(BuiltinOp::Supertype),
        "supertypes" => Some(BuiltinOp::Supertypes),
        "subtypes" => Some(BuiltinOp::Subtypes),
        "typeintersect" => Some(BuiltinOp::Typeintersect),
        // "typejoin" removed - now Pure Julia (base/reflection.jl)
        // "fieldcount" removed - now Pure Julia (base/reflection.jl)
        "hasfield" => Some(BuiltinOp::Hasfield),
        // "isconcretetype", "isabstracttype", "isprimitivetype", "isstructtype", "ismutabletype"
        // removed - now Pure Julia (base/reflection.jl)
        "ismutable" => Some(BuiltinOp::Ismutable),
        // "nameof" removed - now Pure Julia (base/reflection.jl)
        "objectid" => Some(BuiltinOp::Objectid),
        // "isunordered" removed — now Pure Julia (base/operators.jl, Issue #2715)
        // Reflection (method introspection)
        "methods" => Some(BuiltinOp::Methods),
        "hasmethod" => Some(BuiltinOp::HasMethod),
        "which" => Some(BuiltinOp::Which),
        "in" => Some(BuiltinOp::In),
        "iterate" => Some(BuiltinOp::Iterate),
        "collect" => Some(BuiltinOp::Collect),
        "Generator" => Some(BuiltinOp::Generator),
        // Metaprogramming
        "gensym" => Some(BuiltinOp::Gensym),
        _ => None,
    }
}

/// Convert a TypeExpr to a display string (e.g., "Float64", "Point{Float64}", "Array{Point{Float64}}").
pub(super) fn type_expr_to_string(expr: &TypeExpr) -> String {
    match expr {
        TypeExpr::Concrete(jt) => jt.name().to_string(),
        TypeExpr::TypeVar(name) => name.clone(),
        TypeExpr::Parameterized { base, params } => {
            if params.is_empty() {
                base.clone()
            } else {
                let params_str: Vec<String> = params.iter().map(type_expr_to_string).collect();
                format!("{}{{{}}}", base, params_str.join(", "))
            }
        }
        TypeExpr::RuntimeExpr(expr_str) => expr_str.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Verify that every BuiltinOp variant is reachable from at least one
    /// construction path. This catches dead variants that exist in the enum
    /// but are never produced by any code path.
    ///
    /// Construction paths:
    /// 1. `map_builtin_name()` — lowering-time name→BuiltinOp mapping
    /// 2. `base_function_to_builtin_op()` — compile-time Base function mapping
    /// 3. Direct construction in lowering code (macros, quotes, etc.)
    ///
    /// If this test fails, either:
    /// - A new variant was added but not wired into any construction path (add it)
    /// - A variant became dead and should be removed from the enum
    ///
    /// See Issue #2642 for the prevention rationale.
    #[test]
    fn test_all_builtin_ops_reachable() {
        let mut reachable: HashSet<BuiltinOp> = HashSet::new();

        // Path 1: map_builtin_name() variants — enumerated here since
        // map_builtin_name is in a private module (lowering::expr::helpers).
        // When adding a new entry to map_builtin_name(), also add it here.
        let map_builtin_variants = [
            BuiltinOp::Rand,
            BuiltinOp::Sqrt,
            BuiltinOp::IfElse,
            BuiltinOp::Zeros,
            BuiltinOp::Ones,
            BuiltinOp::Reshape,
            BuiltinOp::Length,
            BuiltinOp::Size,
            BuiltinOp::Push,
            BuiltinOp::Pop,
            BuiltinOp::Zero,
            BuiltinOp::StableRNG,
            BuiltinOp::XoshiroRNG,
            BuiltinOp::Randn,
            BuiltinOp::TupleFirst,
            BuiltinOp::TupleLast,
            // HasKey, DictGet, DictMerge, DictKeys, DictValues, DictPairs removed — now Pure Julia (Issue #2572, #2573, #2669)
            BuiltinOp::DictDelete,
            BuiltinOp::Ref,
            BuiltinOp::TypeOf,
            BuiltinOp::Isa,
            BuiltinOp::Iterate,
            BuiltinOp::Collect,
            BuiltinOp::Esc,
            BuiltinOp::Eval,
            BuiltinOp::MacroExpand,
            BuiltinOp::MacroExpandBang,
            BuiltinOp::IncludeString,
            BuiltinOp::EvalFile,
            BuiltinOp::SymbolNew,
            BuiltinOp::ExprNew,
            BuiltinOp::LineNumberNodeNew,
            BuiltinOp::QuoteNodeNew,
            BuiltinOp::GlobalRefNew,
            BuiltinOp::TimeNs,
            BuiltinOp::TestRecord,
            BuiltinOp::TestRecordBroken,
            BuiltinOp::TestSetBegin,
            BuiltinOp::TestSetEnd,
        ];
        for op in &map_builtin_variants {
            reachable.insert(*op);
        }

        // Path 2: base_function_to_builtin_op() — all known input strings
        let base_fn_inputs = [
            "rand",
            "sqrt",
            "ifelse",
            "time_ns",
            "zeros",
            "ones",
            "length",
            "size",
            "ndims",
            "push!",
            "pop!",
            "pushfirst!",
            "popfirst!",
            "insert!",
            "deleteat!",
            "zero",
            "lu",
            "det",
            "StableRNG",
            "Xoshiro",
            "randn",
            "first",
            "last",
            "delete!",
            "get!",
            "empty!", // Dict mutating ops (Issue #2572)
            "keys",
            "values",
            "pairs",
            "merge!", // merge now Pure Julia (Issue #2573)
            "Ref",
            "typeof",
            "isa",
            "eltype",
            "keytype",
            "valtype",
            "sizeof",
            "isbits",
            "isbitstype",
            "supertype",
            "supertypes",
            "subtypes",
            "typeintersect",
            "hasfield",
            "ismutable",
            "objectid",
            "isunordered",
            "methods",
            "hasmethod",
            "which",
            "in",
            "iterate",
            "collect",
            "Generator",
            "gensym",
        ];
        for name in &base_fn_inputs {
            if let Some(op) = base_function_to_builtin_op(name) {
                reachable.insert(op);
            }
        }

        // Path 3: Directly constructed in lowering code (macros, quotes, etc.)
        // These variants are created by explicit `BuiltinOp::Xxx` in lowering/*.rs
        let directly_constructed = [
            BuiltinOp::ExprNew,            // quote/cst_to_constructor.rs, macros.rs
            BuiltinOp::SymbolNew,          // quote/cst_to_constructor.rs, macros.rs
            BuiltinOp::LineNumberNodeNew,  // quote/cst_to_constructor.rs, macros.rs
            BuiltinOp::SplatInterpolation, // quote/handlers.rs
            BuiltinOp::IsDefined,          // macros.rs (@isdefined)
            BuiltinOp::TypeOf,             // mod.rs (type inference)
            BuiltinOp::Seed,               // Random.seed!() via is_random_function
        ];
        for op in &directly_constructed {
            reachable.insert(*op);
        }

        // Known dead variants: kept in enum for handler code but no longer
        // produced by any lowering/compilation path (migrated to Pure Julia).
        let dead_but_kept = [
            BuiltinOp::HasKey,      // now Pure Julia haskey() (Issue #2572)
            BuiltinOp::DictGet,     // now Pure Julia get() (Issue #2572)
            BuiltinOp::DictGetkey,  // now Pure Julia getkey() (Issue #2572)
            BuiltinOp::DictMerge,   // now Pure Julia merge() (Issue #2573)
            BuiltinOp::DictKeys,    // now Pure Julia keys() for Dict (Issue #2669)
            BuiltinOp::DictValues,  // now Pure Julia values() for Dict (Issue #2669)
            BuiltinOp::DictPairs,   // now Pure Julia pairs() for Dict (Issue #2669)
            BuiltinOp::Isunordered, // now Pure Julia isunordered() (Issue #2715)
        ];
        for op in &dead_but_kept {
            reachable.insert(*op);
        }

        // All expected BuiltinOp variants (must match the enum definition in ir/core.rs)
        let all_variants = [
            BuiltinOp::Rand,
            BuiltinOp::Sqrt,
            BuiltinOp::IfElse,
            BuiltinOp::TimeNs,
            BuiltinOp::Zeros,
            BuiltinOp::Ones,
            BuiltinOp::Reshape,
            BuiltinOp::Length,
            BuiltinOp::Size,
            BuiltinOp::Ndims,
            BuiltinOp::Push,
            BuiltinOp::Pop,
            BuiltinOp::PushFirst,
            BuiltinOp::PopFirst,
            BuiltinOp::Insert,
            BuiltinOp::DeleteAt,
            BuiltinOp::Zero,
            BuiltinOp::Lu,
            BuiltinOp::Det,
            BuiltinOp::StableRNG,
            BuiltinOp::XoshiroRNG,
            BuiltinOp::Randn,
            BuiltinOp::TupleFirst,
            BuiltinOp::TupleLast,
            BuiltinOp::HasKey,
            BuiltinOp::DictGet,
            BuiltinOp::DictDelete,
            BuiltinOp::DictKeys,
            BuiltinOp::DictValues,
            BuiltinOp::DictPairs,
            BuiltinOp::DictMerge,
            BuiltinOp::DictGetBang,
            BuiltinOp::DictMergeBang,
            BuiltinOp::DictEmpty,
            BuiltinOp::DictGetkey,
            BuiltinOp::Ref,
            BuiltinOp::TypeOf,
            BuiltinOp::Isa,
            BuiltinOp::Eltype,
            BuiltinOp::Keytype,
            BuiltinOp::Valtype,
            BuiltinOp::Sizeof,
            BuiltinOp::Isbits,
            BuiltinOp::Isbitstype,
            BuiltinOp::Supertype,
            BuiltinOp::Supertypes,
            BuiltinOp::Subtypes,
            BuiltinOp::Typeintersect,
            BuiltinOp::Hasfield,
            BuiltinOp::Ismutable,
            BuiltinOp::Objectid,
            BuiltinOp::Isunordered,
            BuiltinOp::Methods,
            BuiltinOp::HasMethod,
            BuiltinOp::Which,
            BuiltinOp::In,
            BuiltinOp::Seed,
            BuiltinOp::Iterate,
            BuiltinOp::Collect,
            BuiltinOp::Generator,
            BuiltinOp::SymbolNew,
            BuiltinOp::ExprNew,
            BuiltinOp::LineNumberNodeNew,
            BuiltinOp::QuoteNodeNew,
            BuiltinOp::GlobalRefNew,
            BuiltinOp::Gensym,
            BuiltinOp::Esc,
            BuiltinOp::Eval,
            BuiltinOp::MacroExpand,
            BuiltinOp::MacroExpandBang,
            BuiltinOp::IncludeString,
            BuiltinOp::EvalFile,
            BuiltinOp::SplatInterpolation,
            BuiltinOp::TestRecord,
            BuiltinOp::TestRecordBroken,
            BuiltinOp::TestSetBegin,
            BuiltinOp::TestSetEnd,
            BuiltinOp::IsDefined,
        ];

        let mut unreachable = Vec::new();
        for variant in &all_variants {
            if !reachable.contains(variant) {
                unreachable.push(format!("{:?}", variant));
            }
        }

        assert!(
            unreachable.is_empty(),
            "Dead BuiltinOp variants found (not produced by any construction path):\n  {}\n\
             Either remove these from the enum or add them to a construction path.\n\
             See Issue #2642 for the three-layer cleanup checklist.",
            unreachable.join(", ")
        );

        // Also verify the all_variants list is complete (catches missing entries)
        assert_eq!(
            all_variants.len(),
            78, // Must match the actual enum variant count
            "all_variants list count mismatch — update this test when adding/removing BuiltinOp variants"
        );
    }

    /// Verify that every name in `BuiltinId::from_name()` is accounted for
    /// in either `is_base_function()` or an explicit exemption list.
    ///
    /// This catches the inconsistency found in Issue #2639 where 5 out of 7
    /// path operation builtins were NOT registered in `is_base_function()`.
    ///
    /// If this test fails, either:
    /// - Add the name to `is_base_function()` (if it should be routed as a builtin)
    /// - Add it to `EXEMPTED_FROM_IS_BASE_FUNCTION` with a comment explaining why
    #[test]
    fn test_builtin_id_registration_completeness() {
        use crate::builtins::BuiltinId;

        // All names that BuiltinId::from_name() accepts.
        // Must be kept in sync with builtins.rs from_name().
        let all_builtin_names = [
            // Math
            // sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia (base/math.jl)
            "round",
            "trunc",
            "trunc_digits",
            "trunc_sigdigits",
            "nextfloat",
            "prevfloat",
            "count_ones",
            "count_zeros",
            "leading_zeros",
            "leading_ones",
            "trailing_zeros",
            "trailing_ones",
            "bitreverse",
            "bitrotate",
            "bswap",
            "exponent",
            "significand",
            "frexp",
            "issubnormal",
            "maxintfloat",
            "fma",
            "muladd",
            // Array
            "zeros",
            "ones",
            "similar",
            "reshape",
            "length",
            "size",
            "ndims",
            "eltype",
            "keytype",
            "valtype",
            "push!",
            "pop!",
            "pushfirst!",
            "popfirst!",
            "insert!",
            "deleteat!",
            "append!",
            "prepend!",
            "sort",
            "findfirst",
            "findall",
            // HOF
            "any",
            "all",
            "count",
            "ntuple",
            "compose",
            // Range
            "range",
            "collect",
            "LinRange",
            // Complex
            "complex",
            // String
            "string",
            "String",
            "repr",
            "sprintf",
            "ncodeunits",
            "codeunit",
            "codeunits",
            "occursin",
            "Char",
            "codepoint",
            "bitstring",
            "unescape_string",
            "isnumeric",
            // I/O
            "print",
            "println",
            "IOBuffer",
            "take!",
            "takestring!",
            "write",
            "displaysize",
            "include_dependency",
            "__precompile__",
            "normpath",
            "abspath",
            "homedir",
            // File I/O
            "readlines",
            "readline",
            "countlines",
            "isfile",
            "isdir",
            "ispath",
            "filesize",
            "pwd",
            "readdir",
            "mkdir",
            "mkpath",
            "rm",
            "tempdir",
            "tempname",
            "touch",
            "cd",
            "islink",
            "cp",
            "mv",
            "mtime",
            "open",
            "close",
            "eof",
            "isopen",
            // RNG
            "rand",
            "randn",
            // Time
            "time_ns",
            "sleep",
            // Type
            "typeof",
            "isa",
            "sizeof",
            "isbits",
            "isbitstype",
            "supertype",
            "hasfield",
            "ismutable",
            "objectid",
            "isunordered",
            // Equality
            "isequal",
            "isless",
            "hash",
            "!==",
            ">:",
            // Set
            "in",
            "Set",
            "union",
            "intersect",
            "setdiff",
            "symdiff",
            "issubset",
            "isdisjoint",
            "issetequal",
            "union!",
            "intersect!",
            "setdiff!",
            "symdiff!",
            // Conversion
            "convert",
            "promote",
            "signed",
            "unsigned",
            "float",
            "widemul",
            "reinterpret",
            // Copy
            "deepcopy",
            // Reflection
            "_fieldnames",
            "_fieldtypes",
            "_getfield",
            "_isabstracttype",
            "_isconcretetype",
            "_ismutabletype",
            // Hash/Eltype internal intrinsics (Issue #2570, #2582)
            "_hash",
            "_eltype",
            "getfield",
            "setfield!",
            "methods",
            "hasmethod",
            "which",
            "isexported",
            "ispublic",
            // Dict internal intrinsics (Issue #2572, #2669)
            "_dict_get",
            "_dict_set!",
            "_dict_delete!",
            "_dict_haskey",
            "_dict_length",
            "_dict_empty!",
            "_dict_keys",
            "_dict_values",
            "_dict_pairs",
            // Set internal intrinsics (Issue #2574)
            "_set_push!",
            "_set_delete!",
            "_set_in",
            "_set_empty!",
            "_set_length",
            // Tuple
            "first",
            "last",
            // Dict (get/haskey/delete!/get!/getkey/empty! now Pure Julia via dict.jl)
            "Dict",
            "keys",
            "values",
            "pairs",
            "merge",
            "merge!",
            // Linear Algebra
            "lu",
            "det",
            "inv",
            "\\",
            "svd",
            "qr",
            "eigen",
            "eigvals",
            "cholesky",
            "rank",
            "cond",
            // Broadcast
            "Ref",
            // Zero/One
            "zero",
            "one",
            // Numeric Type Constructors
            "Int8",
            "Int16",
            "Int32",
            "Int64",
            "Int128",
            "UInt8",
            "UInt16",
            "UInt32",
            "UInt64",
            "UInt128",
            "Float16",
            "Float32",
            "Float64",
            "BigInt",
            "BigFloat",
            "_bigfloat_precision",
            "_bigfloat_default_precision",
            "_set_bigfloat_default_precision!",
            "_bigfloat_rounding",
            "_set_bigfloat_rounding!",
            // Subnormal
            "get_zero_subnormals",
            "set_zero_subnormals",
            // Missing
            "nonmissingtype",
            // Iterator
            "iterate",
            // Macro
            "Symbol",
            "Expr",
            "gensym",
            "esc",
            "QuoteNode",
            "LineNumberNode",
            "GlobalRef",
            "eval",
            "_meta_parse",
            "_meta_parse_at",
            "_meta_isexpr",
            "_meta_quot",
            "_meta_isidentifier",
            "_meta_isoperator",
            "_meta_isunaryoperator",
            "_meta_isbinaryoperator",
            "_meta_ispostfixoperator",
            "_meta_lower",
            "macroexpand",
            "macroexpand!",
            "include_string",
            "evalfile",
            // Test
            "_test_record!",
            "_test_record_broken!",
            "_testset_begin!",
            "_testset_end!",
            // Regex
            "Regex",
            "match",
            "eachmatch",
            "_regex_replace",
        ];

        // Verify each name actually resolves via from_name
        for name in &all_builtin_names {
            assert!(
                BuiltinId::from_name(name).is_some(),
                "Name '{}' is listed in test but BuiltinId::from_name() returns None — \
                 remove it from this test or fix builtins.rs",
                name
            );
        }

        // Names that are NOT in is_base_function() by design.
        // Each exemption must have a comment explaining why.
        let exempted: HashSet<&str> = [
            // Type constructors — handled by type dispatch path, not is_base_function()
            "Int8",
            "Int16",
            "Int32",
            "Int64",
            "Int128",
            "UInt8",
            "UInt16",
            "UInt32",
            "UInt64",
            "UInt128",
            "Float16",
            "Float32",
            "Float64",
            "BigInt",
            "BigFloat",
            // Note: Char is in is_base_function() — not exempted
            "Dict",
            "Set",
            "Regex",
            "String",
            "LinRange",
            // Internal intrinsics — prefixed with underscore, not callable from Julia
            "_fieldnames",
            "_fieldtypes",
            "_getfield",
            "_isabstracttype",
            "_isconcretetype",
            "_ismutabletype",
            // _hash, _eltype: now in is_base_function (Issue #2570, #2582)
            "_dict_get",
            "_dict_set!",
            "_dict_delete!",
            "_dict_haskey",
            "_dict_length",
            "_dict_empty!",
            "_dict_keys",
            "_dict_values",
            "_dict_pairs",
            "_set_push!",
            "_set_delete!",
            "_set_in",
            "_set_empty!",
            "_set_length",
            "_bigfloat_precision",
            "_bigfloat_default_precision",
            "_set_bigfloat_default_precision!",
            "_bigfloat_rounding",
            "_set_bigfloat_rounding!",
            // _meta_parse, _meta_parse_at, _meta_lower are in is_base_function() — not exempted
            "_meta_isexpr",
            "_meta_quot",
            "_meta_isidentifier",
            "_meta_isoperator",
            "_meta_isunaryoperator",
            "_meta_isbinaryoperator",
            "_meta_ispostfixoperator",
            "_test_record!",
            "_test_record_broken!",
            "_testset_begin!",
            "_testset_end!",
            // _regex_replace is in is_base_function() — not exempted
            // Compile-time intercepted — handled by explicit routing in call.rs
            // before is_base_function() is checked
            // Note: floor, ceil are in is_base_function() — not exempted
            "trunc",
            "trunc_digits",
            "trunc_sigdigits",
            "convert",
            "promote",
            "float",
            "widemul",
            "reinterpret",
            "signed",
            "unsigned",
            "complex",
            "similar",
            "reshape",
            "ntuple",
            "compose",
            "sort",
            "append!",
            "prepend!",
            "findfirst",
            "findall",
            "deepcopy",
            "nextfloat",
            "prevfloat",
            "count_ones",
            "count_zeros",
            "leading_zeros",
            "leading_ones",
            "trailing_zeros",
            "trailing_ones",
            "bitreverse",
            "bitrotate",
            "bswap",
            "exponent",
            "significand",
            "frexp",
            "issubnormal",
            "maxintfloat",
            "fma",
            "muladd",
            "codepoint",
            "codeunits",
            "bitstring",
            "unescape_string",
            "isnumeric",
            "normpath",
            "abspath",
            "homedir",
            "merge",       // Pure Julia (Issue #2573), no longer in is_base_function()
            "isunordered", // Pure Julia (Issue #2715), no longer in is_base_function()
            "one", // BuiltinId::One exists but not in is_base_function (Pure Julia covers most)
            "inv", // Handled specially in call.rs (matrix vs rational)
            // Operator builtins — matched by operator dispatch, not function name
            "!==",
            ">:",
            "\\",
            // File I/O — compile-time routed, not through is_base_function()
            // Note: readline is in is_base_function() — not exempted
            "readlines",
            "countlines",
            "isfile",
            "isdir",
            "ispath",
            "filesize",
            "pwd",
            "readdir",
            "mkdir",
            "mkpath",
            "cd",
            "islink",
            "cp",
            "mv",
            "mtime",
            "displaysize",
            // These are in is_base_function but under different names or paths
            "getfield",
            "setfield!",
            // Equality — compile-time routed
            "isequal",
            "isless",
            "hash",
            "nonmissingtype",
            "get_zero_subnormals",
            "set_zero_subnormals",
            // Set mutation variants — now in is_base_function(), removed from exemptions
            // String — compile-time routed
            "occursin",
            // Macro — compile-time routed (BuiltinOp path)
            "Symbol",
            "Expr",
            "gensym",
            "esc",
            "QuoteNode",
            "LineNumberNode",
            "GlobalRef",
            "eval",
            "macroexpand",
            "macroexpand!",
            "include_string",
            "evalfile",
            "match",
            "eachmatch",
            // Regex
            "match",
            "eachmatch",
            // Linear algebra — routed via is_base_submodule_function("LinearAlgebra"),
            // not through is_base_function()
            "svd",
            "qr",
            "eigen",
            "eigvals",
            "cholesky",
            "rank",
            "cond",
        ]
        .iter()
        .cloned()
        .collect();

        let mut missing = Vec::new();
        for name in &all_builtin_names {
            if !is_base_function(name) && !exempted.contains(name) {
                missing.push(*name);
            }
        }

        assert!(
            missing.is_empty(),
            "BuiltinId names not in is_base_function() or exemption list:\n  {}\n\
             Either add to is_base_function() or add to EXEMPTED with a comment.\n\
             See Issue #2639.",
            missing.join(", ")
        );

        // Verify exemption list is not stale (no exempted names that ARE in is_base_function)
        let mut stale_exemptions = Vec::new();
        for name in &exempted {
            if is_base_function(name) {
                stale_exemptions.push(*name);
            }
        }

        assert!(
            stale_exemptions.is_empty(),
            "Stale exemptions — these names ARE in is_base_function() but are also exempted:\n  {}\n\
             Remove from the exemption list.",
            stale_exemptions.join(", ")
        );
    }
}
