//! Internal-intrinsic special-case handlers extracted from `compile_call`
//! (Issue #6332): the `_`-prefixed compiler intrinsics backing Pure Julia
//! Base code (reflection, Dict/Set storage, hashing, test recording, regex
//! helpers) plus the public `getfield` / `setfield!` field-access builtins.
//!
//! Every original arm in this group unconditionally returned (each is an
//! arity check followed by argument compilation and one `CallBuiltin`), so
//! the handler always produces `Some(..)` for a registered name.

use crate::builtins::BuiltinId;
use crate::bytecode::{Instr, ValueType};
use crate::compile::{err, CResult, CoreCompiler};

use super::{ctry, CallCtx};

/// One table-driven handler for all internal intrinsics: exact arity check
/// (with the original error message), compile each argument in order, emit
/// `CallBuiltin`, and return the original static result type.
pub(super) fn compile_internal_intrinsic(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    // (arity, builtin, return type, arity-mismatch error message), in the
    // original match-arm order. See the per-intrinsic notes below.
    let (arity, builtin, return_type, arity_error): (usize, BuiltinId, ValueType, &str) =
        match ctx.function {
            // _fieldnames(T) - internal builtin for tuple of field names
            "_fieldnames" => (
                1,
                BuiltinId::_Fieldnames,
                ValueType::Tuple,
                "_fieldnames requires exactly 1 argument",
            ),
            // _compose_exception_type(f, types) - interprocedural exception
            // type composed from a user function body's callees (Issue #5600).
            "_compose_exception_type" => (
                2,
                BuiltinId::ComposeExceptionType,
                ValueType::Any,
                "_compose_exception_type requires exactly 2 arguments",
            ),
            // _compose_effects(f, types) - body-derived effect summary for
            // matched user methods (Issue #8441).
            "_compose_effects" => (
                2,
                BuiltinId::ComposeEffects,
                ValueType::Any,
                "_compose_effects requires exactly 2 arguments",
            ),
            // _return_types_by_ftype(f, types) - return-type reflection
            // through method-table dispatch (Issue #5603).
            "_return_types_by_ftype" => (
                2,
                BuiltinId::_ReturnTypesByFtype,
                ValueType::Array,
                "_return_types_by_ftype requires exactly 2 arguments",
            ),
            // _fieldtypes(T) - internal builtin for tuple of field types
            "_fieldtypes" => (
                1,
                BuiltinId::_Fieldtypes,
                ValueType::Tuple,
                "_fieldtypes requires exactly 1 argument",
            ),
            // _fieldoffset(T, i) - internal builtin for field byte offset
            "_fieldoffset" => (
                2,
                BuiltinId::_Fieldoffset,
                ValueType::U64,
                "_fieldoffset requires exactly 2 arguments",
            ),
            // _datatype_alignment(T) - internal builtin for a type's byte
            // alignment (Issue #5107)
            "_datatype_alignment" => (
                1,
                BuiltinId::_DatatypeAlignment,
                ValueType::I64,
                "_datatype_alignment requires exactly 1 argument",
            ),
            // _allocatedinline(T) - internal builtin: whether T is stored
            // inline (unboxed) in a container (Issue #5107)
            "_allocatedinline" => (
                1,
                BuiltinId::_Allocatedinline,
                ValueType::Bool,
                "_allocatedinline requires exactly 1 argument",
            ),
            // _getfield(x, i) - internal builtin for runtime field access by index
            // This is used by dump() to access struct fields at runtime
            "_getfield" => (
                2,
                BuiltinId::_Getfield,
                ValueType::Any,
                "_getfield requires exactly 2 arguments: _getfield(x, i)",
            ),
            // _isabstracttype(T) - internal intrinsic for type classification
            "_isabstracttype" => (
                1,
                BuiltinId::_Isabstracttype,
                ValueType::Bool,
                "_isabstracttype requires exactly 1 argument",
            ),
            // _isconcretetype(T) - internal intrinsic for type classification
            "_isconcretetype" => (
                1,
                BuiltinId::_Isconcretetype,
                ValueType::Bool,
                "_isconcretetype requires exactly 1 argument",
            ),
            // _ismutabletype(T) - internal intrinsic for type classification
            "_ismutabletype" => (
                1,
                BuiltinId::_Ismutabletype,
                ValueType::Bool,
                "_ismutabletype requires exactly 1 argument",
            ),
            // _isprimitivetype(T) - internal intrinsic for type classification (Issue #3767)
            "_isprimitivetype" => (
                1,
                BuiltinId::_Isprimitivetype,
                ValueType::Bool,
                "_isprimitivetype requires exactly 1 argument",
            ),
            // _isstructtype(T) - internal intrinsic for type classification
            "_isstructtype" => (
                1,
                BuiltinId::_Isstructtype,
                ValueType::Bool,
                "_isstructtype requires exactly 1 argument",
            ),
            // _typeintersect(a, b) - internal intrinsic for type intersection
            "_typeintersect" => (
                2,
                BuiltinId::_Typeintersect,
                ValueType::DataType,
                "_typeintersect requires exactly 2 arguments",
            ),
            // _type_equal(a, b) - semantic type equality for type objects
            "_type_equal" => (
                2,
                BuiltinId::_TypeEqual,
                ValueType::Bool,
                "_type_equal requires exactly 2 arguments",
            ),
            // _type_parameters(T) - internal intrinsic for DataType parameter access
            "_type_parameters" => (
                1,
                BuiltinId::_TypeParameters,
                ValueType::Tuple,
                "_type_parameters requires exactly 1 argument",
            ),
            // _make_tuple_type(types) - construct `Tuple{types...}` from a
            // runtime collection of type objects, backing Pure Julia
            // tuple_type_tail/cons which cannot splat into `Tuple{...}`
            // (Issue #5119).
            "_make_tuple_type" => (
                1,
                BuiltinId::_MakeTupleType,
                ValueType::DataType,
                "_make_tuple_type requires exactly 1 argument",
            ),
            // Hash intrinsic (Issue #2582)
            // _hash(x) - internal intrinsic for hash computation
            "_hash" => (
                1,
                BuiltinId::_Hash,
                ValueType::I64,
                "_hash requires exactly 1 argument",
            ),
            // Eltype intrinsic (Issue #2570)
            // _eltype(x) - internal intrinsic for element type
            "_eltype" => (
                1,
                BuiltinId::_Eltype,
                ValueType::DataType,
                "_eltype requires exactly 1 argument",
            ),
            // Dict carrier intrinsics (`_dict_get`/`_dict_set!`/…) were removed
            // with `Value::Dict` (Issue #6731): `Dict` is now a pure-Julia
            // `Dict{K,V}` struct, so these HashMap intrinsics have no callers.
            // Set carrier intrinsics (`_set_push!`/`_set_in`/…) were removed with
            // `Value::Set` (Issue #6732): `Set` is a pure-Julia struct over
            // `Dict{T,Nothing}`, so these HashSet intrinsics have no callers.
            // getfield(x, name) or getfield(x, i) - get field by name (Symbol) or index (Int)
            // This is the public Julia API for field access
            "getfield" => (
                2,
                BuiltinId::Getfield,
                ValueType::Any,
                "getfield requires exactly 2 arguments: getfield(x, name) or getfield(x, i)",
            ),
            // setfield!(x, name, v) or setfield!(x, i, v) - set field by name (Symbol) or index (Int)
            // This is the public Julia API for field mutation
            "setfield!" => (
                3,
                BuiltinId::Setfield,
                ValueType::Any,
                "setfield! requires exactly 3 arguments: setfield!(x, name, v) or setfield!(x, i, v)",
            ),
            // Test builtins - for Pure Julia @test/@testset/@test_throws macros
            "_test_record!" => (
                2,
                BuiltinId::TestRecord,
                ValueType::Nothing,
                "_test_record! requires exactly 2 arguments",
            ),
            "_test_record_broken!" => (
                2,
                BuiltinId::TestRecordBroken,
                ValueType::Nothing,
                "_test_record_broken! requires exactly 2 arguments",
            ),
            // _test_record_error!(msg, detail) — errored `@test` outcome (Issue #10093)
            "_test_record_error!" => (
                2,
                BuiltinId::TestRecordError,
                ValueType::Nothing,
                "_test_record_error! requires exactly 2 arguments",
            ),
            "_testset_begin!" => (
                1,
                BuiltinId::TestSetBegin,
                ValueType::Nothing,
                "_testset_begin! requires exactly 1 argument",
            ),
            "_testset_end!" => (
                0,
                BuiltinId::TestSetEnd,
                ValueType::Nothing,
                "_testset_end! takes no arguments",
            ),
            // Regex replace builtin (Issue #2112)
            "_regex_replace" => (
                4,
                BuiltinId::RegexReplace,
                ValueType::Str,
                "_regex_replace requires 4 arguments: _regex_replace(string, regex, replacement, count)",
            ),
            // SubstitutionString capture-reference expansion (Issue #10174)
            "_expand_substitution" => (
                3,
                BuiltinId::ExpandSubstitution,
                ValueType::Str,
                "_expand_substitution requires 3 arguments: _expand_substitution(subst, match, regex)",
            ),
            // findnext(re, str, i) primitive for the multi-pattern replace scan (Issue #10175)
            "_regex_match_from" => (
                3,
                BuiltinId::RegexMatchFrom,
                ValueType::Any,
                "_regex_match_from requires 3 arguments: _regex_match_from(regex, string, byteindex)",
            ),
            // _endswith_regex(string, regex) — internal helper for the pure-Julia
            // endswith(s, ::Regex) method (Issue #5676).
            "_endswith_regex" => (
                2,
                BuiltinId::EndsWithRegex,
                ValueType::Bool,
                "_endswith_regex requires 2 arguments: _endswith_regex(string, regex)",
            ),
            // _regex_findnext(regex, string, i) — internal helper for the pure-Julia
            // findnext(::Regex, s, i) / findfirst(::Regex, s) methods (Issue #10177).
            "_regex_findnext" => (
                3,
                BuiltinId::RegexFindnext,
                ValueType::Any, // Returns RegexMatch or Nothing
                "_regex_findnext requires 3 arguments: _regex_findnext(regex, string, i)",
            ),
            // VM-owned continuation/session-state boundaries (Issue #10349).
            "_task_register_main" => (
                1,
                BuiltinId::TaskRegisterMain,
                ValueType::I64,
                "_task_register_main requires exactly 1 argument",
            ),
            "_task_schedule" => (
                2,
                BuiltinId::TaskSchedule,
                ValueType::I64,
                "_task_schedule requires exactly 2 arguments",
            ),
            "_task_yield" => (
                0,
                BuiltinId::TaskYield,
                ValueType::Nothing,
                "_task_yield takes no arguments",
            ),
            "_task_park" => (
                0,
                BuiltinId::TaskPark,
                ValueType::Nothing,
                "_task_park takes no arguments",
            ),
            "_task_wake" => (
                1,
                BuiltinId::TaskWake,
                ValueType::Nothing,
                "_task_wake requires exactly 1 argument",
            ),
            "_task_current" => (
                0,
                BuiltinId::TaskCurrent,
                ValueType::Any,
                "_task_current takes no arguments",
            ),
            // Printf float→string boundary for the pure-Julia engine (Issue #6746).
            "_printf_fmt_float" => (
                3,
                BuiltinId::PrintfFmtFloat,
                ValueType::Str,
                "_printf_fmt_float requires 3 arguments: _printf_fmt_float(x, conv, precision)",
            ),
            _ => return None,
        };
    if args.len() != arity {
        return Some(err(arity_error));
    }
    for arg in args {
        ctry!(c.compile_expr(arg));
    }
    c.emit(Instr::CallBuiltin(builtin, arity));
    Some(Ok(return_type))
}
