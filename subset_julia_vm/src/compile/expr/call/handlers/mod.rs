//! Table-driven special-case call handlers for `compile_call` (Issue #6332).
//!
//! Upstream Julia registers per-function compiler handling in tables
//! (`julia/Compiler/src/tfuncs.jl`, `add_tfunc`) instead of one monolithic
//! if/else chain. This module mirrors that design: [`special_case_handler`]
//! maps a function name to a handler with the uniform [`CallHandler`]
//! signature, and `compile_call` consults the table at the single point
//! where the extracted branches originally lived (immediately before the
//! big `match function` block).
//!
//! Semantics-preservation rules for migrating a branch out of
//! `compile_call`:
//! - Only branches keyed purely on the function name at their original
//!   decision point may move into the table. Order-dependent preprocessing
//!   (splat handling, callable-variable resolution, parametric-constructor
//!   parsing, ...) stays in `compile_call`.
//! - Multiple original branches for the same name are replayed inside one
//!   handler in their original relative order.
//! - `None` means "special case not applicable": control falls through to
//!   the code after the dispatch point, exactly like the original
//!   non-returning match arms / failed `if` conditions.

mod arrays;
mod collections;
mod early;
mod internals;
mod math;
mod misc;
mod strings;

use crate::compile::{CResult, CoreCompiler};
use crate::ir::core::Expr;
use crate::vm::ValueType;

/// Borrowed call-site context passed to every special-case handler.
#[derive(Debug)]
pub(super) struct CallCtx<'a> {
    pub(super) function: &'a str,
    pub(super) args: &'a [Expr],
    pub(super) kwargs: &'a [(String, Expr)],
    /// Not yet read by the migrated handlers; reserved for handlers moved in
    /// later migration rounds (Issue #6332).
    #[allow(dead_code)]
    pub(super) splat_mask: &'a [bool],
    pub(super) kwargs_splat_mask: &'a [bool],
    pub(super) has_splat: bool,
    pub(super) has_kwargs_splat: bool,
}

/// Uniform handler signature: `Some(result)` = the special case applied and
/// `compile_call` returns it; `None` = not applicable, fall through to the
/// generic call path.
pub(super) type CallHandler = fn(&mut CoreCompiler<'_>, &CallCtx<'_>) -> Option<CResult<ValueType>>;

