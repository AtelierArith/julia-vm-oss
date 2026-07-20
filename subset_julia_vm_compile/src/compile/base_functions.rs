//! Base function classification and builtin operation mapping.
//!
//! This module provides functions for classifying Julia function names:
//! - Whether a function belongs to Base or a Base submodule
//! - Whether a function is a random function
//! - Whether an operator can be reduced from n-arg to binary
//! - Mapping function names to builtin operations

use crate::ir::core::{BuiltinOp, Expr, Literal};

use super::constants::is_math_constant;

/// Classification for public Base names that still have a Rust fallback route.
///
/// Public Julia APIs should use method dispatch first whenever upstream Julia
/// implements the behavior in Julia. Direct/runtime/internal classifications
/// document the cases where sjulia still intentionally emits a Rust operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BaseRouteKind {
    /// Public call should consult Julia methods first; Rust is only a primitive
    /// or cache-compatibility fallback.
    DispatchFirst,
    /// Public call is intentionally compiled directly to a Rust builtin.
    DirectBuiltin,
    /// Boundary to runtime/OS/host services where Pure Julia is not expected.
    RuntimeBoundary,
    /// Internal helper used by sjulia's Pure Julia Base implementation.
    InternalIntrinsic,
    /// Constructor or compiler helper with special lowering semantics.
    CompilerIntrinsic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BaseFunctionRoute {
    pub name: &'static str,
    pub builtin_op: Option<BuiltinOp>,
    pub kind: BaseRouteKind,
    /// Upstream Julia source that should be checked before changing the route.
    pub upstream_ref: &'static str,
}

impl BaseFunctionRoute {
    const fn new(
        name: &'static str,
        builtin_op: Option<BuiltinOp>,
        kind: BaseRouteKind,
        upstream_ref: &'static str,
    ) -> Self {
        Self {
            name,
            builtin_op,
            kind,
            upstream_ref,
        }
    }

    pub(super) fn is_dispatch_first(self) -> bool {
        matches!(self.kind, BaseRouteKind::DispatchFirst)
    }
}

const fn route(
    name: &'static str,
    builtin_op: BuiltinOp,
    kind: BaseRouteKind,
    upstream_ref: &'static str,
) -> BaseFunctionRoute {
    BaseFunctionRoute::new(name, Some(builtin_op), kind, upstream_ref)
}

const fn marker(
    name: &'static str,
    kind: BaseRouteKind,
    upstream_ref: &'static str,
) -> BaseFunctionRoute {
    BaseFunctionRoute::new(name, None, kind, upstream_ref)
}

/// Single registry for public Base names whose Rust builtin route is still
/// retained at compile time.
///
/// When adding a public Base fallback, add it here with a route kind and the
/// upstream Julia file that owns the semantics. The CI audit rejects direct
/// string arms in `base_function_to_builtin_op` so this remains the inventory.
pub(super) const BASE_FUNCTION_ROUTES: &[BaseFunctionRoute] = &[
    route(
        "rand",
        BuiltinOp::Rand,
        BaseRouteKind::RuntimeBoundary,
        "julia/stdlib/Random/src/Random.jl",
    ),
    route(
        "sqrt",
        BuiltinOp::Sqrt,
        BaseRouteKind::DispatchFirst,
        "julia/base/math.jl",
    ),
    route(
        "time_ns",
        BuiltinOp::TimeNs,
        BaseRouteKind::RuntimeBoundary,
        "julia/base/c.jl",
    ),
    route(
        "length",
        BuiltinOp::Length,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    route(
        "size",
        BuiltinOp::Size,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    route(
        "ndims",
        BuiltinOp::Ndims,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    route(
        "push!",
        BuiltinOp::Push,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "pop!",
        BuiltinOp::Pop,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "pushfirst!",
        BuiltinOp::PushFirst,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "popfirst!",
        BuiltinOp::PopFirst,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "insert!",
        BuiltinOp::Insert,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "deleteat!",
        BuiltinOp::DeleteAt,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "reshape",
        BuiltinOp::Reshape,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "zero",
        BuiltinOp::Zero,
        BaseRouteKind::DispatchFirst,
        "julia/base/number.jl",
    ),
    route(
        "lu",
        BuiltinOp::Lu,
        BaseRouteKind::DispatchFirst,
        "julia/stdlib/LinearAlgebra/src/lu.jl",
    ),
    route(
        "det",
        BuiltinOp::Det,
        BaseRouteKind::DispatchFirst,
        "julia/stdlib/LinearAlgebra/src/dense.jl",
    ),
    route(
        "StableRNG",
        BuiltinOp::StableRNG,
        BaseRouteKind::RuntimeBoundary,
        "julia/stdlib/Random/src/RNGs.jl",
    ),
    route(
        "Xoshiro",
        BuiltinOp::XoshiroRNG,
        BaseRouteKind::RuntimeBoundary,
        "julia/stdlib/Random/src/Xoshiro.jl",
    ),
    route(
        "MersenneTwister",
        BuiltinOp::MersenneTwisterRNG,
        BaseRouteKind::RuntimeBoundary,
        "julia/stdlib/Random/src/RNGs.jl",
    ),
    route(
        "randn",
        BuiltinOp::Randn,
        BaseRouteKind::RuntimeBoundary,
        "julia/stdlib/Random/src/normal.jl",
    ),
    route(
        "_tuple_first",
        BuiltinOp::TupleFirst,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/tuple.jl",
    ),
    route(
        "_tuple_last",
        BuiltinOp::TupleLast,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/tuple.jl",
    ),
    route(
        "_range_step",
        BuiltinOp::RangeStep,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/range.jl",
    ),
    marker("step", BaseRouteKind::DispatchFirst, "julia/base/range.jl"),
    // Issue #6731 slice 2: delete!/get!/empty!/merge! on a Dict route through the
    // pure-Julia Dict{K,V} methods (base/dict.jl) only — no Value::Dict builtin
    // fallback. Markers (no builtin_op) so the call dispatches to the pure
    // parametric methods (static and dynamic, incl. the #6584 Any-binding case).
    marker(
        "delete!",
        BaseRouteKind::DispatchFirst,
        "julia/base/dict.jl",
    ),
    marker("get!", BaseRouteKind::DispatchFirst, "julia/base/dict.jl"),
    marker("empty!", BaseRouteKind::DispatchFirst, "julia/base/dict.jl"),
    // Issue #6731: keys/values/pairs route through pure-Julia Dict{K,V} methods
    // (base/dict.jl) only — no Value::Dict builtin fallback. Marker (no
    // builtin_op) so base_function_to_builtin_op returns None and the call
    // dispatches to the pure parametric methods (static and dynamic).
    marker(
        "keys",
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractdict.jl",
    ),
    marker(
        "values",
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractdict.jl",
    ),
    marker(
        "pairs",
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractdict.jl",
    ),
    marker("merge!", BaseRouteKind::DispatchFirst, "julia/base/dict.jl"),
    marker(
        "Ref",
        BaseRouteKind::DispatchFirst,
        "julia/base/refpointer.jl",
    ),
    marker(
        "compose",
        BaseRouteKind::DispatchFirst,
        "julia/base/operators.jl",
    ),
    marker(
        "deepcopy",
        BaseRouteKind::DispatchFirst,
        "julia/base/deepcopy.jl",
    ),
    marker(
        "nonmissingtype",
        BaseRouteKind::DispatchFirst,
        "julia/base/missing.jl",
    ),
    route(
        "typeof",
        BuiltinOp::TypeOf,
        BaseRouteKind::DirectBuiltin,
        "julia/base/essentials.jl",
    ),
    route(
        "isa",
        BuiltinOp::Isa,
        BaseRouteKind::DirectBuiltin,
        "julia/base/operators.jl",
    ),
    route(
        "eltype",
        BuiltinOp::Eltype,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    route(
        "keytype",
        BuiltinOp::Keytype,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractdict.jl",
    ),
    route(
        "valtype",
        BuiltinOp::Valtype,
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractdict.jl",
    ),
    route(
        "sizeof",
        BuiltinOp::Sizeof,
        BaseRouteKind::DispatchFirst,
        "julia/base/essentials.jl",
    ),
    marker(
        "isbits",
        BaseRouteKind::DispatchFirst,
        "julia/base/reflection.jl",
    ),
    route(
        "isbitstype",
        BuiltinOp::Isbitstype,
        BaseRouteKind::DispatchFirst,
        "julia/base/runtime_internals.jl",
    ),
    route(
        "_supertype",
        BuiltinOp::Supertype,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/reflection.jl",
    ),
    route(
        "_typename",
        BuiltinOp::Typename,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/reflection.jl",
    ),
    route(
        "_function_name",
        BuiltinOp::FunctionName,
        BaseRouteKind::InternalIntrinsic,
        "julia/base/reflection.jl",
    ),
    route(
        "subtypes",
        BuiltinOp::Subtypes,
        BaseRouteKind::DirectBuiltin,
        "julia/base/reflection.jl",
    ),
    marker(
        "hasfield",
        BaseRouteKind::DispatchFirst,
        "julia/base/reflection.jl",
    ),
    marker(
        "ismutable",
        BaseRouteKind::DispatchFirst,
        "julia/base/reflection.jl",
    ),
    route(
        "objectid",
        BuiltinOp::Objectid,
        BaseRouteKind::DispatchFirst,
        "julia/base/runtime_internals.jl",
    ),
    route(
        "_methods_by_ftype",
        BuiltinOp::Methods,
        BaseRouteKind::InternalIntrinsic,
        "julia/Compiler/src/methodtable.jl",
    ),
    route(
        "hasmethod",
        BuiltinOp::HasMethod,
        BaseRouteKind::DispatchFirst,
        "julia/base/reflection.jl",
    ),
    route(
        "in",
        BuiltinOp::In,
        BaseRouteKind::DispatchFirst,
        "julia/base/operators.jl",
    ),
    marker("∈", BaseRouteKind::DispatchFirst, "julia/base/operators.jl"),
    marker("∉", BaseRouteKind::DispatchFirst, "julia/base/operators.jl"),
    marker("∋", BaseRouteKind::DispatchFirst, "julia/base/operators.jl"),
    marker("∌", BaseRouteKind::DispatchFirst, "julia/base/operators.jl"),
    route(
        "iterate",
        BuiltinOp::Iterate,
        BaseRouteKind::DispatchFirst,
        "julia/base/essentials.jl",
    ),
    route(
        "collect",
        BuiltinOp::Collect,
        BaseRouteKind::DispatchFirst,
        "julia/base/array.jl",
    ),
    route(
        "Generator",
        BuiltinOp::Generator,
        BaseRouteKind::CompilerIntrinsic,
        "julia/base/generator.jl",
    ),
    route(
        "gensym",
        BuiltinOp::Gensym,
        BaseRouteKind::CompilerIntrinsic,
        "julia/base/expr.jl",
    ),
    route(
        "macroexpand",
        BuiltinOp::MacroExpand,
        BaseRouteKind::CompilerIntrinsic,
        "julia/base/reflection.jl",
    ),
    route(
        "macroexpand!",
        BuiltinOp::MacroExpandBang,
        BaseRouteKind::CompilerIntrinsic,
        "julia/base/reflection.jl",
    ),
    marker(
        "getindex",
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    marker(
        "setindex!",
        BaseRouteKind::DispatchFirst,
        "julia/base/abstractarray.jl",
    ),
    marker(
        "ncodeunits",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/basic.jl",
    ),
    marker(
        "codeunit",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/basic.jl",
    ),
    marker(
        "codeunits",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/basic.jl",
    ),
    marker(
        "isvalid",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/basic.jl",
    ),
    marker(
        "string",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/io.jl",
    ),
    marker(
        "String",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/string.jl",
    ),
    marker("Char", BaseRouteKind::DispatchFirst, "julia/base/char.jl"),
    marker("Int", BaseRouteKind::DispatchFirst, "julia/base/char.jl"),
    marker(
        "_string",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/strings/io.jl",
    ),
    marker(
        "_string_from_chars",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/strings/string.jl",
    ),
    marker(
        "_char_to_int",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/char.jl",
    ),
    marker(
        "_int_to_char",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/char.jl",
    ),
    marker(
        "sprintf",
        // Pure-Julia Printf engine (base/printf.jl, Issue #6746). The Rust
        // BuiltinId::Sprintf remains only as a no-method fallback.
        BaseRouteKind::DispatchFirst,
        "julia/stdlib/Printf/src/Printf.jl",
    ),
    marker(
        "bitstring",
        BaseRouteKind::DispatchFirst,
        "julia/base/intfuncs.jl",
    ),
    marker(
        "codepoint",
        BaseRouteKind::DispatchFirst,
        "julia/base/char.jl",
    ),
    marker(
        "isnumeric",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/unicode.jl",
    ),
    marker(
        "unescape_string",
        BaseRouteKind::DispatchFirst,
        "julia/base/strings/io.jl",
    ),
    marker("parse", BaseRouteKind::DispatchFirst, "julia/base/parse.jl"),
    marker(
        "tryparse",
        BaseRouteKind::DispatchFirst,
        "julia/base/parse.jl",
    ),
    marker(
        "_tryparse_float64",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/parse.jl",
    ),
    marker(
        "_linspace_range_f64",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/twiceprecision.jl",
    ),
    marker(
        "_try_complex_scale_tp_range_f64",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/twiceprecision.jl",
    ),
    marker(
        "_try_broadcast_typed_kernel",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/broadcast.jl",
    ),
    marker(
        "_try_broadcast_binary_arith",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/broadcast.jl",
    ),
    marker("big", BaseRouteKind::DispatchFirst, "julia/base/gmp.jl"),
    marker(
        "convert",
        BaseRouteKind::DispatchFirst,
        "julia/base/essentials.jl",
    ),
    marker(
        "promote",
        BaseRouteKind::DispatchFirst,
        "julia/base/promotion.jl",
    ),
    marker(
        "signed",
        BaseRouteKind::DispatchFirst,
        "julia/base/number.jl",
    ),
    marker(
        "unsigned",
        BaseRouteKind::DispatchFirst,
        "julia/base/number.jl",
    ),
    marker(
        "memoryref",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/essentials.jl",
    ),
    marker(
        "memoryrefnew",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/essentials.jl",
    ),
    marker(
        "memoryrefget",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/essentials.jl",
    ),
    marker(
        "memoryrefset!",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/essentials.jl",
    ),
    marker(
        "memoryrefoffset",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/genericmemory.jl",
    ),
    marker(
        "memoryrefparent",
        BaseRouteKind::InternalIntrinsic,
        "julia/base/genericmemory.jl",
    ),
];

pub(super) fn base_function_route(name: &str) -> Option<&'static BaseFunctionRoute> {
    let name = name.strip_prefix("Base.").unwrap_or(name);
    BASE_FUNCTION_ROUTES.iter().find(|route| route.name == name)
}

/// Extract module path from a nested FieldAccess expression.
/// For example, Base.MathConstants returns Some("Base.MathConstants")
pub(super) fn extract_module_path_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Var(name, _) => Some(name.to_string()),
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
            "push!" | "pop!" | "length" | "size" | "collect" // Note: trues, falses, fill, zeros, ones are now Pure Julia (base/array.jl); first/last are Pure Julia (Issue #3734)
        ),
        // Random is a stdlib root module, not a Base submodule. Upstream rejects
        // `Base.Random`, so sjulia must not expose `Base.Random.<fn>` as a public
        // route (Issue #8278).
        "Random" => false,
        // Note: Complex submodule removed — all functions (complex, real, imag, conj, abs, abs2)
        // are now Pure Julia (base/complex.jl, base/number.jl) — Issue #2645
        // LinearAlgebra is a stdlib, not a Base submodule. Upstream rejects
        // `Base.LinearAlgebra`, so sjulia must not expose it as a public route.
        "LinearAlgebra" => false,
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
    let name = name.strip_prefix("Base.").unwrap_or(name);
    matches!(
        name,
        // I/O
        "println" | "print" | "error" | "throw" | "rethrow" |
        "IOBuffer" | "take!" | "takestring!" | "write" |
        "open" | "close" | "isopen" | "eof" | "readline" |
        "seek" | "position" | "skip" | "flush" |
        "tempname" | "tempdir" | "touch" | "rm" |
        "include_dependency" | "__precompile__" |
        // Error/backtrace VM inspection helpers
        "_sjulia_backtrace" | "_sjulia_catch_backtrace" | "_sjulia_stacktrace" |
        // Note: dirname, basename are now Pure Julia (base/path.jl) — Issue #2637
        // Math functions
        // Note: sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia (base/math.jl)
        "sqrt" | "floor" | "ceil" | "round" |
        // Low-level bit CPU intrinsics (Issue #6741): the public functions
        // count_ones / leading_zeros / trailing_zeros / bitreverse / bswap are
        // pure Julia (base/int.jl) and call these underscored intrinsics, which
        // must be recognized here so the pure wrappers compile to them. (The
        // public functions get their method tables from base/int.jl; derived
        // count_zeros / leading_ones / trailing_ones / bitrotate are pure Julia
        // too — Issue #6722.)
        "_ctpop_int" | "_ctlz_int" | "_cttz_int" | "_bitreverse_int" | "_bswap_int" |
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
        // Note: first, last are now Pure Julia (Issue #3734)
        // Range
        "range" |
        // RNG constructors
        "StableRNG" | "Xoshiro" | "MersenneTwister" |
        // String operations
        "string" | "String" | "sprintf" |
        // Note: uppercase, lowercase, titlecase are now Pure Julia (base/strings/unicode.jl)
        // Note: strip, lstrip, rstrip, chomp, chop are now Pure Julia (base/strings/util.jl)
        // Note: startswith, endswith, occursin, join are now Pure Julia (base/strings/search.jl)
        // Note: repeat is now Pure Julia (base/strings/basic.jl)
        // Note: split is now Pure Julia (base/strings/util.jl)
        // Note: findfirst, findlast, findnext, findprev are now Pure Julia (base/strings/search.jl)
        "ncodeunits" | "codeunit" | "codeunits" | "isvalid" |
        "bitstring" | "codepoint" | "isnumeric" | "unescape_string" |
        "parse" | "tryparse" | "Char" | "Int" |
        // Float parse intrinsic; public parse/tryparse(Float64) are pure Julia (#6748)
        "_tryparse_float64" |
        // TwicePrecision float range intrinsic; public range(start, stop; length)
        // is pure Julia (base/range.jl, Issue #9419)
        "_linspace_range_f64" |
        // Complex-scaled TwicePrecision range broadcast (Issue #9659)
        "_try_complex_scale_tp_range_f64" |
        // Bulk typed-kernel broadcast (Issues #9693/#8797)
        "_try_broadcast_typed_kernel" |
        "_try_broadcast_binary_arith" |
        // Utility
        "zero" | "ifelse" | "Ref" | "compose" | "deepcopy" | "nonmissingtype" | "time_ns" |
        // Type inspection
        "typeof" | "isa" | "eltype" | "keytype" | "valtype" | "sizeof" | "isbitstype" |
        "subtypes" |
        // isbits, hasfield, ismutable removed - pure Julia (Issue #6738)
        // typejoin removed - now Pure Julia (base/reflection.jl)
        // isconcretetype, isabstracttype, isprimitivetype, isstructtype, ismutabletype removed
        // now Pure Julia (base/reflection.jl)
        // fieldcount and nameof removed - now Pure Julia (base/reflection.jl)
        // Note: isunordered is now Pure Julia (base/operators.jl, Issue #2715)
        "objectid" |
        // Reflection (method introspection)
        "hasmethod" |
        // Module introspection (Julia 1.11+)
        "names" | "isexported" | "ispublic" |
        // Set operations
        // Note: union/intersect/setdiff/symdiff/issubset/isdisjoint/issetequal and
        // their mutating variants now Pure Julia (base/set.jl) — Issue #3724
        "in" | "∈" | "∉" | "∋" | "∌" |
        // Iterator protocol (enables fallback to builtin for arrays/ranges)
        "iterate" |
        // Julia-compliant indexing
        "getindex" | "setindex!" |
        // MemoryRef primitives; Core builtins mirrored for Memory/Array storage.
        "memoryref" | "memoryrefnew" | "memoryrefget" | "memoryrefset!" |
        "memoryrefoffset" | "memoryrefparent" |
        // Meta module internal builtins
        "_meta_parse" | "_meta_parse_at" | "_meta_lower" |
        // Regex internal builtins
        "_regex_replace" | "_endswith_regex" | "_regexmatch_keys" | "_regex_findnext" |
        "_expand_substitution" | "_regex_match_from" |
        // VM continuation/session-state boundaries (Issue #10349)
        "_task_register_main" | "_task_schedule" | "_task_yield" |
        "_task_park" | "_task_wake" | "_task_current" |
        // Printf float→string boundary (Issue #6746)
        "_printf_fmt_float" |
        // Internal intrinsics for Pure Julia migration (Issue #2570, #2582, #3762, #3772)
        "_hash" | "_eltype" | "_supertype" | "_typename" | "_function_name" |
        "_ref_new" | "_ref_get" | "_compose" | "_nonmissingtype" | "_deepcopy" |
        "_string" | "_string_from_chars" | "_char_to_int" | "_int_to_char" |
        "_methods_by_ftype" | "_fma" |
        "_mark_bitvector" | "_mark_bitarray" |
        // Tuple-type construction backing tuple_type_tail/cons (Issue #5119)
        "_make_tuple_type" |
        // _tuple_first/_tuple_last/_range_step: aliases for Pure Julia code
        // that needs direct field access on native `Value::Range` values
        // (Issues #3734/#9519).
        "_tuple_first" | "_tuple_last" | "_range_step" |
        // signed / unsigned: Pure Julia methods exist in base/number.jl for
        // integer and Bool types (Issue #3727). Listed here so dispatch
        // failure on unsupported argument types (e.g. Float64) falls back to
        // the Rust BuiltinId::Signed / BuiltinId::Unsigned handler.
        "signed" | "unsigned"
    )
}

/// Public Base names whose direct calls must try Julia method dispatch before
/// Rust fallback routing. These functions have Pure Julia methods or user
/// extension points, while `base_function_to_builtin_op` remains as the
/// primitive/cache-compatibility fallback when dispatch finds no match.
pub(super) fn is_method_dispatch_first_base_function(name: &str) -> bool {
    base_function_route(name).is_some_and(|route| route.is_dispatch_first())
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
    let route = base_function_route(name)?;
    debug_assert!(
        !route.upstream_ref.is_empty(),
        "Base route {name} must document the upstream Julia source"
    );
    route.builtin_op
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
            // Note: BuiltinOp::Sqrt removed from map_builtin_name (Issue #3737).
            // Still reachable via `base_function_to_builtin_op("sqrt")` as a
            // fallback when method dispatch finds no Pure Julia method.
            // Note: BuiltinOp::IfElse moved to dead_but_kept (Issue #3733).
            // Note: BuiltinOp::Reshape removed from map_builtin_name (Issue #4276).
            // Still reachable via `base_function_to_builtin_op("reshape")` as a
            // fallback when method dispatch finds no Pure Julia method.
            // Note: BuiltinOp::Length / BuiltinOp::Size removed from
            // map_builtin_name (Issue #3736). Still reachable via
            // `base_function_to_builtin_op("length"|"size")` as a fallback for
            // primitive collections (Array, Tuple, String, Dict, Set, Range,
            // Generator) when method dispatch finds no Pure Julia method.
            // Note: BuiltinOp::Push, BuiltinOp::Pop removed from map_builtin_name
            // (Issue #3739). Still reachable via `base_function_to_builtin_op`
            // as fallback when method dispatch finds no Pure Julia method
            // (e.g., for `Array` push!/pop!).
            // Note: BuiltinOp::Zero removed from map_builtin_name (Issue #3737).
            // Still reachable via `base_function_to_builtin_op("zero")` as a
            // fallback when method dispatch finds no Pure Julia method.
            BuiltinOp::StableRNG,
            BuiltinOp::XoshiroRNG,
            BuiltinOp::MersenneTwisterRNG,
            BuiltinOp::Randn,
            // Note: TupleFirst, TupleLast removed from map_builtin_name (Issue #3734) — now Pure Julia
            // HasKey, DictGet, DictMerge, DictKeys, DictValues, DictPairs removed — now Pure Julia (Issue #2572, #2573, #2669)
            // Note: BuiltinOp::DictDelete removed from map_builtin_name (Issue
            // #3739). Still reachable via `base_function_to_builtin_op("delete!")`
            // for `Value::Dict` (legacy Rust HashMap) when dispatch finds no
            // Pure Julia method.
            BuiltinOp::TypeOf,
            BuiltinOp::Isa,
            // Note: BuiltinOp::Iterate / BuiltinOp::Collect removed from
            // map_builtin_name (Issue #3735). Still reachable via
            // `base_function_to_builtin_op("iterate"|"collect")` as a fallback
            // for primitive containers (Array, Tuple, String, Range) when
            // method dispatch finds no matching Pure Julia method.
            BuiltinOp::Esc,
            BuiltinOp::Eval,
            BuiltinOp::MacroExpand,
            BuiltinOp::MacroExpandBang,
            // Note: BuiltinOp::IncludeString and BuiltinOp::EvalFile moved to dead_but_kept (Issue #3738).
            BuiltinOp::SymbolNew,
            BuiltinOp::ExprNew,
            BuiltinOp::LineNumberNodeNew,
            BuiltinOp::QuoteNodeNew,
            BuiltinOp::GlobalRefNew,
            BuiltinOp::TimeNs,
            BuiltinOp::TestRecord,
            BuiltinOp::TestRecordBroken,
            BuiltinOp::TestRecordError,
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
            // Note: "ifelse" removed — now Pure Julia (Issue #3733).
            "time_ns",
            "length",
            "size",
            "ndims",
            "reshape",
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
            "MersenneTwister",
            "randn",
            // Note: "first", "last" removed (Issue #3734) — now Pure Julia
            // Internal aliases for native Range access (Issues #3734/#9519)
            "_tuple_first",
            "_tuple_last",
            "_range_step",
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
            "isbitstype",
            "_supertype",
            "_typename",
            "_function_name",
            "subtypes",
            "objectid",
            "isunordered",
            "_methods_by_ftype",
            "hasmethod",
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
            BuiltinOp::HasKey,        // now Pure Julia haskey() (Issue #2572)
            BuiltinOp::DictGet,       // now Pure Julia get() (Issue #2572)
            BuiltinOp::DictGetkey,    // now Pure Julia getkey() (Issue #2572)
            BuiltinOp::DictMerge,     // now Pure Julia merge() (Issue #2573)
            BuiltinOp::DictKeys,      // now Pure Julia keys() for Dict (Issue #2669)
            BuiltinOp::DictValues,    // now Pure Julia values() for Dict (Issue #2669)
            BuiltinOp::DictPairs,     // now Pure Julia pairs() for Dict (Issue #2669)
            BuiltinOp::DictDelete,    // now Pure Julia delete!() for Dict (Issue #6731)
            BuiltinOp::DictGetBang,   // now Pure Julia get!() for Dict (Issue #6731)
            BuiltinOp::DictEmpty,     // now Pure Julia empty!() for Dict (Issue #6731)
            BuiltinOp::DictMergeBang, // now Pure Julia merge!() for Dict (Issue #6731)
            BuiltinOp::Isunordered,   // now Pure Julia isunordered() (Issue #2715)
            BuiltinOp::IfElse,        // now Pure Julia ifelse() (Issue #3733)
            BuiltinOp::TupleFirst,    // now Pure Julia first() (Issue #3734)
            BuiltinOp::TupleLast,     // now Pure Julia last() (Issue #3734)
            BuiltinOp::Zeros,         // now Pure Julia zeros() allocation dispatch (Issue #4036)
            BuiltinOp::Ones,          // now Pure Julia ones() allocation dispatch (Issue #4036)
            BuiltinOp::Ref,           // now Pure Julia Ref(x) wrapper (Issue #8779)
            BuiltinOp::IncludeString, // now Pure Julia include_string() (Issue #3738)
            BuiltinOp::EvalFile,      // now Pure Julia evalfile() (Issue #3738)
        ];
        for op in &dead_but_kept {
            reachable.insert(*op);
        }

        // All expected BuiltinOp variants (must match the enum definition in
        // subset_julia_vm_types/src/ir/core.rs)
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
            BuiltinOp::MersenneTwisterRNG,
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
            BuiltinOp::Isbitstype,
            BuiltinOp::Supertype,
            BuiltinOp::Typename,
            BuiltinOp::FunctionName,
            BuiltinOp::Subtypes,
            BuiltinOp::Objectid,
            BuiltinOp::Isunordered,
            BuiltinOp::Methods,
            BuiltinOp::HasMethod,
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
            BuiltinOp::TestRecordError,
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
            76, // Must match the actual enum variant count (Issue #6738: -Isbits/-Hasfield/-Ismutable; Issue #7306: +MersenneTwisterRNG; Issue #10093: +TestRecordError)
            "all_variants list count mismatch — update this test when adding/removing BuiltinOp variants"
        );
    }

    #[test]
    fn test_method_dispatch_first_base_functions_keep_builtin_fallbacks() {
        for name in [
            "sqrt",
            "length",
            "getindex",
            "keys",
            "values",
            "pairs",
            "push!",
            "pop!",
            "pushfirst!",
            "popfirst!",
            "insert!",
            "deleteat!",
            "ncodeunits",
            "codeunit",
            "codeunits",
            "string",
            "String",
            "Char",
            "Int",
            "delete!",
            "convert",
            "promote",
            "signed",
        ] {
            assert!(
                is_method_dispatch_first_base_function(name),
                "{name} should route through method dispatch before builtin fallback"
            );
        }

        for name in ["open", "readline", "Regex", "rand", "time_ns"] {
            assert!(
                !is_method_dispatch_first_base_function(name),
                "{name} is a runtime boundary or direct builtin, not dispatch-first"
            );
        }
    }

    #[test]
    fn test_base_function_routes_are_classified_and_documented() {
        let mut names = HashSet::new();
        for route in BASE_FUNCTION_ROUTES {
            assert!(
                names.insert(route.name),
                "duplicate BASE_FUNCTION_ROUTES entry for {}",
                route.name
            );
            assert!(
                !route.upstream_ref.is_empty() && route.upstream_ref.starts_with("julia/"),
                "{} must document a ./julia upstream source reference",
                route.name
            );
        }

        for name in [
            "length",
            "collect",
            "push!",
            "pushfirst!",
            "popfirst!",
            "insert!",
            "deleteat!",
            "empty!",
            "getindex",
            "setindex!",
            "ncodeunits",
            "codeunit",
            "codeunits",
            "eltype",
            "sizeof",
            "hasfield",
            "isbits",
            "isbitstype",
            "ismutable",
            "objectid",
            "hasmethod",
            "bitstring",
            "codepoint",
            "isnumeric",
            "unescape_string",
            "parse",
            "tryparse",
            "String",
            "string",
            "convert",
            "promote",
            "Char",
            "Int",
            "in",
            "sprintf",
        ] {
            let route = base_function_route(name);
            assert!(
                route.is_some(),
                "{name} should be classified in BASE_FUNCTION_ROUTES"
            );
            if let Some(route) = route {
                assert_eq!(route.kind, BaseRouteKind::DispatchFirst);
            }
        }

        for name in [
            "_string",
            "_string_from_chars",
            "_char_to_int",
            "_int_to_char",
        ] {
            let route = base_function_route(name);
            assert!(
                route.is_some(),
                "{name} should be classified in BASE_FUNCTION_ROUTES"
            );
            if let Some(route) = route {
                assert_eq!(route.kind, BaseRouteKind::InternalIntrinsic);
            }
        }
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
            // nextfloat/prevfloat removed — Pure Julia (base/float.jl, Issue #6740).
            // Bit CPU intrinsics — public count_ones/leading_zeros/trailing_zeros/
            // bitreverse/bswap are pure Julia (Issue #6741); count_zeros/
            // leading_ones/trailing_ones/bitrotate too (Issue #6722).
            "_ctpop_int",
            "_ctlz_int",
            "_cttz_int",
            "_bitreverse_int",
            "_bswap_int",
            // exponent/significand/frexp/issubnormal removed — Pure Julia (base/float.jl, Issue #6740).
            // Note: maxintfloat, fma, muladd removed — Pure Julia (Issue #3732).
            // Internal `_fma` intrinsic preserves IEEE fused semantics on Float64.
            "_fma",
            "_neg_any",
            // Array
            "similar",
            "_mark_bitvector",
            "reshape",
            "length",
            "size",
            "ndims",
            "eltype",
            "keytype",
            "valtype",
            "memoryref",
            "memoryrefnew",
            "memoryrefget",
            "memoryrefset!",
            "memoryrefoffset",
            "memoryrefparent",
            "push!",
            "pop!",
            "pushfirst!",
            "popfirst!",
            "insert!",
            "deleteat!",
            "append!",
            "prepend!",
            // sort: Now Pure Julia (base/sort.jl) — Issue #3725
            // findfirst/findall: dead BuiltinId variants removed (Issue #6745);
            // now pure Julia (base/array.jl).
            // HOF
            "any",
            "all",
            "count",
            // Note: "ntuple" removed from inventory — Pure Julia (base/tuple.jl,
            // Issue #4973). No longer routed through BuiltinId; the direct-call
            // fast path lives in compile/expr/builtin_hof.rs.
            "_compose",
            // Range
            "range",
            "collect",
            "LinRange",
            // Note: "complex" removed — Pure Julia (base/complex.jl, Issue #3727)
            // String
            "_string",
            "_string_from_chars",
            "sprintf",
            "ncodeunits",
            "codeunit",
            "occursin",
            "_char_to_int",
            "_int_to_char",
            // codepoint, bitstring removed - now Pure Julia (Issue #6747)
            // unescape_string removed - now Pure Julia (Issue #6724)
            // isnumeric removed - now Pure Julia (Issue #6752)
            // Float parse intrinsic; public parse/tryparse(Float64) pure (#6748)
            "_tryparse_float64",
            // TwicePrecision float range intrinsic; public range(start, stop;
            // length) is pure Julia (base/range.jl, Issue #9419)
            "_linspace_range_f64",
            // Complex-scaled TwicePrecision range broadcast (Issue #9659)
            "_try_complex_scale_tp_range_f64",
            // Bulk typed-kernel broadcast (Issues #9693/#8797)
            "_try_broadcast_typed_kernel",
            "_try_broadcast_binary_arith",
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
            "_sjulia_backtrace",
            "_sjulia_catch_backtrace",
            "_sjulia_stacktrace",
            "normpath",
            "abspath",
            "homedir",
            // File I/O
            "readlines",
            "eachline",
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
            "isunordered",
            // Equality
            "isequal",
            "isless",
            "hash",
            "_not_egal",
            ">:",
            // Set
            "in",
            // Note: union/intersect/setdiff/symdiff/issubset/isdisjoint/issetequal
            // and mutating variants now Pure Julia (base/set.jl) — Issue #3724
            // Conversion
            "convert",
            "promote",
            "signed",
            "unsigned",
            // Note: "float" / "widemul" removed from from_name() — Pure Julia
            // (Issue #3727 / #6737).
            "reinterpret",
            // Copy
            "_deepcopy",
            // Reflection
            "_fieldnames",
            "_fieldtypes",
            "_fieldoffset",
            "_datatype_alignment",
            "_allocatedinline",
            "_getfield",
            "_isabstracttype",
            "_isconcretetype",
            "_ismutabletype",
            "_isprimitivetype",
            "_type_parameters",
            "_supertype",
            "_typename",
            "_function_name",
            "_methods_by_ftype",
            // Hash/Eltype internal intrinsics (Issue #2570, #2582)
            "_hash",
            "_eltype",
            // Tuple-type construction intrinsic (Issue #5119)
            "_make_tuple_type",
            "getfield",
            "setfield!",
            "names",
            "isexported",
            "ispublic",
            "_isdefined_module_binding",
            "_isdefined_binding_field",
            "_module_name",
            // Dict internal carrier intrinsics removed with Value::Dict (Issue #6731)
            // Set internal intrinsics (Issue #2574)
            // Tuple
            "first",
            "last",
            // Internal aliases used by Pure Julia for Value::Range access
            "_tuple_first",
            "_tuple_last",
            "_range_step",
            // Dict — struct-dispatch trampolines (Issue #6731). Dict/merge route
            // through pure-Julia methods with no builtin mapping.
            "keys",
            "values",
            "pairs",
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
            "_ref_new",
            "_ref_get",
            // Zero/One
            "zero",
            "one",
            // Numeric Type Constructors
            "_to_int8",
            "_to_int16",
            "_to_int32",
            "Int64",
            "_to_int128",
            "_to_uint8",
            "_to_uint16",
            "_to_uint32",
            "_to_uint64",
            "_to_uint128",
            "_to_float16",
            "_to_float32",
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
            "_nonmissingtype",
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
            "_test_record_error!",
            "_testset_begin!",
            "_testset_end!",
            // Regex
            "Regex",
            "match",
            "eachmatch",
            "_regex_replace",
            "_expand_substitution",
            "_regex_match_from",
            "_task_register_main",
            "_task_schedule",
            "_task_yield",
            "_task_park",
            "_task_wake",
            "_task_current",
            "_endswith_regex",
            "_regex_findnext",
            // Printf
            "_printf_fmt_float",
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
            // Public type constructors handled by type dispatch path, not is_base_function().
            // Fixed-width public constructors route through pure-Julia wrappers to the
            // underscored conversion boundaries exempted below (Issue #8777).
            "Int64",
            "Float64",
            "BigInt",
            "BigFloat",
            "Dict",
            "Regex",
            "LinRange",
            // Public array constructors migrated to Pure Julia dispatch but
            // retained in BuiltinId for compatibility with old bytecode.
            "zeros",
            "ones",
            // Internal intrinsics — prefixed with underscore, not callable from Julia
            "_fieldnames",
            "_fieldtypes",
            "_fieldoffset",
            "_datatype_alignment",
            "_allocatedinline",
            "_getfield",
            "_isabstracttype",
            "_isconcretetype",
            "_ismutabletype",
            "_isprimitivetype",
            "_type_parameters",
            // Module-binding probe backing function-form isdefined (Issue #5002/#4958)
            "_isdefined_module_binding",
            // Core.Binding-field probe backing function-form isdefined (Issue #10067)
            "_isdefined_binding_field",
            // Module-name probe backing Pure Julia nameof(::Module) (Issue #11171)
            "_module_name",
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
            "_neg_any",
            "_not_egal",
            "_to_int8",
            "_to_int16",
            "_to_int32",
            "_to_int128",
            "_to_uint8",
            "_to_uint16",
            "_to_uint32",
            "_to_uint64",
            "_to_uint128",
            "_to_float16",
            "_to_float32",
            "_test_record!",
            "_test_record_broken!",
            "_test_record_error!",
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
            // Note: "float" / "widemul" removed from inventory — Pure Julia
            // (Issue #3727 / #6737).
            "reinterpret",
            // Note: "signed" / "unsigned" added to is_base_function (Issue #3727)
            // so dispatch failure for unsupported argument types falls back to
            // the Rust BuiltinId handler. They are no longer exempted.
            // Note: "complex" removed from inventory — Pure Julia (Issue #3727).
            "similar",
            "reshape",
            // Note: "ntuple" removed — Pure Julia (base/tuple.jl, Issue #4973).
            // Note: "compose" removed — Pure Julia wrapper over _compose (Issue #8779).
            // "sort": Now Pure Julia (base/sort.jl) — Issue #3725
            "append!",
            "prepend!",
            // findfirst/findall removed - dead BuiltinId variants gone (Issue #6745)
            // Note: "deepcopy" removed — Pure Julia wrapper over _deepcopy (Issue #8779).
            // nextfloat/prevfloat removed — Pure Julia (base/float.jl, Issue #6740).
            // Bit ops: the underscored CPU intrinsics _ctpop_int/_ctlz_int/
            // _cttz_int/_bitreverse_int/_bswap_int are in is_base_function();
            // the public count_ones/leading_zeros/trailing_zeros/bitreverse/bswap
            // (and count_zeros/leading_ones/trailing_ones/bitrotate) are pure
            // Julia (base/int.jl, Issues #6741/#6722) and need no exemption.
            // exponent/significand/frexp/issubnormal removed — Pure Julia (base/float.jl, Issue #6740).
            // Note: maxintfloat, fma, muladd removed — Pure Julia (Issue #3732).
            // _fma is an internal intrinsic registered in is_base_function(), no exemption needed.
            "normpath",
            "abspath",
            "homedir",
            "merge",       // Pure Julia (Issue #2573), no longer in is_base_function()
            "isunordered", // Pure Julia (Issue #2715), no longer in is_base_function()
            "one", // BuiltinId::One exists but not in is_base_function (Pure Julia covers most)
            "inv", // Handled specially in call.rs (matrix vs rational)
            // Operator builtins — matched by operator dispatch, not function name
            ">:",
            "\\",
            // File I/O — compile-time routed, not through is_base_function()
            // Note: readline is in is_base_function() — not exempted
            "readlines",
            "eachline",
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
            // Note: "nonmissingtype" removed — Pure Julia wrapper over _nonmissingtype (Issue #8779).
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
            // first, last — now Pure Julia (Issue #3734); BuiltinId still resolves
            // for legacy/specialize paths but the public name no longer routes here
            "first",
            "last",
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