/// Propagates a `CResult` error out of a handler as `Some(Err(..))`,
/// mirroring the `?` operator used by the original `compile_call` body.
macro_rules! ctry {
    ($expr:expr) => {
        match $expr {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    };
}
pub(super) use ctry;

/// Earliest dispatch table, consulted at the very top of `compile_call`
/// (right after the splat-mask flags are computed, before the enum
/// pre-pass, the splat block, and callable-variable resolution). Hosts the
/// name-keyed if-chain that originally lived at that position.
pub(super) fn early_special_case_handler(name: &str) -> Option<CallHandler> {
    Some(match name {
        "#__sjulia_boundscheck_enabled__" => early::compile_boundscheck_enabled,
        "#__sjulia_inbounds__" => early::compile_inbounds,
        "#__sjulia_inline__" | "#__sjulia_noinline__" => early::compile_inline_metadata,
        "print" | "Base.print" | "println" | "Base.println" => early::compile_print_println,
        "hasmethod" => early::compile_hasmethod_world,
        "invoke" | "Base.invoke" => early::compile_invoke,
        "merge" | "Base.merge" => early::compile_merge,
        _ => return None,
    })
}

/// Single-level table dispatch from function name to special-case handler.
pub(super) fn special_case_handler(name: &str) -> Option<CallHandler> {
    Some(match name {
        "pop!" | "Base.pop!" => collections::compile_pop,
        "popfirst!" | "Base.popfirst!" => collections::compile_popfirst,
        "push!" | "Base.push!" => collections::compile_push,
        "pushfirst!" | "Base.pushfirst!" => collections::compile_pushfirst,
        "insert!" | "Base.insert!" => collections::compile_insert,
        "deleteat!" | "Base.deleteat!" => collections::compile_deleteat,
        "delete!" | "Base.delete!" => collections::compile_delete,
        "empty!" | "Base.empty!" => collections::compile_empty,
        "merge!" | "Base.merge!" => collections::compile_merge_bang,
        "get!" | "Base.get!" => collections::compile_get_bang,
        "keys" | "Base.keys" | "values" | "Base.values" | "pairs" | "Base.pairs" => {
            collections::compile_keys_values_pairs
        }
        "haskey" | "Base.haskey" => collections::compile_haskey,
        "keytype" | "Base.keytype" | "valtype" | "Base.valtype" => {
            collections::compile_keytype_valtype
        }
        "in" | "Base.in" => collections::compile_in,
        "∈" | "Base.∈" => collections::compile_elem_of,
        "∉" | "Base.∉" | "∋" | "Base.∋" | "∌" | "Base.∌" => {
            collections::compile_membership_aliases
        }
        "tuple" => misc::compile_tuple,
        "!" => misc::compile_not,
        "NamedTuple" => misc::compile_empty_named_tuple,
        "ntuple" => misc::compile_ntuple,
        "rethrow" | "Base.rethrow" => misc::compile_rethrow,
        "occursin" => strings::compile_occursin,
        "match" => strings::compile_regex_match,
        "eachmatch" => strings::compile_regex_eachmatch,
        "inv" => math::compile_inv,
        "\\" => math::compile_left_division,
        // `hash` is no longer force-intercepted (Issue #6728): it dispatches
        // through normal Julia method dispatch to the pure-Julia `hash` methods
        // (base/hashing.jl), so user `hash(::T)` overloads are respected — like
        // `isequal`/`isless`, which already dispatch purely.
        "convert" | "Base.convert" => misc::compile_convert,
        "promote" => misc::compile_promote,
        // widemul removed - now Pure Julia (base/number.jl, Issue #6737)
        "reinterpret" => misc::compile_reinterpret,
        "deepcopy" => misc::compile_deepcopy,
        // Dict/Set/Array/Vector constructor interceptions originally lived
        // immediately AFTER the big `match function` block. Hosting them in
        // this pre-match table is order-preserving because no remaining
        // match arm is keyed on (or has side effects for) these names, so
        // the match was a provable no-op for them.
        "Dict" => collections::compile_dict_constructor_call,
        "Set" => collections::compile_set_constructor_call,
        "Array" | "Vector" | "Matrix" => arrays::compile_array_vector_constructor,
        "getindex" | "setindex!" => arrays::compile_getindex_setindex,
        "reshape" | "Base.reshape" => arrays::compile_reshape,
        "similar" | "Base.similar" => arrays::compile_similar,
        "collect_similar" | "Base.collect_similar" => arrays::compile_collect_similar,
        "collect" => arrays::compile_collect,
        "_fieldnames"
        | "_compose_exception_type"
        | "_return_types_by_ftype"
        | "_fieldtypes"
        | "_fieldoffset"
        | "_datatype_alignment"
        | "_allocatedinline"
        | "_getfield"
        | "_isabstracttype"
        | "_isconcretetype"
        | "_ismutabletype"
        | "_isprimitivetype"
        | "_isstructtype"
        | "_typeintersect"
        | "_type_parameters"
        | "_make_tuple_type"
        | "_hash"
        | "_eltype"
        | "_dict_get"
        | "_dict_set!"
        | "_dict_delete!"
        | "_dict_haskey"
        | "_dict_length"
        | "_dict_empty!"
        | "_dict_keys"
        | "_dict_values"
        | "_dict_pairs"
        | "_set_push!"
        | "_set_delete!"
        | "_set_in"
        | "_set_empty!"
        | "_set_length"
        | "getfield"
        | "setfield!"
        | "_test_record!"
        | "_test_record_broken!"
        | "_testset_begin!"
        | "_testset_end!"
        | "_regex_replace"
        | "_endswith_regex"
        | "_printf_fmt_float" => internals::compile_internal_intrinsic,
        _ => return None,
    })
}

/// Second dispatch table, consulted after the struct-constructor
/// resolution chain (parametric constructors, `struct_table` direct
/// constructors, `resolve_parametric_struct_name`) and immediately before
/// the generic method-table dispatch. Hosts the name-keyed if-chain
/// (`sprint` ... `length`) that originally lived at exactly that position;
/// it cannot share the pre-match table because a user struct constructor
/// named like one of these functions must keep winning first.
pub(super) fn post_struct_special_case_handler(name: &str) -> Option<CallHandler> {
    Some(match name {
        "sprint" => strings::compile_sprint,
        // Note: floor/ceil/round/trunc digits/sigdigits/base kwargs are now pure
        // Julia (base/floatfuncs.jl, Issue #6742) — they dispatch to the keyword
        // methods there instead of the former Rust *Digits/*SigDigits builtins.
        "string" => strings::compile_string_base_kwarg,
        "mapreduce" | "mapfoldl" | "mapfoldr" | "Base.mapreduce" | "Base.mapfoldl"
        | "Base.mapfoldr" => misc::compile_mapreduce_init_kwarg,
        "reduce" | "foldl" | "foldr" | "Base.reduce" | "Base.foldl" | "Base.foldr" => {
            misc::compile_reduce_init_kwarg
        }
        "parse" | "tryparse" => strings::compile_parse_tryparse,
        "sqrt" | "Base.sqrt" => math::compile_sqrt,
        "eltype" => misc::compile_eltype_query,
        "length" => misc::compile_length_pairs,
        _ => return None,
    })
}
