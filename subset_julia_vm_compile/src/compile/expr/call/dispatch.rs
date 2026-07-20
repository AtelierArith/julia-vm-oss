//! Generic method-table dispatch tail of `compile_call` (Issue #6332).
//!
//! This module hosts the former tail of `compile_call` — the generic
//! multiple-dispatch path over `method_tables` (including its dispatch-error
//! fallback arms and per-function return-type overrides) and the
//! no-method-table builtin fallback. It was moved here **verbatim** as a pure
//! extraction: `compile_call` tail-calls
//! [`CoreCompiler::compile_generic_dispatch_call`] after all table-driven
//! special-case handlers and constructor resolution have fallen through, so
//! evaluation order and behavior are unchanged.

use std::collections::HashSet;

use crate::builtins::BuiltinId;
use crate::bytecode::{DynamicCallCandidate, Instr, ValueType};
use crate::inference_core::CoreType;
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::types::{nominal_family_name, nominal_family_names_compatible, JuliaType};

use crate::compile::{
    base_function_to_builtin_op, err, is_base_function, is_builtin_type_name,
    is_method_dispatch_first_base_function, is_random_function, is_reducible_nary_operator,
    julia_type_to_value_type, CResult, CompileError, CoreCompiler,
};

use super::{
    core_is_abstract_array_family_type, core_is_array_family_type, is_rank_unknown_array_julia_type,
};

type MethodTable = crate::compile::method_table::MethodTable;

pub(super) fn is_dict_annotation(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Dict)
        || matches!(ty, JuliaType::Struct(name) if name.split('{').next() == Some("Dict"))
}

fn is_truncated_result_call(
    function: &str,
    args: &[Expr],
    kwargs: &[(crate::ir::core::InternedStr, Expr)],
) -> bool {
    matches!(function, "truncated" | "Distributions.truncated")
        && (args.len() >= 2
            || kwargs
                .iter()
                .any(|(_, value)| !matches!(value, Expr::Literal(Literal::Nothing, _))))
}

pub(super) fn is_runtime_unknown_struct_arg(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Struct(name)
        if !is_callable_singleton_struct_name(name) && !is_native_range_runtime_struct_name(name))
}

fn is_callable_singleton_struct_name(name: &str) -> bool {
    name.starts_with("typeof(") && name.ends_with(')')
}

fn is_native_range_runtime_struct_name(name: &str) -> bool {
    matches!(nominal_family_name(name), "UnitRange" | "StepRange")
}

/// True when `ty` is a `Union` with at least one member that is itself a type
/// object (`DataType`, or `Type{T}`/`TypeOf(_)`), e.g. the inferred return
/// type of `cond ? Float64 : BigFloat` (Issue #9955).
///
/// Every member of such a union is a concrete, dispatchable type object at
/// runtime — the union only reflects that static inference could not pin
/// down *which* branch's type object without evaluating `cond`. Static
/// dispatch resolution must not read this shape as a proof that no method
/// matches (a bare `Type{T} where T` catch-all, or any per-branch concrete
/// method, still successfully dispatches once the runtime value is known);
/// callers use this to route to runtime typed dispatch instead of emitting
/// a guaranteed `ThrowMethodError`.
fn julia_type_is_datatype_union(ty: &JuliaType) -> bool {
    let JuliaType::Union(members) = ty else {
        return false;
    };
    members
        .iter()
        .any(|member| matches!(member, JuliaType::DataType | JuliaType::TypeOf(_)))
}

fn type_object_dispatch_builtin_fallback(op: BuiltinOp) -> Option<(BuiltinId, ValueType)> {
    match op {
        BuiltinOp::Isbitstype => Some((BuiltinId::Isbitstype, ValueType::Bool)),
        BuiltinOp::Isa => Some((BuiltinId::Isa, ValueType::Bool)),
        BuiltinOp::Subtypes => Some((BuiltinId::Subtypes, ValueType::Any)),
        _ => None,
    }
}

fn is_range_family_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => true,
        JuliaType::Struct(name) => {
            let unqualified = name.rsplit('.').next().unwrap_or(name.as_str());
            let base = unqualified.split('{').next().unwrap_or(unqualified);
            matches!(
                base,
                "AbstractRange"
                    | "AbstractUnitRange"
                    | "UnitRange"
                    | "StepRange"
                    | "StepRangeLen"
                    | "LinRange"
                    | "OneTo"
            )
        }
        JuliaType::UnionAll { body, .. } => is_range_family_julia_type(body),
        _ => false,
    }
}

fn is_native_range_family_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::UnitRange | JuliaType::StepRange => true,
        JuliaType::Struct(name) => {
            let unqualified = name.rsplit('.').next().unwrap_or(name.as_str());
            let base = unqualified.split('{').next().unwrap_or(unqualified);
            matches!(base, "UnitRange" | "StepRange")
        }
        JuliaType::UnionAll { body, .. } => is_native_range_family_julia_type(body),
        _ => false,
    }
}

impl<'a> CoreCompiler<'a> {
    fn source_ordered_runtime_candidates(
        &self,
        method_table_name: &str,
        table: &MethodTable,
        arity: usize,
    ) -> Option<Vec<DynamicCallCandidate>> {
        if self.strict_undefined_check {
            let current_function_name = self.current_function_name.as_ref()?;
            if !self
                .shared_ctx
                .source_world_function_names
                .contains(current_function_name)
            {
                return None;
            }
        } else if !self.repl_source_ordered_top_level_dispatch {
            return None;
        }
        let entries = self
            .shared_ctx
            .source_ordered_method_sigs
            .get(method_table_name)?;
        if !entries
            .iter()
            .any(|entry| entry.visible_from_source_start.is_some())
        {
            return None;
        }
        let call_start = self.current_span()?.start;
        let source_visible = entries
            .iter()
            .filter(|entry| {
                entry.visible_from_source_start.is_some() && entry.sig.accepts_arity(arity)
            })
            .collect::<Vec<_>>();
        // A REPL delta installs every current-input body dormant, so any later
        // arity-compatible method must reach the VM's min-world fence. Ordinary
        // whole-program compilation retains the established same-signature
        // redefinition rule; some direct-pipeline callers merge their prelude
        // without Base provenance, so treating every later overload as a live
        // delta would misclassify those inherited methods (Issues #9784/#11477).
        let has_later_source_method = if self.shared_ctx.repl_source_ordered_dispatch {
            source_visible.iter().any(|entry| {
                entry
                    .visible_from_source_start
                    .is_some_and(|start| start > call_start)
            })
        } else {
            source_visible.iter().enumerate().any(|(idx, left)| {
                source_visible.iter().skip(idx + 1).any(|right| {
                    let pair_crosses_call_site = left
                        .visible_from_source_start
                        .is_some_and(|start| start > call_start)
                        || right
                            .visible_from_source_start
                            .is_some_and(|start| start > call_start);
                    pair_crosses_call_site
                        && left.sig.vararg_param_index == right.sig.vararg_param_index
                        && left.sig.vararg_fixed_count == right.sig.vararg_fixed_count
                        && left.sig.core_signature() == right.sig.core_signature()
                })
            })
        };
        if !has_later_source_method {
            return None;
        }

        let tracked_indices: HashSet<usize> =
            entries.iter().map(|entry| entry.sig.global_index).collect();
        let mut candidates = table
            .methods
            .iter()
            .filter(|method| {
                !tracked_indices.contains(&method.global_index) && method.accepts_arity(arity)
            })
            .map(|method| DynamicCallCandidate::Method(method.global_index))
            .collect::<Vec<_>>();
        candidates.extend(
            entries
                .iter()
                .filter(|entry| entry.sig.accepts_arity(arity))
                .map(|entry| DynamicCallCandidate::Method(entry.sig.global_index)),
        );
        (!candidates.is_empty()).then_some(candidates)
    }

    fn source_visible_method_table(
        &self,
        method_table_name: &str,
        table: &MethodTable,
    ) -> Option<MethodTable> {
        if self.strict_undefined_check {
            return None;
        }
        let call_start = self.current_span()?.start;
        let entries = self
            .shared_ctx
            .source_ordered_method_sigs
            .get(method_table_name)?;
        if !entries
            .iter()
            .any(|entry| entry.visible_from_source_start.is_some())
        {
            return None;
        }
        let has_later_source_method = entries.iter().any(|entry| {
            entry
                .visible_from_source_start
                .is_some_and(|start| start > call_start)
        });
        if !has_later_source_method {
            return None;
        }

        let mut hid_later_source_method = false;
        let tracked_indices: HashSet<usize> =
            entries.iter().map(|entry| entry.sig.global_index).collect();
        let retained_methods = table
            .methods
            .iter()
            .filter(|method| !tracked_indices.contains(&method.global_index))
            .cloned()
            .collect::<Vec<_>>();
        let mut visible = table.clone_with_methods_for_compile(retained_methods);
        for entry in entries {
            if entry
                .visible_from_source_start
                .is_some_and(|start| start > call_start)
            {
                hid_later_source_method = true;
                continue;
            }
            visible.add_method(entry.sig.clone());
        }

        hid_later_source_method.then_some(visible)
    }

    /// Whether ordinary dispatch for this call has one statically selected
    /// Base/prelude-owned winner.
    ///
    /// Constructor transfer functions may refine an `Any` result only when
    /// the method whose semantics they model actually wins (Issue #11434).
    pub(in crate::compile) fn base_owned_dispatch_wins(
        &self,
        function: &str,
        arg_types: &[JuliaType],
    ) -> bool {
        let lexical_owned_table = self.lexical_function_tables.get(function).cloned();
        let module_owned_table =
            lexical_owned_table.or_else(|| self.module_owned_function_table_name(function));
        let base_qualified_function;
        let method_table_name = if let Some(owned) = module_owned_table.as_deref() {
            owned
        } else if self.method_tables.contains_key(function) {
            function
        } else if is_method_dispatch_first_base_function(function) {
            base_qualified_function = format!("Base.{function}");
            if self.method_tables.contains_key(&base_qualified_function) {
                base_qualified_function.as_str()
            } else {
                function
            }
        } else {
            function
        };
        let Some(original_table) = self.method_tables.get(method_table_name) else {
            return false;
        };
        let source_visible_table =
            self.source_visible_method_table(method_table_name, original_table);
        let table = source_visible_table.as_ref().unwrap_or(original_table);
        let bare_constructor_table = (!function.contains('{')
            && self.resolve_parametric_struct_name(function).is_some()
            && table.has_explicit_parametric_inner_constructors())
        .then(|| {
            table.clone_with_methods_for_compile(
                table
                    .methods
                    .iter()
                    .filter(|method| {
                        !table.is_explicit_parametric_inner_constructor(method.global_index)
                    })
                    .cloned()
                    .collect(),
            )
        });
        let table = bare_constructor_table.as_ref().unwrap_or(table);
        if self
            .source_ordered_runtime_candidates(method_table_name, table, arg_types.len())
            .is_some()
            || (opaque_runtime_eval_targets_function(
                &self.shared_ctx.opaque_runtime_eval_function_names,
                function,
            ) && method_table_has_non_base_methods_for_opaque_eval(table))
        {
            return false;
        }
        let Ok(method) = table.dispatch(arg_types) else {
            return false;
        };
        let has_any_arg = arg_types.iter().any(|ty| matches!(ty, JuliaType::Any));
        !should_runtime_dispatch(table, method, arg_types, arg_types.len(), has_any_arg)
            && table.is_base_program_global_index(method.global_index)
    }
}

/// Whether a core parameter is `Family{..., value, ...}` for `family`, i.e. it
/// pins at least one *concrete value* type parameter (a `CoreType::Value` such
/// as the `0` in `SVector{0,T}`). Issue #8537.
fn core_struct_param_pins_concrete_value_param(param: &CoreType, family: &str) -> bool {
    matches!(param, CoreType::Struct { name, params }
        if nominal_family_name(name) == family
            && params.iter().any(|p| matches!(p, CoreType::Value(_))))
}

fn core_static_datatype_exact_match(actual: &CoreType, expected: &CoreType) -> bool {
    let mut bindings: std::collections::HashMap<String, CoreType> =
        std::collections::HashMap::new();
    core_static_datatype_exact_match_inner(actual, expected, false, &mut bindings)
}

/// Issue #11490: a `where`-bound type parameter that recurs across multiple
/// positions of a candidate signature (e.g. `Tuple{T,T}`, `Struct{T,T}`) must
/// bind to the SAME concrete type at every occurrence — upstream Julia's
/// diagonal rule. Each occurrence used to be checked independently (no shared
/// state between sibling `Tuple`/`Struct` element comparisons), so a repeated
/// type variable silently accepted any combination of concrete element types
/// (`Tuple{Int,Int}` and `Tuple{Int,String}` both "matched" `Tuple{T,T}`).
/// `bindings` threads the first concrete type each named type variable binds
/// to through the whole match tree; a later occurrence of the same name must
/// match that binding exactly. The anonymous placeholder name `"_"` (used for
/// bare covariant/contravariant bounds like `<:Number`, not a real shared
/// `where` parameter — see `parse_covariant_bound`/`parse_contravariant_bound`
/// in `subset_julia_vm_types`) is exempt: each occurrence is an independent
/// existential, not a repeated variable.
fn core_static_datatype_exact_match_inner(
    actual: &CoreType,
    expected: &CoreType,
    allow_typevar_param: bool,
    bindings: &mut std::collections::HashMap<String, CoreType>,
) -> bool {
    if actual == expected {
        return true;
    }

    match (actual, expected) {
        (CoreType::TypeVar(var), expected) if allow_typevar_param => {
            let lower_ok = var
                .lower_bound
                .as_deref()
                .is_none_or(|lower| lower.is_subtype_of(expected));
            let upper_ok = var
                .upper_bound
                .as_deref()
                .is_none_or(|upper| expected.is_subtype_of(upper));
            if !(lower_ok && upper_ok) {
                return false;
            }
            if var.name == "_" {
                // Anonymous covariant/contravariant bound placeholder: not a
                // shared binding, no cross-occurrence consistency required.
                return true;
            }
            match bindings.get(&var.name) {
                Some(bound) => bound == expected,
                None => {
                    bindings.insert(var.name.clone(), expected.clone());
                    true
                }
            }
        }
        (CoreType::TypeOf(actual), CoreType::TypeOf(expected))
        | (CoreType::Vararg(actual), CoreType::Vararg(expected)) => {
            core_static_datatype_exact_match_inner(actual, expected, false, bindings)
        }
        (
            CoreType::VarargLen {
                element: actual_element,
                len: actual_len,
            },
            CoreType::VarargLen {
                element: expected_element,
                len: expected_len,
            },
        ) => {
            core_static_datatype_exact_match_inner(
                actual_element,
                expected_element,
                false,
                bindings,
            ) && core_static_datatype_exact_match_inner(actual_len, expected_len, false, bindings)
        }
        (CoreType::Union(actual), CoreType::Union(expected)) => {
            if actual.len() != expected.len() {
                return false;
            }
            let mut matched = vec![false; expected.len()];
            actual.iter().all(|actual_ty| {
                if let Some(idx) = expected.iter().enumerate().position(|(idx, expected_ty)| {
                    !matched[idx]
                        && core_static_datatype_exact_match_inner(
                            actual_ty,
                            expected_ty,
                            allow_typevar_param,
                            bindings,
                        )
                }) {
                    matched[idx] = true;
                    true
                } else {
                    false
                }
            })
        }
        (
            CoreType::Struct {
                name: actual_name,
                params: actual_params,
            },
            CoreType::Struct {
                name: expected_name,
                params: expected_params,
            },
        ) => {
            actual_name == expected_name
                && actual_params.len() == expected_params.len()
                && actual_params.iter().zip(expected_params).all(
                    |(actual_param, expected_param)| {
                        core_static_datatype_exact_match_inner(
                            actual_param,
                            expected_param,
                            true,
                            bindings,
                        )
                    },
                )
        }
        (CoreType::Tuple(actual), CoreType::Tuple(expected)) => {
            actual.len() == expected.len()
                && actual.iter().zip(expected).all(|(actual_ty, expected_ty)| {
                    core_static_datatype_exact_match_inner(actual_ty, expected_ty, true, bindings)
                })
        }
        (CoreType::NamedTuple(actual), CoreType::NamedTuple(expected)) => {
            actual.len() == expected.len()
                && actual.iter().zip(expected).all(
                    |((actual_name, actual_ty), (expected_name, expected_ty))| {
                        actual_name == expected_name
                            && core_static_datatype_exact_match_inner(
                                actual_ty,
                                expected_ty,
                                true,
                                bindings,
                            )
                    },
                )
        }
        _ => false,
    }
}

/// Issue #8537: an inline-constructed parametric struct value (e.g.
/// `SVector(1.0, 2.0)`) is typed at compile time only by its *bare family*
/// name (`Struct("SVector")`) — its value type parameters (the `N` in
/// `SVector{N,T}`) are unknown until runtime. When such an argument reaches a
/// method table that also holds a sibling method pinning that parameter to a
/// concrete value (e.g. `g(::SVector{0,T})`), static dispatch cannot soundly
/// choose: the bare family loosely matches the concrete method and either wins
/// on specificity (wrong result) or matches nothing (spurious MethodError).
/// The runtime value carries the real parameters, so route to runtime dispatch.
///
/// Returns true when some argument slot is a bare parametric-struct family
/// (no `{...}` spelled, not a `typeof(...)` singleton) and at least one
/// arity-matching method pins a concrete value parameter for that family in
/// the same slot.
fn bare_parametric_struct_arg_has_value_param_sibling(
    table: &crate::compile::method_table::MethodTable,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    (0..args_len).any(|idx| {
        let Some(JuliaType::Struct(arg_name)) = arg_types.get(idx) else {
            return false;
        };
        // A concrete `SVector{2, Float64}` spelling dispatches soundly at
        // compile time; only the value-parameter-unknown bare family is unsound.
        if arg_name.contains('{') || is_callable_singleton_struct_name(arg_name) {
            return false;
        }
        let family = nominal_family_name(arg_name);
        table.methods.iter().any(|m| {
            m.accepts_arity(args_len)
                && m.expanded_core_param_types_for_arity(args_len)
                    .and_then(|cores| cores.get(idx).cloned())
                    .as_ref()
                    .is_some_and(|param| core_struct_param_pins_concrete_value_param(param, family))
        })
    })
}

impl CoreCompiler<'_> {
    fn base_method_crosses_nominal_struct_origin(
        &self,
        table: &MethodTable,
        method: &crate::compile::method_table::MethodSig,
        args: &[Expr],
        arg_types: &[JuliaType],
    ) -> bool {
        if !table.is_base_program_global_index(method.global_index) {
            return false;
        }
        let Some(param_types) =
            method
                .expanded_core_param_types_for_arity(args.len())
                .map(|types| {
                    types
                        .iter()
                        .map(crate::inference_core::core_type_to_julia_type)
                        .collect::<Vec<_>>()
                })
        else {
            return false;
        };
        let type_params: Vec<_> = method
            .core_signature_type_vars()
            .iter()
            .map(crate::inference_core::core_type_var_to_type_param)
            .collect();

        arg_types.iter().zip(param_types).any(|(actual, param)| {
            let mut pattern_matches = |pattern: &JuliaType, actual: &JuliaType| {
                crate::inference_core::dispatch_resolver::julia_signature_match_with_bindings(
                    std::slice::from_ref(pattern),
                    std::slice::from_ref(actual),
                    &type_params,
                )
                .is_some()
            };
            let mut origin_conflicts = |base_family: &str, actual_family: &str| {
                let base_type_id = self
                    .shared_ctx
                    .struct_table
                    .resolve_in_owner("Main", base_family)
                    .map(|(_, info)| info.type_id);
                let actual_type_id = self
                    .shared_ctx
                    .struct_table
                    .resolve(actual_family)
                    .map(|(_, info)| info.type_id);
                base_type_id.is_some_and(|type_id| Some(type_id) != actual_type_id)
            };
            crate::types::base_bare_nominal_origin_conflict_with(
                &param,
                actual,
                &mut pattern_matches,
                &mut origin_conflicts,
            )
        })
    }

    /// A bare `where`-clause type parameter variable whose runtime binding may
    /// be an integer value rather than a type (Issue #8539). `Val{...}`-style
    /// parameters are excluded: they load through `LoadAny` and already infer
    /// as `Any`, so the `has_any_arg` path covers them.
    fn expr_is_value_capable_type_param_var(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Var(name, _) => {
                self.current_type_param_index.contains_key(name.as_str())
                    && !self.val_type_params.contains(name.as_str())
                    && !self.val_bool_params.contains(name.as_str())
                    && !self.val_symbol_params.contains(name.as_str())
            }
            _ => false,
        }
    }

    fn static_datatype_core_arg(&self, expr: &Expr) -> Option<CoreType> {
        match expr {
            Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args,
                ..
            } => match args.as_slice() {
                [Expr::Literal(Literal::Str(type_name), _)] => {
                    Some(CoreType::from_julia_name(type_name))
                }
                _ => None,
            },
            Expr::Literal(Literal::DataType(type_name), _) => {
                Some(CoreType::from_julia_name(type_name))
            }
            _ => self
                .resolve_static_datatype_value(expr)
                .map(|ty| CoreType::from(&ty)),
        }
    }

    pub(in crate::compile) fn emit_runtime_dispatched_kwargs_call(
        &mut self,
        method_table_name: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
        args_already_compiled: bool,
    ) -> CResult<ValueType> {
        if !args_already_compiled {
            for arg in args {
                self.compile_expr(arg)?;
            }
        }

        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        // Keyword runtime dispatch must preserve the exact method-table
        // candidates. Some visible exported tables, such as the bare
        // `solve` table after `using OrdinaryDiffEq`, contain qualified method
        // bodies only (`SciMLBase.solve` / `OrdinaryDiffEq.solve`), so a plain
        // `PushFunction("solve")` cannot recover them at runtime (Issue #8396).
        self.emit_function_value(method_table_name);
        self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
            crate::bytecode::CallVarKwargsSplat {
                arg_count: args.len(),
                pos_splat_mask: vec![false; args.len()],
                kwarg_names,
                kwargs_splat_mask: kwargs_splat_mask.to_vec(),
            },
        )));
        Ok(ValueType::Any)
    }

    fn emit_catchable_no_method_error_call(
        &mut self,
        method_table_name: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
        has_kwargs_splat: bool,
        arg_types: &[JuliaType],
    ) -> CResult<ValueType> {
        if !kwargs.is_empty() || has_kwargs_splat {
            return self.emit_runtime_dispatched_kwargs_call(
                method_table_name,
                args,
                kwargs,
                kwargs_splat_mask,
                false,
            );
        }

        for arg in args {
            self.compile_expr(arg)?;
        }

        // Keep the argument values for the raise: the funnel exposes them as
        // the caught MethodError's `.args`, with `.f` from the callable name
        // (Issue #11374). The message keeps the compile-time signature text.
        let arg_sig: Vec<String> = arg_types.iter().map(|t| format!("::{}", t)).collect();
        self.emit(Instr::PushStr(format!(
            "no method matching {}({})",
            method_table_name,
            arg_sig.join(", ")
        )));
        self.emit(Instr::PushStr(method_table_name.to_string()));
        self.emit(Instr::CallBuiltin(
            BuiltinId::ThrowMethodErrorWithArgs,
            args.len() + 2,
        ));
        Ok(ValueType::Any)
    }

    fn function_definitely_accepts_supplied_keywords(
        &self,
        global_index: usize,
        explicit_keyword_names: &[&str],
        has_kwargs_splat: bool,
    ) -> bool {
        let Some(function) = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&global_index)
        else {
            return false;
        };
        if function.kwparams.iter().any(|kwparam| kwparam.is_varargs) {
            return true;
        }
        !has_kwargs_splat
            && explicit_keyword_names.iter().all(|key| {
                function
                    .kwparams
                    .iter()
                    .any(|kwparam| !kwparam.is_varargs && kwparam.name.as_str() == *key)
            })
    }

    fn function_may_accept_supplied_keywords(
        &self,
        global_index: usize,
        explicit_keyword_names: &[&str],
        has_kwargs_splat: bool,
    ) -> bool {
        let Some(function) = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&global_index)
        else {
            return false;
        };
        if function.kwparams.iter().any(|kwparam| kwparam.is_varargs) {
            return true;
        }
        if has_kwargs_splat {
            return !function.kwparams.is_empty();
        }
        explicit_keyword_names.iter().all(|key| {
            function
                .kwparams
                .iter()
                .any(|kwparam| !kwparam.is_varargs && kwparam.name.as_str() == *key)
        })
    }

    pub(in crate::compile) fn keyword_call_requires_runtime_dispatch(
        &self,
        table: &crate::compile::method_table::MethodTable,
        method: &crate::compile::method_table::MethodSig,
        arg_types: &[JuliaType],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
    ) -> bool {
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|is_splat| *is_splat);
        if kwargs.is_empty() && !has_kwargs_splat {
            return false;
        }

        let explicit_keyword_names: Vec<&str> = kwargs
            .iter()
            .enumerate()
            .filter(|(idx, _)| !kwargs_splat_mask.get(*idx).copied().unwrap_or(false))
            .map(|(_, (name, _))| name.as_str())
            .collect();

        if self.function_definitely_accepts_supplied_keywords(
            method.global_index,
            &explicit_keyword_names,
            has_kwargs_splat,
        ) {
            return false;
        }

        table.methods.iter().any(|candidate| {
            table.signature_matches_arg_types(candidate, arg_types)
                && self.function_may_accept_supplied_keywords(
                    candidate.global_index,
                    &explicit_keyword_names,
                    has_kwargs_splat,
                )
        })
    }

    /// Issue #7793: synthesized field-count default-constructor fallback for
    /// the multi-arg / static-miss `NoMethodFound` recovery arms.
    ///
    /// Defining any user **outer** constructor registers the struct name as a
    /// function with a method table that contains only the declared
    /// constructors — never the synthesized field-count default constructor.
    /// A top-level call whose arity
    /// differs from every declared constructor therefore misses dispatch with
    /// `NoMethodFound`, and the multi-arg / static-miss arms below build their
    /// candidate set from `accepts_arity(args.len())`, find none, and would
    /// error — even though upstream Julia still synthesizes (and keeps
    /// reachable) the field-count default constructor `Foo(::F1, ..., ::Fn)`.
    ///
    /// When `function` names a struct in `struct_table` and the call arity
    /// equals its field count, fall back to `compile_struct_constructor`
    /// (the field-count **built-in** constructor — NOT a re-dispatch to the
    /// user method, which would re-enter this same miss / recurse). This
    /// mirrors the single-arg arm (the `args.len() == 1` recovery already does
    /// the same via `struct_table.get(function)`), so all arities behave
    /// consistently. The caller passes the resolved method-table owner; a
    /// same-leaf sibling may never establish constructor identity (#11436).
    pub(super) fn try_struct_field_count_default_ctor_fallback(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        // Upstream Julia only synthesizes (and keeps reachable) the field-count
        // default constructor when the struct declares NO inner constructor. A
        // struct WITH an inner constructor that does not accept this call is a
        // genuine `MethodError`, so do not manufacture a default constructor for
        // it here (would silently build from raw fields and diverge from
        // upstream). Clone the `StructInfo` out of `shared_ctx` first so the
        // immutable borrow ends before the `&mut self` calls below.
        let struct_info = self
            .shared_ctx
            .struct_table
            .resolve_scoped(function, self.current_module_path.as_deref(), false)
            .map(|(_, info)| info)
            .filter(|info| !info.has_inner_constructor && info.fields.len() == args.len())
            .cloned();
        let Some(struct_info) = struct_info else {
            return Ok(None);
        };
        // Issue #7793 regression guard: only synthesize the field-count default
        // constructor when the argument types are actually convertible to the
        // (concrete) field types. When they are NOT (e.g. an outer ctor exists
        // but this call matches neither it nor the field types), fall through to
        // normal dispatch so it raises a catchable runtime `MethodError`,
        // matching upstream Julia — instead of `compile_struct_constructor`
        // emitting an uncatchable compile-time `Cannot convert ...` error.
        let has_runtime_unknown_arg = args
            .iter()
            .any(|arg| matches!(self.infer_julia_type(arg), JuliaType::Any));
        if has_runtime_unknown_arg
            || self.struct_field_count_ctor_args_convertible(&struct_info, args)
        {
            return self.compile_struct_constructor(struct_info, args).map(Some);
        }
        Ok(None)
    }

    /// Generic dispatch tail of `compile_call`: user/Base method-table
    /// multiple dispatch with runtime-dispatch candidate emission,
    /// dispatch-error fallbacks, return-type overrides, and the
    /// builtin/no-method-table fallback path.
    pub(super) fn compile_generic_dispatch_call(
        &mut self,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
        has_kwargs_splat: bool,
    ) -> CResult<ValueType> {
        // Issue #7575: when compiling inside a module that defines its OWN
        // function `function`, an unqualified call resolves to that module's
        // method table — never the shared bare-name pool that also holds a
        // parent module's same-named (possibly more-specific, typed) methods.
        let module_owned_table = self.module_owned_function_table_name(function);
        let base_qualified_function;
        let method_table_name = if let Some(owned) = module_owned_table.as_deref() {
            owned
        } else if self.method_tables.contains_key(function) {
            function
        } else if is_method_dispatch_first_base_function(function) {
            base_qualified_function = format!("Base.{}", function);
            if self.method_tables.contains_key(&base_qualified_function) {
                base_qualified_function.as_str()
            } else {
                function
            }
        } else {
            function
        };
        // Check if this is a user-defined function with potential multiple dispatch
        if let Some(table) = self.method_tables.get(method_table_name) {
            let source_visible_table = self.source_visible_method_table(method_table_name, table);
            let table = source_visible_table.as_ref().unwrap_or(table);
            // Julia gives bare constructors (`Foo(...)`) and explicit
            // parametric constructors (`Foo{T}(...)`) distinct implicit self
            // arguments. sjulia projects both away in MethodTable, so an
            // identical value signature would otherwise let the explicit
            // inner method steal the bare outer call (Issue #10959). For a
            // known bare parametric-struct call, remove only methods recorded
            // with the `Foo{T}` self; bare struct-body inner constructors and
            // ordinary outer constructors remain eligible. The table's own
            // serialized constructor-self-family carrier answers this for
            // both user and cached Base tables identically (Issue #10962,
            // #10974).
            let bare_constructor_table = (!function.contains('{')
                && self.resolve_parametric_struct_name(function).is_some()
                && table.has_explicit_parametric_inner_constructors())
            .then(|| {
                table.clone_with_methods_for_compile(
                    table
                        .methods
                        .iter()
                        .filter(|method| {
                            !table.is_explicit_parametric_inner_constructor(method.global_index)
                        })
                        .cloned()
                        .collect(),
                )
            });
            let table = bare_constructor_table.as_ref().unwrap_or(table);
            // Check if the function is accessible (top-level or imported via using)
            if !self.imported_functions.contains(function)
                && !self.lexical_function_tables.contains_key(function)
            {
                return err(format!(
                    "function '{}' is not imported. Use 'using ModuleName' or 'using ModuleName: {}' to import it, or use 'ModuleName.{}()' for qualified access.",
                    function, function, function
                ));
            }

            if let Some(candidates) =
                self.source_ordered_runtime_candidates(method_table_name, table, args.len())
            {
                if kwargs.is_empty() && kwargs_splat_mask.iter().all(|is_splat| !*is_splat) {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit_dynamic_call(method_table_name, usize::MAX, args.len(), candidates);
                    return Ok(ValueType::Any);
                }

                // A later source method is installed dormant in a REPL delta.
                // Direct keyword calls would target that future index and trip
                // the VM's world fence; function-value dispatch selects the
                // reached method first, then the replacement after its marker.
                return self.emit_runtime_dispatched_kwargs_call(
                    method_table_name,
                    args,
                    kwargs,
                    kwargs_splat_mask,
                    false,
                );
            }

            if opaque_runtime_eval_targets_function(
                &self.shared_ctx.opaque_runtime_eval_function_names,
                function,
            ) && method_table_has_non_base_methods_for_opaque_eval(table)
            {
                let splat_mask = vec![false; args.len()];
                return self.compile_runtime_global_function_call(
                    function,
                    args,
                    kwargs,
                    &splat_mask,
                    kwargs_splat_mask,
                );
            }

            // Infer argument types for dispatch.
            //
            // An argument inferred as `Bottom` (`Union{}`) means inference could
            // not prove the expression produces a value (e.g. the abstract
            // `//(::Integer, ::Integer)` fallback's interprocedural return type,
            // Issue #9362: `float(0x06 // 0x04)`). Since `Union{} <: T` holds for
            // every candidate signature, trusting it for static method selection
            // either picks an arbitrary method (flaky, hash-order dependent —
            // e.g. `float(::Complex)` receiving a `Rational` and raising an
            // InternalError) or raises a spurious compile-time ambiguity.
            // Normalize it to the runtime-unknown `Any` so the call defers to
            // runtime dispatch on the actual value type.
            let mut arg_types: Vec<JuliaType> = args
                .iter()
                .map(|a| match self.infer_julia_type(a) {
                    JuliaType::Bottom => JuliaType::Any,
                    ty => ty,
                })
                .collect();
            for (arg, ty) in args.iter().zip(arg_types.iter_mut()) {
                if matches!(ty, JuliaType::DataType) {
                    if let Some(mut static_ty) = self.resolve_static_datatype_value(arg) {
                        if let JuliaType::Struct(name) = &static_ty {
                            if name.starts_with("Union{") && name.ends_with('}') {
                                static_ty = crate::inference_core::core_type_to_julia_type(
                                    &CoreType::from_julia_name(name),
                                );
                            }
                        }
                        *ty = JuliaType::TypeOf(Box::new(static_ty));
                    }
                }
            }
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && is_reducible_nary_operator(function)
                && args.len() > 2
            {
                // Julia's n-ary `+`/`*` calls reduce to a left fold when no
                // more-specific method is known at compile time. Do this
                // before the broad Any-argument dynamic-call path so untyped
                // keyword/default frames do not emit a 3-arg call site with
                // only the string-concat vararg candidate (Issue #8369).
                return self.compile_nary_operator_reduction(function, args);
            }
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() == 2
                && (matches!(
                    base_function_to_builtin_op(function),
                    Some(BuiltinOp::Iterate)
                ) || matches!(
                    base_function_to_builtin_op(method_table_name),
                    Some(BuiltinOp::Iterate)
                ))
            {
                // The iterator protocol has VM-side fallback logic for
                // primitive collections and Pure Julia iterator structs. Do
                // not let the broad Any/Struct multi-arg dynamic-call path
                // below turn `iterate(collection, state)` into a generic
                // CallDynamic; that path cannot apply the iterator sentinel
                // handling needed by wrappers such as Iterators.Filter
                // (Issue #8370).
                return self.compile_builtin(&BuiltinOp::Iterate, args);
            }
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() > 1
                && arg_types.iter().any(|arg| {
                    matches!(arg, JuliaType::Any)
                        || is_runtime_unknown_struct_arg(arg)
                        || arg.is_abstract_container()
                })
            {
                let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
                let static_dispatch_is_sufficient =
                    table.dispatch(&arg_types).ok().is_some_and(|method| {
                        !should_runtime_dispatch(table, method, &arg_types, args.len(), has_any_arg)
                    });
                if !static_dispatch_is_sufficient {
                    let candidates = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect::<Vec<_>>();
                    if !candidates.is_empty() {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        self.emit_dynamic_call(
                            method_table_name,
                            usize::MAX,
                            args.len(),
                            candidates,
                        );
                        if let Some(hof_ty) = self.infer_hof_call_site_return_type(function, args) {
                            return Ok(hof_ty);
                        }
                        if is_truncated_result_call(function, args, kwargs) {
                            return Ok(self
                                .shared_ctx
                                .get_struct_type_id("Distributions.Truncated")
                                .or_else(|| self.shared_ctx.get_struct_type_id("Truncated"))
                                .map(ValueType::Struct)
                                .unwrap_or(ValueType::Any));
                        }
                        return Ok(ValueType::Any);
                    }
                }
            }
            if kwargs.is_empty() && !table.methods.iter().any(|m| m.accepts_arity(args.len())) {
                if let Some(vt) =
                    self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                {
                    return Ok(vt);
                }
            }

            // Issue #8537: a bare parametric-struct-family argument (value type
            // parameters unknown at compile time, e.g. the inline
            // `SVector(1.0, 2.0)` typed only as `Struct("SVector")`) must not
            // statically bind to a concrete value-parameter sibling method such
            // as `g(::SVector{0,T})` — the bare family loosely matches it and
            // either wins on specificity (wrong method) or matches nothing
            // (spurious MethodError). The runtime value carries the real
            // parameters, so defer to runtime dispatch, which selects the
            // parameter-generic method (or the truly matching concrete one).
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && !args.is_empty()
                && table.methods.len() > 1
                && bare_parametric_struct_arg_has_value_param_sibling(table, &arg_types, args.len())
            {
                let candidates: Vec<DynamicCallCandidate> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| DynamicCallCandidate::Method(m.global_index))
                    .collect();
                if !candidates.is_empty() {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit_dynamic_call(method_table_name, usize::MAX, args.len(), candidates);
                    if let Some(hof_ty) = self.infer_hof_call_site_return_type(function, args) {
                        return Ok(hof_ty);
                    }
                    return Ok(ValueType::Any);
                }
            }

            if matches!(function, "length" | "Base.length")
                && args.len() == 1
                && matches!(
                    arg_types.first(),
                    Some(JuliaType::Tuple | JuliaType::TupleOf(_))
                )
            {
                return self.compile_builtin(&BuiltinOp::Length, args);
            }

            // Issue #9133: `length` on a statically-known Array (including
            // `Vector{T}` / `Matrix{T}` parameter annotations) compiles to the
            // `Length` builtin, which returns `I64` directly — the upstream
            // analogue is `length(a::Array)` bottoming out in `arraylen`.
            // Without this, static dispatch matched the generic Pure Julia
            // `length` method and compiled `CallResolved` + `DynamicToI64`,
            // making the ANNOTATED parameter slower than an un-annotated one
            // (whose `Any`-typed arg already takes the builtin fallback path
            // below). Runtime arrays in that fallback execute the same builtin,
            // so this is output-identical.
            if matches!(function, "length" | "Base.length")
                && args.len() == 1
                && matches!(
                    arg_types.first(),
                    Some(JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_))
                )
            {
                return self.compile_builtin(&BuiltinOp::Length, args);
            }

            // Issue #9200 S3: `length` / `size` / `ndims` / `eltype` of a native
            // generator must use the Rust builtin, which special-cases a FILTERED
            // generator as `SizeUnknown` -> a `length`/`size` MethodError (Issue
            // #9320 / #9379). Since the S2/S3 desugar an inline generator now
            // infers as `JuliaType::Generator` (it was `Any` before), which would
            // otherwise statically dispatch the pure-Julia `length(g::Generator) =
            // length(g.iter)` method and return the COLLAPSED base iterator's
            // length — wrong for a filtered generator. Before the desugar these
            // inline generators were `Any`-typed and already took the builtin.
            if args.len() == 1
                && matches!(arg_types.first(), Some(JuliaType::Generator))
                && matches!(
                    function,
                    "length"
                        | "size"
                        | "ndims"
                        | "eltype"
                        | "Base.length"
                        | "Base.size"
                        | "Base.ndims"
                        | "Base.eltype"
                )
            {
                if let Some(builtin_op) = base_function_to_builtin_op(function) {
                    return self.compile_builtin(&builtin_op, args);
                }
            }

            if matches!(function, "step" | "Base.step")
                && kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() == 1
                && arg_types
                    .first()
                    .is_some_and(is_native_range_family_julia_type)
            {
                // Native `RangeValue` carries the user-visible step type. Route
                // exact native UnitRange/StepRange calls directly to the
                // intrinsic; abstract/struct-backed ranges continue through
                // method dispatch below.
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::RangeStep, 1));
                return Ok(ValueType::Any);
            }

            if matches!(function, "step" | "Base.step")
                && kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() == 1
                && arg_types.first().is_some_and(is_range_family_julia_type)
            {
                // Issue #9519: `ValueType::Range` locals are only typed as the
                // native range family at compile time, while the runtime
                // `RangeValue` carries whether the value is a UnitRange or a
                // StepRange{T,S}. Let runtime dispatch select the specific
                // StepRange method instead of statically binding `step(::Any)`.
                let candidates: Vec<DynamicCallCandidate> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(1))
                    .map(|m| DynamicCallCandidate::Method(m.global_index))
                    .collect();
                if !candidates.is_empty() {
                    self.compile_expr(&args[0])?;
                    self.emit_dynamic_call(method_table_name, usize::MAX, 1, candidates);
                    return Ok(ValueType::Any);
                }
            }

            // Check if any argument type is Any - this requires runtime dispatch
            let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
            // Issue #8539: a where-clause type parameter used as a call argument
            // is statically typed `DataType`, but at runtime it may hold an
            // integer value (e.g. `N` in `SVector{N,T}` — bound as `I64` by
            // `bind_type_params`, Issue #6625). When static dispatch misses,
            // treat such an argument as runtime-unknown so the call routes to
            // runtime typed dispatch instead of a guaranteed ThrowMethodError.
            //
            // Issue #9955: a call argument that is itself a *call* returning a
            // type object from multiple branches (e.g. `cond ? Float64 :
            // BigFloat`) infers as `Union{DataType, DataType}` (or a `Union`
            // of `Type{T}` members) — every possible runtime value is still a
            // concrete, dispatchable type object, just not statically pinned
            // to one. Widening every `DataType`/`Type{T}` union member to the
            // coarse `DataType` marker collapses distinct branches into
            // duplicate-looking entries, but static dispatch must not treat
            // this as a *proof* that no method matches: it is the same
            // "runtime-unknown, defer to dynamic dispatch" situation as a
            // where-clause value-type param, just arising from a
            // multi-branch method body instead of a param annotation.
            let has_value_type_param_arg = args.iter().zip(arg_types.iter()).any(|(arg, ty)| {
                (matches!(ty, JuliaType::DataType)
                    && self.expr_is_value_capable_type_param_var(arg))
                    || julia_type_is_datatype_union(ty)
            });
            let has_multiple_methods = table.methods.len() > 1;

            if has_any_arg
                && args.len() == 1
                && matches!(function, "length" | "size" | "ndims" | "eltype" | "collect")
            {
                if matches!(function, "length" | "size" | "ndims" | "eltype") {
                    let candidates = self.user_unary_runtime_candidates(table);
                    if !candidates.is_empty() {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        let builtin_id = match function {
                            "length" => BuiltinId::Length,
                            "size" => BuiltinId::Size,
                            "ndims" => BuiltinId::Ndims,
                            "eltype" => BuiltinId::Eltype,
                            _ => unreachable!("guarded by matches! above"),
                        };
                        self.emit(Instr::CallDynamicOrBuiltin(builtin_id, candidates));
                        return Ok(match function {
                            "length" | "ndims" => ValueType::I64,
                            "size" => ValueType::Tuple,
                            "eltype" => ValueType::DataType,
                            _ => unreachable!("guarded by matches! above"),
                        });
                    }
                }
                if let Some(builtin_op) = base_function_to_builtin_op(function) {
                    return self.compile_builtin(&builtin_op, args);
                }
                return self.compile_builtin_call(function, args);
            }

            if function == "show" && has_any_arg && has_multiple_methods {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();
                if !candidates.is_empty() {
                    // Issue #4347: `IOBuffer()` is still often Any-typed at
                    // statement boundaries, so `show(buf, ())` must dispatch
                    // on runtime IOBuffer/Tuple{} instead of statically picking
                    // an arbitrary specific show method such as CartesianIndex.
                    let fallback_index = candidates[0];
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        fallback_index,
                        candidates,
                    ));
                    return Ok(ValueType::Any);
                }
            }

            if matches!(function, "promote_type" | "promote_rule")
                && arg_types.iter().all(|t| matches!(t, JuliaType::DataType))
                && table.methods.iter().any(method_has_typeof_param)
            {
                for arg in args {
                    self.compile_expr(arg)?;
                }

                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();

                if !candidates.is_empty() {
                    let fallback_index = table
                        .methods
                        .iter()
                        .find(|m| method_has_typeof_typevar_param(m))
                        .map(|m| m.global_index)
                        .unwrap_or(candidates[0]);
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        fallback_index,
                        candidates,
                    ));
                    return Ok(ValueType::DataType);
                }
            }

            // floor(Int, x) / ceil(Int, x) / round(Int, x) / trunc(Int, x):
            // compile-time rounding with a constant target type.  The second
            // arg may be Any-typed (e.g. inside a loop over StaticArrayInline
            // elements) so static dispatch fails and the has_datatype_arg path
            // below would emit CallTypedDispatch — full resolution every call.
            // Short-circuit to compile_builtin_call so the FloorF64 + integer
            // conversion intrinsics are emitted instead (Issue #7964).
            // Only target types the builtin rounding path can actually
            // represent take the short-circuit; other type names (e.g.
            // `round(BigInt, x)`) must fall through to method dispatch so the
            // pure-Julia methods like `round(::Type{BigInt}, x::BigFloat)`
            // (base/gmp.jl) are reachable (Issue #9424).
            if matches!(function, "round" | "floor" | "ceil" | "trunc")
                && args.len() == 2
                && matches!(&args[0], Expr::Var(n, _)
                    if is_builtin_type_name(n)
                        && super::super::builtin_math::rounding_target_type(n).is_some())
            {
                return self.compile_builtin_call(function, args);
            }

            let has_type_object_arg = arg_types
                .iter()
                .any(|t| matches!(t, JuliaType::DataType | JuliaType::TypeOf(_)));
            let has_runtime_datatype_arg = args.iter().zip(arg_types.iter()).any(|(arg, ty)| {
                matches!(ty, JuliaType::DataType)
                    && self.resolve_static_datatype_value(arg).is_none()
            });
            let has_typeof_methods = table.methods.iter().any(method_has_typeof_param);
            let typeof_typevar_fallback = table
                .methods
                .iter()
                .find(|m| method_has_typeof_typevar_param(m))
                .map(|m| m.global_index);

            if function == "ndims" && args.len() == 1 && has_type_object_arg {
                return self.compile_builtin(&BuiltinOp::Ndims, args);
            }

            if matches!(
                function,
                "eltype" | "keytype" | "valtype" | "promote_type" | "promote_rule"
            ) && has_type_object_arg
                && has_typeof_methods
            {
                for arg in args {
                    self.compile_expr(arg)?;
                }

                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();

                if !candidates.is_empty() {
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        typeof_typevar_fallback.unwrap_or(candidates[0]),
                        candidates,
                    ));
                    return Ok(ValueType::DataType);
                }
            }

            if has_runtime_datatype_arg && has_typeof_methods {
                for arg in args {
                    self.compile_expr(arg)?;
                }

                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();

                if !candidates.is_empty() {
                    if let Some((builtin_id, return_type)) = base_function_to_builtin_op(function)
                        .or_else(|| base_function_to_builtin_op(method_table_name))
                        .and_then(type_object_dispatch_builtin_fallback)
                    {
                        self.emit(Instr::CallTypedDispatchOrBuiltin(
                            builtin_id,
                            method_table_name.to_string(),
                            args.len(),
                            candidates,
                        ));
                        return Ok(return_type);
                    }
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        typeof_typevar_fallback.unwrap_or(candidates[0]),
                        candidates,
                    ));
                    let return_type = match function {
                        "promote_type" | "promote_rule" | "typeof" | "eltype" | "keytype"
                        | "valtype" => ValueType::DataType,
                        _ => ValueType::Any,
                    };
                    return Ok(return_type);
                }
            }

            if kwargs.is_empty() && args.len() == 1 {
                if let Some(static_core) = self.static_datatype_core_arg(&args[0]) {
                    let expected = CoreType::TypeOf(Box::new(static_core));
                    if let Some(method) = table
                        .methods
                        .iter()
                        .find(|m| {
                            m.expanded_core_param_types_for_arity(1)
                                .is_some_and(|params| {
                                    params.first().is_some_and(|param| {
                                        core_static_datatype_exact_match(param, &expected)
                                    })
                                })
                        })
                        .cloned()
                    {
                        self.compile_expr(&args[0])?;
                        if self.inbounds_context {
                            self.emit(Instr::CallInbounds(method.global_index, 1));
                        } else {
                            self.emit(Instr::CallResolved(method.global_index, 1));
                        }
                        return Ok(method.return_type);
                    }
                }
            }

            // Find the best matching method
            // If dispatch fails for a known base function, fall back to the builtin implementation
            let method = match table.dispatch(&arg_types) {
                Ok(m) => m,
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if is_base_function(function)
                        && has_type_object_arg
                        && has_typeof_methods
                        && typeof_typevar_fallback.is_some() =>
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        self.emit(Instr::CallTypedDispatch(
                            method_table_name.to_string(),
                            args.len(),
                            typeof_typevar_fallback.unwrap_or(candidates[0]),
                            candidates,
                        ));
                        let return_type = match function {
                            "promote_type" | "promote_rule" | "typeof" | "eltype" | "keytype"
                            | "valtype" => ValueType::DataType,
                            _ => ValueType::Any,
                        };
                        return Ok(return_type);
                    }

                    return Err(CompileError::Dispatch(
                        crate::types::DispatchError::NoMethodFound {
                            name: method_table_name.to_string(),
                            arg_types,
                        },
                    ));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "size"
                        && args.len() == 2
                        && arg_types
                            .iter()
                            .any(|ty| matches!(ty, JuliaType::Struct(_))) =>
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if let Some(fallback_index) = candidates.first().copied() {
                        self.emit(Instr::CallTypedDispatch(
                            method_table_name.to_string(),
                            args.len(),
                            fallback_index,
                            candidates,
                        ));
                        return Ok(ValueType::Any);
                    }

                    if let Some(builtin_op) = base_function_to_builtin_op(function) {
                        return self.compile_builtin(&builtin_op, args);
                    }
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "IteratorEltype" && args.len() == 1 && !has_any_arg =>
                {
                    if let Some(
                        JuliaType::UnitRange
                        | JuliaType::StepRange
                        | JuliaType::Array
                        | JuliaType::VectorOf(_)
                        | JuliaType::MatrixOf(_)
                        | JuliaType::Tuple
                        | JuliaType::TupleOf(_)
                        | JuliaType::String,
                    ) = arg_types.first()
                    {
                        return self.compile_call("HasEltype", &[], &[], &[], &[]);
                    }
                    return Err(CompileError::Dispatch(
                        crate::types::DispatchError::NoMethodFound {
                            name: method_table_name.to_string(),
                            arg_types,
                        },
                    ));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if self
                        .shared_ctx
                        .struct_table
                        .get(function)
                        .is_some_and(|info| info.is_mutable && info.fields.len() == args.len()) =>
                {
                    let struct_info = self
                        .shared_ctx
                        .struct_table
                        .get(function)
                        .cloned()
                        .ok_or_else(|| {
                            CompileError::Msg(format!(
                                "Internal error: struct {} not found",
                                function
                            ))
                        })?;
                    return self.compile_struct_constructor(struct_info, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if matches!(function, "round" | "floor" | "ceil" | "trunc")
                        && args.len() == 2
                        && matches!(&args[0], Expr::Var(n, _) if is_builtin_type_name(n)) =>
                {
                    // `round(T, x)` / `floor(T, x)` / `ceil(T, x)` / `trunc(T, x)`
                    // type-conversion form. The builtin handler recognizes the
                    // `(TypeName, value)` shape; route there even when `x` is
                    // Any-typed (inside a function/loop/comprehension), where static
                    // dispatch otherwise fails with NoMethodFound (Issue #5657). A
                    // `round(x, mode)` / `round(x; digits)` call has a non-type first
                    // argument and is unaffected.
                    return self.compile_builtin_call(function, args);
                }
                Err(_)
                    if is_base_function(function)
                        && function != "convert"
                        && !has_any_arg
                        && !has_value_type_param_arg =>
                {
                    // Fallback to builtin for known base functions (e.g., floor(Float64))
                    // BUT only when argument types are known at compile time.
                    // When has_any_arg is true, we fall through to runtime dispatch instead,
                    // which allows user-defined methods (like Float64(::MyType)) to be called.
                    // A `has_value_type_param_arg` argument (a where-bound value parameter,
                    // e.g. `v` in `g(::VP{v}) where {v} = String(v)`) is DataType-typed
                    // statically but holds an ordinary runtime value, so its type is NOT
                    // known at compile time either. It must also fall through to runtime
                    // dispatch, otherwise a base function that has a method table but no
                    // builtin op (e.g. `String`, whose constructors are pure-Julia methods)
                    // reaches `compile_builtin_call`, which raises a spurious compile-time
                    // "Unknown function: String" instead of dispatching `String(::Symbol)`
                    // on the actual runtime value (Issue #10597).
                    // Try BuiltinOp first (handles iterate, typeof, etc. with proper types)
                    if let Some(builtin_op) = base_function_to_builtin_op(function) {
                        return self.compile_builtin(&builtin_op, args);
                    }
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if is_builtin_type_name(function)
                        && args.len() == 1
                        && !has_any_arg
                        && !has_value_type_param_arg =>
                {
                    // Fallback to builtin type constructor when user-defined method doesn't match
                    // AND the argument type is known at compile time (not Any).
                    // This handles cases like Float64(42) when user defined Float64(::MyType)
                    // but there's no Float64(::Int64) method.
                    // When has_any_arg is true, we fall through to runtime dispatch instead.
                    // A `has_value_type_param_arg` argument is runtime-unknown for the same
                    // reason as the base-function arm above, so it also defers to the
                    // value-parameter runtime dispatch arm below rather than the static
                    // builtin path (Issue #10597).
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if is_reducible_nary_operator(function) && args.len() > 2 =>
                {
                    // Julia semantics: when no specific n-arg method exists for operators like +/*,
                    // n-arg calls like +(a, b, c) reduce to +(+(a, b), c).
                    // This is Julia's generic: +(a, b, c, xs...) = afoldl(+, a+b, c, xs...)
                    // This works regardless of whether the methods are Base extensions or user-defined,
                    // as long as there's no specific n-arg method that matches the argument types.
                    return self.compile_nary_operator_reduction(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "convert" || function == "Base.convert" =>
                {
                    if args.len() != 2 {
                        return err("convert requires exactly 2 arguments: convert(T, x)");
                    }
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if args.len() == 1
                        && kwargs.is_empty()
                        && arg_types
                            .first()
                            .is_some_and(is_rank_unknown_array_julia_type)
                        && table.methods.iter().any(|m| {
                            m.param_count() == 1
                                && m.param_matches_at_call_position(0, core_is_array_family_type)
                        }) =>
                {
                    // Issue #7266: a single array-family argument whose element
                    // type is unknown at compile time (most notably a
                    // comprehension `[expr for ...]`, imaged as the bare
                    // `JuliaType::Struct("Vector")`) statically matches NO method
                    // — the parametric `::AbstractVector{<:Real}` / `::Vector{T}`
                    // arms need a concrete element type. The correct concrete
                    // `Vector{Float64}` value DOES select the right method at
                    // runtime, so route to runtime dispatch with the no-match
                    // sentinel (mirroring the single-arg `has_any_arg` arm) rather
                    // than throwing a static MethodError or loose-matching an
                    // unrelated abstract-scalar method (the pre-fix bug routed
                    // `Vector` to `::Integer`).
                    //
                    // Issue #10206: broadened from `core_is_abstract_array_family_type`
                    // to `core_is_array_family_type` (abstract OR concrete
                    // `Array`/`Vector`/`Matrix`, e.g. `Array{Any,2}`) — a
                    // multi-iterator comprehension's bare `Struct("Matrix")`
                    // could never statically match a *concrete*
                    // `::Array{Any,2}`-typed candidate either, so it hit this
                    // same "no static match" situation, but the guard only
                    // recognized abstract candidates and fell through to a
                    // compile-time `ThrowMethodError` instead of deferring
                    // Issue #10315 subsequently made the rank-1 collector keep
                    // the same rank-known/element-unresolved provenance; its
                    // runtime value now reaches this policy instead of being
                    // mislabeled as a statically proven `Vector{Any}`.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1)
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();
                    self.emit_dynamic_call(method_table_name, usize::MAX, 1, candidates);
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if (has_any_arg || has_value_type_param_arg) && args.len() == 1 =>
                {
                    if !kwargs.is_empty() {
                        return self.emit_runtime_dispatched_kwargs_call(
                            method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                            false,
                        );
                    }

                    // For functions that have builtin fallbacks (floor, ceil, etc.), use the builtin path
                    // which has CallDynamicOrBuiltin support for runtime dispatch with builtin fallback.
                    // This includes:
                    // - Rounding functions (floor, ceil, round, trunc) with Rational methods
                    // - Math functions (sqrt, abs, sign) with struct methods
                    // - I/O functions (take!) that have both builtin (IOBuffer) and Julia (Channel) methods
                    // All should fall back to builtin for Float64/Int64 or IO types.
                    // Note: sin, cos, tan, exp, log removed — now Pure Julia (base/math.jl)
                    match function {
                        // Note: sin, cos, tan, exp, log removed — now Pure Julia (base/math.jl)
                        "floor" | "ceil" | "round" | "trunc" | "sqrt" | "abs" | "sign"
                        | "take!" | "takestring!" | "Int" | "UInt" | "Char" => {
                            return self.compile_builtin_call(function, args);
                        }
                        // Shape/eltype protocol (Issues #3736/#4066): when the argument type is
                        // unknown at compile time and no Pure Julia method matches
                        // the inferred (Any) type, route to BuiltinId::{Length,Size,Ndims}.
                        // The runtime handlers there dispatch to the method table
                        // for Struct/StructRef values and otherwise fall back to
                        // primitive container behavior (Array, Tuple, String,
                        // Range, Dict, Set, Generator).
                        "length" | "size" | "ndims" | "eltype" | "objectid" => {
                            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                                return self.compile_builtin(&builtin_op, args);
                            }
                            return self.compile_builtin_call(function, args);
                        }
                        // Iterator protocol (Issue #3735): same fallback story as
                        // length/size — route to BuiltinOp::Iterate / BuiltinOp::Collect
                        // so the runtime handler can do its own struct-vs-primitive
                        // dispatch (BuiltinId::Iterate / BuiltinId::RangeCollect).
                        "iterate" | "collect" => {
                            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                                return self.compile_builtin(&builtin_op, args);
                            }
                            return self.compile_builtin_call(function, args);
                        }
                        _ => {}
                    }

                    // Issue #10871: a DispatchFirst Base function whose Rust
                    // fallback dispatches on a `Type{T}` argument (isbitstype,
                    // isa, subtypes) must not lose that fallback just because
                    // *this* call site's argument infers as plain `Any`
                    // rather than the statically-known `DataType`/`TypeOf`
                    // shape handled above (e.g. an unannotated parameter
                    // `g(T) = Base.isbitstype(T)` called as `g(Int64)`).
                    // Reuse the same BuiltinOp -> (BuiltinId, ValueType)
                    // conversion the has_runtime_datatype_arg arm above uses,
                    // so a dispatch miss reaches the Rust builtin instead of
                    // a spurious MethodError (bytecode previously confirmed:
                    // `CallDynamic` with only the user's `Method(_)` as
                    // candidate, no builtin fallback). Functions without a
                    // registered builtin_op (e.g. `isbits`/`ismutable`, which
                    // are pure-Julia catch-alls per Issue #6738) are
                    // unaffected and keep using the generic CallDynamic path
                    // below.
                    if let Some((builtin_id, return_type)) = base_function_to_builtin_op(function)
                        .or_else(|| base_function_to_builtin_op(method_table_name))
                        .and_then(type_object_dispatch_builtin_fallback)
                    {
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(1))
                            .map(|m| m.global_index)
                            .collect();
                        if !candidates.is_empty() {
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.emit(Instr::CallTypedDispatchOrBuiltin(
                                builtin_id,
                                method_table_name.to_string(),
                                1,
                                candidates,
                            ));
                            return Ok(return_type);
                        }
                    }

                    // When argument type is Any (compile-time unknown) and there are methods,
                    // use runtime dispatch. This handles cases like inv(x) where x::Rational{T}.
                    // At compile time we don't know the concrete type, so we dispatch at runtime.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    // Build candidates for runtime dispatch from all single-arg
                    // methods. The expected type name is derived from each
                    // candidate's FunctionInfo at runtime (Issue #6496).
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1)
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();

                    if !candidates.is_empty() {
                        // No compile-time method accepted `Any`, so there is no
                        // valid Julia fallback when runtime candidate scoring
                        // also misses. Use the no-match sentinel and let the VM
                        // raise MethodError instead of calling an arbitrary
                        // specific candidate (Issue #4020).
                        self.emit_dynamic_call(method_table_name, usize::MAX, 1, candidates);
                        // Return Any since we don't know the concrete return type
                        // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                        return Ok(ValueType::Any);
                    }

                    // No method candidates - check if this is a struct constructor
                    // If so, fall back to the default struct constructor
                    if let Some(struct_info) = self.shared_ctx.struct_table.get(function) {
                        if struct_info.fields.len() == args.len() {
                            return self.compile_struct_constructor(struct_info.clone(), args);
                        }
                    }

                    // No candidates found - fall through to error
                    return err(format!("No method matching {}({:?})", function, arg_types));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if (has_any_arg || has_value_type_param_arg) && args.len() >= 2 =>
                {
                    if !kwargs.is_empty() {
                        return self.emit_runtime_dispatched_kwargs_call(
                            method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                            false,
                        );
                    }

                    // Shape protocol (Issue #4314): `size(x, dim)` with an
                    // Any-typed first argument must still let runtime method
                    // dispatch select Pure Julia struct methods such as
                    // `Base.size(::Diagonal, ::Int)`. Primitive containers are
                    // covered by the runtime candidates from Base's `size`
                    // methods below, so do not short-circuit to BuiltinId::Size
                    // here.
                    // Iterator protocol (Issue #3735): `iterate(coll, state)` with
                    // unknown argument types should route to BuiltinOp::Iterate so
                    // the runtime handler can dispatch via its own struct/primitive
                    // logic.
                    if matches!(function, "iterate") && args.len() == 2 {
                        if let Some(builtin_op) = base_function_to_builtin_op(function) {
                            return self.compile_builtin(&builtin_op, args);
                        }
                    }
                    // When argument types include Any (compile-time unknown) for multi-arg functions,
                    // use runtime dispatch. This handles cases like gcd(a, b) where a, b have type T.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    // Build candidates for runtime dispatch from all matching-arity methods
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        if candidates.len() == 1 {
                            self.emit_call_or_specialize(
                                method_table_name,
                                candidates[0],
                                args.len(),
                            );
                        } else {
                            // Use the first method as fallback
                            let fallback_index = candidates[0];
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                fallback_index,
                                candidates,
                            ));
                        }
                        // The runtime dispatch above selects the concrete method,
                        // but for higher-order functions whose callable argument
                        // is an inline lambda (now `Any`-typed since its bare
                        // nested name left the short-name table — Issue #8105) the
                        // result type is still statically inferable from the
                        // call-site expressions. Recover it so `y = reduce(op, xs)`
                        // keeps its precise (e.g. Float64) binding type instead of
                        // widening to `Any`; non-HOF callees stay `Any` as before.
                        if let Some(hof_ty) = self.infer_hof_call_site_return_type(function, args) {
                            return Ok(hof_ty);
                        }
                        // Return Any since we don't know the concrete return type
                        // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                        return Ok(ValueType::Any);
                    }

                    // Issue #7793: no arity-matching declared constructor, but
                    // the name is a struct whose field count equals the call
                    // arity — fall back to the synthesized field-count default
                    // constructor (mirrors the single-arg arm above).
                    if let Some(vt) =
                        self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                    {
                        return Ok(vt);
                    }

                    // No candidates found - fall through to error
                    return err(format!("No method matching {}({:?})", function, arg_types));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if kwargs.is_empty()
                        && table.methods.iter().any(|m| m.param_count() == args.len()) =>
                {
                    if has_any_arg || has_value_type_param_arg {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(args.len()))
                            .map(|m| m.global_index)
                            .collect();
                        if let Some(&fallback_index) = candidates.first() {
                            if candidates.len() == 1 {
                                self.emit_call_or_specialize(
                                    method_table_name,
                                    fallback_index,
                                    args.len(),
                                );
                            } else {
                                self.emit(Instr::CallTypedDispatch(
                                    method_table_name.to_string(),
                                    args.len(),
                                    fallback_index,
                                    candidates,
                                ));
                            }
                            return Ok(ValueType::Any);
                        }
                    }
                    // Issue #7793: a same-arity declared constructor exists but
                    // its types did not match. The struct still has its
                    // synthesized field-count default constructor, so when the
                    // call arity equals the field count fall back to it instead
                    // of throwing (mirrors the single-arg arm). Routes to the
                    // field-count built-in constructor, never a re-dispatch.
                    if let Some(vt) =
                        self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                    {
                        return Ok(vt);
                    }
                    // Exact spellings have explicit precedence. Do not scan the
                    // hash-backed registry and let registration order choose a
                    // constructor identity (Issue #11436).
                    let struct_match = self
                        .shared_ctx
                        .struct_table
                        .get(method_table_name)
                        .filter(|info| {
                            !info.has_inner_constructor && info.fields.len() == args.len()
                        })
                        .cloned();
                    if let Some(struct_info) = struct_match {
                        let has_runtime_unknown_arg = args
                            .iter()
                            .any(|arg| matches!(self.infer_julia_type(arg), JuliaType::Any));
                        if has_runtime_unknown_arg
                            || self.struct_field_count_ctor_args_convertible(&struct_info, args)
                        {
                            return self.compile_struct_constructor(struct_info, args);
                        }
                    }

                    // Issue #6007: a fully static method miss is still a runtime
                    // MethodError in Julia. Evaluate arguments for side effects,
                    // then raise a catchable runtime MethodError instead of
                    // aborting compilation with Dispatch(NoMethodFound).
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    // Keep the argument values for the raise (Issue #11374):
                    // `.args`/`.f` come from the funnel payload.
                    let arg_sig: Vec<String> =
                        arg_types.iter().map(|t| format!("::{}", t)).collect();
                    self.emit(Instr::PushStr(format!(
                        "no method matching {}({})",
                        method_table_name,
                        arg_sig.join(", ")
                    )));
                    self.emit(Instr::PushStr(method_table_name.to_string()));
                    self.emit(Instr::CallBuiltin(
                        BuiltinId::ThrowMethodErrorWithArgs,
                        args.len() + 2,
                    ));
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if self.in_catchable_runtime_error_region() =>
                {
                    return self.emit_catchable_no_method_error_call(
                        method_table_name,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                        has_kwargs_splat,
                        &arg_types,
                    );
                }
                Err(crate::types::DispatchError::AmbiguousMethod { .. }) => {
                    // Check if this is a Type{T} dispatch scenario:
                    // - At least one argument only inferred as DataType (a type value)
                    // - Methods have TypeOf patterns in their parameters
                    //
                    // Julia only needs runtime type-object dispatch for the Type{...}
                    // positions, not for every argument. This lets mixed calls with a
                    // runtime DataType value and concrete non-type arguments reach the
                    // runtime Type{...} fallback instead of being rejected here.
                    let has_datatype_arg =
                        arg_types.iter().any(|t| matches!(t, JuliaType::DataType));
                    let has_typeof_methods = table.methods.iter().any(method_has_typeof_param);

                    if has_datatype_arg && has_typeof_methods {
                        // Compile arguments - they are type values (DataType)
                        for arg in args {
                            self.compile_expr(arg)?;
                        }

                        // Build candidates for runtime typed dispatch
                        // (candidate function indices; expected type names are
                        // derived at runtime, Issue #6496)
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(args.len()))
                            .map(|m| m.global_index)
                            .collect();

                        // Find the fallback method (one with TypeVar patterns - generic version)
                        let fallback_index = table
                            .methods
                            .iter()
                            .find(|m| method_has_typeof_typevar_param(m))
                            .map(|m| m.global_index)
                            .unwrap_or(candidates.first().copied().unwrap_or(0));

                        if candidates.len() == 1 && candidates[0] == fallback_index {
                            self.emit_call_or_specialize(
                                method_table_name,
                                candidates[0],
                                args.len(),
                            );
                        } else {
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                fallback_index,
                                candidates,
                            ));
                        }

                        // Return type is typically Any, but override for type-returning functions
                        let return_type = match function {
                            "promote_type" | "promote_rule" | "typeof" | "eltype" | "keytype"
                            | "valtype" => ValueType::DataType,
                            _ => ValueType::Any,
                        };
                        return Ok(return_type);
                    }

                    // Check if any argument is Any OR a concrete Struct - use
                    // runtime dispatch in that case.
                    //
                    // Issue #4827: a `Struct(T)` argument can statically tie
                    // several method-table arms (e.g. `show(::IO, ::Struct(X))`
                    // for many built-in `X`, plus the generic `show(::IO, ::Any)`)
                    // when the dispatcher's tie-breakers can't pick a unique best
                    // — even though only the runtime concrete struct type selects
                    // the correct arm. Before #4827 this surfaced rarely because a
                    // local `IOBuffer()` inferred as `Any` (so `has_any_arg` was
                    // already true and we ran the runtime-dispatch path). Now that
                    // an `IOBuffer()` slot is statically `IO`, neither arg is `Any`,
                    // so an ambiguous `show(buf::IO, x::Struct)` without a specific
                    // user method would error at compile time. Defer to runtime
                    // dispatch (matching upstream Julia semantics, and what
                    // `CallTypedDispatch` does): it scores the candidates against
                    // the runtime concrete type and raises a proper MethodError if
                    // none applies.
                    let has_struct_arg =
                        arg_types.iter().any(|t| matches!(t, JuliaType::Struct(_)));
                    // A where-bound value-parameter argument (or a DataType-union
                    // branch value) is runtime-unknown for an *ambiguous* miss too,
                    // not only a `NoMethodFound` miss: the static ambiguity is an
                    // artifact of the coarse `DataType` arg type, while the concrete
                    // runtime value selects a single best method. Route it to runtime
                    // typed dispatch alongside `Any`/`Struct` args rather than
                    // throwing an unconditional ambiguity MethodError (keeps the
                    // value-parameter dispatch invariant symmetric across both
                    // `DispatchError` variants, Issue #10597).
                    if has_any_arg || has_struct_arg || has_value_type_param_arg {
                        if !kwargs.is_empty() {
                            return self.emit_runtime_dispatched_kwargs_call(
                                method_table_name,
                                args,
                                kwargs,
                                kwargs_splat_mask,
                                false,
                            );
                        }

                        // Compile arguments
                        for arg in args {
                            self.compile_expr(arg)?;
                        }

                        // Build candidates for runtime dispatch
                        // (candidate function indices; expected type names are
                        // derived at runtime, Issue #6496)
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(args.len()))
                            .map(|m| m.global_index)
                            .collect();

                        if !candidates.is_empty() {
                            if candidates.len() == 1 {
                                self.emit_call_or_specialize(
                                    method_table_name,
                                    candidates[0],
                                    args.len(),
                                );
                            } else {
                                // Use the first candidate as fallback
                                let fallback_index = candidates[0];
                                self.emit(Instr::CallTypedDispatch(
                                    method_table_name.to_string(),
                                    args.len(),
                                    fallback_index,
                                    candidates,
                                ));
                            }
                            // Return Any since we don't know the concrete return type
                            // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                            return Ok(ValueType::Any);
                        }
                    }

                    // Genuinely ambiguous call with no most-specific resolution
                    // and no runtime-dispatch fallback. Upstream Julia raises a
                    // *catchable* runtime `MethodError` (ambiguity) here rather
                    // than aborting; mirror that instead of returning a hard
                    // `CompileError::Dispatch(AmbiguousMethod{..})` that exits the
                    // process (Issue #5071).
                    // Candidate rows are sourced core-projection-first (the
                    // canonical-inverse reconstruction renders identically for
                    // every round-tripping spelling; Issue #6495, stage
                    // 6b-iii). `params.len()` is an arity read.
                    let candidate_sigs: Vec<Vec<JuliaType>> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == args.len())
                        .map(|m| m.projected_param_julia_types())
                        .collect();

                    // Build the runtime message. `VmError::MethodError` already
                    // prepends "MethodError: ", so omit that prefix here. The
                    // shape resembles upstream's
                    // "f(::Int64, ::Int64) is ambiguous. Candidates: ...".
                    let arg_sig: Vec<String> =
                        arg_types.iter().map(|t| format!("::{}", t)).collect();
                    let mut message = format!(
                        "{}({}) is ambiguous. Candidates:",
                        method_table_name,
                        arg_sig.join(", ")
                    );
                    for sig in &candidate_sigs {
                        let sig_str: Vec<String> = sig.iter().map(|t| format!("::{}", t)).collect();
                        message.push_str(&format!(
                            "\n  {}({})",
                            method_table_name,
                            sig_str.join(", ")
                        ));
                    }

                    // Evaluate the arguments for side-effect fidelity (upstream
                    // evaluates call arguments before dispatch fails), then drop
                    // them and throw the runtime MethodError.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    for _ in args {
                        self.emit(Instr::Pop);
                    }
                    self.emit(Instr::ThrowMethodError(message));

                    // The throw is unconditional; the value type after it is
                    // unreachable, so report Any.
                    return Ok(ValueType::Any);
                }
                Err(e) => return Err(CompileError::Dispatch(e)),
            };

            if self.base_method_crosses_nominal_struct_origin(table, method, args, &arg_types) {
                if !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat) {
                    return self.emit_runtime_dispatched_kwargs_call(
                        method_table_name,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                        false,
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                let candidates = table
                    .methods
                    .iter()
                    .filter(|candidate| candidate.accepts_arity(args.len()))
                    .map(|candidate| DynamicCallCandidate::Method(candidate.global_index))
                    .collect();
                self.emit_dynamic_call(method_table_name, usize::MAX, args.len(), candidates);
                return Ok(ValueType::Any);
            }

            // Compile positional arguments with expected types.
            //
            // IMPORTANT: When runtime dispatch will decide the final method, do not
            // coerce arguments based on the statically selected fallback method. For
            // `ones(T, 2)` with T as an unknown runtime type object, static dispatch
            // may pick a dims fallback, but Julia dispatch must still see the first
            // argument as a DataType value and select `ones(::Type{T}, ...)`.
            // Issue #8158: the qualified `Module.f(x)` path
            // (`compile_module_call_via_method_table`) mirrors this exact policy
            // through the shared `should_runtime_dispatch` helper so a qualified
            // call defers to runtime dispatch in the same cases as this
            // unqualified call. The single-vs-multi split is kept here because
            // the two emit different instructions below (single → `CallDynamic`,
            // multi → `CallTypedDispatch`).
            let use_single_arg_runtime_dispatch =
                single_arg_runtime_dispatch_required(table, method, &arg_types, has_any_arg);
            let use_multi_arg_runtime_dispatch = has_multiple_methods && args.len() > 1 && {
                // The per-slot `Any` probes read the canonical
                // `core_signature` projection first (Issue #6495, stage
                // 6b-iii); `params.len()` comparisons are arity reads.
                // Out-of-range slots are non-matches, preserving the
                // legacy `zip` truncation / `params.get` `Option` gates.
                let (case1, case2, case3) = if has_any_arg {
                    let case1 = arg_types.iter().enumerate().any(|(idx, arg_ty)| {
                        matches!(arg_ty, JuliaType::Any)
                            && method_param_is_not_any_at_call_position(method, idx)
                    });
                    let case2 = !case1 && {
                        let matched_all_any = method.all_params_match(core_is_any_param);
                        matched_all_any
                            && table.methods.iter().any(|m| {
                                m.accepts_arity(args.len())
                                    && m.global_index != method.global_index
                                    && m.any_param_matches(|core| !core_is_any_param(core))
                            })
                    };
                    let case3 = !case1
                        && !case2
                        && table.methods.iter().any(|m| {
                            m.accepts_arity(args.len())
                                && m.global_index != method.global_index
                                && (0..args.len()).any(|idx| {
                                    matches!(arg_types.get(idx), Some(JuliaType::Any))
                                        && method_param_is_any_at_call_position(method, idx)
                                        && method_param_is_not_any_at_call_position(m, idx)
                                })
                        });
                    (case1, case2, case3)
                } else {
                    (false, false, false)
                };
                // Abstract-array-family probe sourced from the canonical
                // `core_signature` projection (Issue #6495, stages
                // 7a/7c-ii).
                let case4 = table.methods.iter().any(|m| {
                    m.global_index != method.global_index
                        && m.accepts_arity(args.len())
                        && (0..args.len()).any(|idx| {
                            arg_types
                                .get(idx)
                                .is_some_and(is_rank_unknown_array_julia_type)
                                && m.param_matches_at_call_position(
                                    idx,
                                    core_is_abstract_array_family_type,
                                )
                        })
                });
                let case5 =
                    runtime_unknown_struct_arg_requires_dispatch(method, &arg_types, args.len())
                        || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
                            method,
                            &arg_types,
                            args.len(),
                        );
                case1 || case2 || case3 || case4 || case5
            };
            let use_runtime_dispatch =
                use_single_arg_runtime_dispatch || use_multi_arg_runtime_dispatch;
            let force_resolved_static_type_object_call = !use_runtime_dispatch
                && args
                    .iter()
                    .any(|arg| self.resolve_static_datatype_value(arg).is_some());
            // Cross-check: the extracted shared policy must agree with this inline
            // computation (the qualified path relies on it — Issue #8158).
            debug_assert_eq!(
                use_runtime_dispatch,
                should_runtime_dispatch(table, method, &arg_types, args.len(), has_any_arg),
                "should_runtime_dispatch drifted from the inline bare-call policy"
            );

            let has_callsite_kwargs =
                !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat);
            if has_callsite_kwargs
                && (use_runtime_dispatch
                    || self.keyword_call_requires_runtime_dispatch(
                        table,
                        method,
                        &arg_types,
                        kwargs,
                        kwargs_splat_mask,
                    ))
            {
                // Issue #8566: keyworded calls dispatch through the keyword-
                // accepting method set, like upstream's Core.kwcall table. A
                // positional-only method may still be best for `f(x)`, while
                // `f(x; kw=...)` selects a forwarding fallback such as
                // `f(xs...; kws...)` when that is the applicable kw method.
                return self.emit_runtime_dispatched_kwargs_call(
                    method_table_name,
                    args,
                    kwargs,
                    kwargs_splat_mask,
                    false,
                );
            }

            // Handle varargs functions differently - compile all args
            if let Some(vararg_idx) = method.vararg_param_index {
                // Compile fixed params with their expected types
                for (idx, arg) in args.iter().enumerate() {
                    if idx < vararg_idx {
                        // Fixed parameter - use expected type ONLY if not using runtime dispatch
                        if use_runtime_dispatch {
                            // Runtime dispatch: don't coerce, preserve original type
                            self.compile_expr(arg)?;
                        } else if idx < method.param_count() {
                            // Coercion gate sourced core-projection-first via
                            // the canonical inverse (Issue #6495, stage
                            // 6b-iii); `params.len()` is an arity read.
                            let param_ty = method.projected_param_julia_type(idx);
                            if *param_ty == JuliaType::Any
                                || param_ty.is_narrow_integer()
                                || param_ty.is_abstract_integer()
                                || param_ty.is_abstract_container()
                                || param_ty.is_abstract_with_struct_subtypes()
                                || is_dict_annotation(&param_ty)
                            {
                                self.compile_expr(arg)?;
                            } else {
                                let vt = julia_type_to_value_type(&param_ty);
                                self.compile_expr_as(arg, vt)?;
                            }
                        } else {
                            self.compile_expr(arg)?;
                        }
                    } else {
                        // Varargs - compile as-is
                        self.compile_expr(arg)?;
                    }
                }
            } else {
                // Non-varargs: compile args paired with params (the
                // `take(params.len())` mirrors the historical `zip`
                // truncation; the coercion gate reads the canonical
                // `core_signature` projection first — Issue #6495, stage
                // 6b-iii).
                for (idx, arg) in args.iter().enumerate().take(method.param_count()) {
                    // When using runtime dispatch, don't coerce - preserve original type
                    if use_runtime_dispatch {
                        self.compile_expr(arg)?;
                        continue;
                    }
                    let param_ty = method.projected_param_julia_type(idx);
                    if *param_ty == JuliaType::Any {
                        // For `Any` typed parameters, don't coerce - just compile the argument as-is
                        self.compile_expr(arg)?;
                    } else if param_ty.is_narrow_integer() || param_ty.is_abstract_integer() {
                        // For narrow integer types (Int8, Int16, Int32, UInt*, Bool, Int128)
                        // and abstract integer supertypes (Integer, Signed, Unsigned, Real, Number),
                        // don't coerce to I64 - preserve the specific type so the function
                        // body receives the correct Value variant (e.g., Value::I32 not Value::I64).
                        self.compile_expr(arg)?;
                    } else if param_ty.is_abstract_container()
                        || param_ty.is_abstract_with_struct_subtypes()
                        || is_native_range_family_julia_type(&param_ty)
                        || is_dict_annotation(&param_ty)
                    {
                        // Abstract container params (`AbstractArray` / `AbstractRange`),
                        // abstract families with declared struct subtypes (`IO` /
                        // `Function` / `AbstractString` / `AbstractChar`, Issue #8560), and
                        // public Dict params during the struct-backed migration:
                        // a concrete subtype value may be a struct (`OneTo`, `SubArray`,
                        // a functor / user IO type), or a struct-backed `Dict{K,V}`, not
                        // the native `ValueType` `julia_type_to_value_type` maps to, so
                        // compile as-is rather than coercing. Issue #5842 / #6619.
                        self.compile_expr(arg)?;
                    } else {
                        let vt = julia_type_to_value_type(&param_ty);
                        self.compile_expr_as(arg, vt)?;
                    }
                }
            }

            if kwargs.is_empty() {
                // Check if runtime dispatch is needed
                if use_single_arg_runtime_dispatch {
                    // Build candidates for runtime dispatch (single-arg). The
                    // expected type name is derived from each candidate's
                    // FunctionInfo at runtime (Issue #6496).
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1 && method_param_is_not_any_at(m, 0))
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();

                    if !candidates.is_empty() {
                        let candidates_are_base_only = candidates.iter().all(|c| {
                            matches!(c, DynamicCallCandidate::Method(idx)
                                if table.is_base_program_global_index(*idx))
                        });
                        let fallback_index = if candidates_are_base_only
                            || method.all_params_match(core_is_any_param)
                        {
                            method.global_index
                        } else {
                            usize::MAX
                        };
                        // Use CallDynamic for runtime dispatch
                        self.emit_dynamic_call(
                            method_table_name,
                            fallback_index,
                            args.len(),
                            candidates,
                        );
                    } else {
                        // No specific candidates, use static dispatch (with Lazy AoT check)
                        self.emit_call_or_specialize(
                            method_table_name,
                            method.global_index,
                            args.len(),
                        );
                    }
                } else if use_multi_arg_runtime_dispatch {
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        if has_any_arg
                            || should_use_dynamic_call_for_runtime_dispatch(
                                method,
                                &arg_types,
                                args.len(),
                            )
                        {
                            self.emit_dynamic_call(
                                method_table_name,
                                method.global_index,
                                args.len(),
                                candidates
                                    .into_iter()
                                    .map(DynamicCallCandidate::Method)
                                    .collect(),
                            );
                        } else {
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                method.global_index,
                                candidates,
                            ));
                        }
                    } else {
                        // No specific candidates, use static dispatch
                        self.emit_call_or_specialize(
                            method_table_name,
                            method.global_index,
                            args.len(),
                        );
                    }
                } else {
                    // No kwargs - use Call instruction (with Lazy AoT check)
                    if force_resolved_static_type_object_call {
                        if self.inbounds_context {
                            self.emit(Instr::CallInbounds(method.global_index, args.len()));
                        } else {
                            self.emit(Instr::CallResolved(method.global_index, args.len()));
                        }
                    } else {
                        self.emit_call_or_specialize(
                            method_table_name,
                            method.global_index,
                            args.len(),
                        );
                    }
                }
            } else {
                if use_runtime_dispatch {
                    return self.emit_runtime_dispatched_kwargs_call(
                        method_table_name,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                        true,
                    );
                }

                // Compile kwarg values (they go on stack after positional args)
                let kwarg_names: Vec<String> =
                    kwargs.iter().map(|(name, _)| name.to_string()).collect();
                for (_, value) in kwargs {
                    // Infer type and compile value
                    let ty = self.compile_expr(value)?;
                    // For now, leave as is - VM will coerce if needed
                    let _ = ty;
                }
                // Emit CallWithKwargs or CallWithKwargsSplat instruction
                if has_kwargs_splat {
                    self.emit(Instr::CallWithKwargsSplat(
                        method.global_index,
                        args.len(),
                        kwarg_names,
                        kwargs_splat_mask.to_vec(),
                    ));
                } else {
                    self.emit(Instr::CallWithKwargs(
                        method.global_index,
                        args.len(),
                        kwarg_names,
                    ));
                }
            }
            let hof_function_name = method_table_name
                .strip_prefix("Base.")
                .unwrap_or(method_table_name);
            let has_hof_callsite_return_inference = hof_function_name == "map" && args.len() >= 3;
            let has_known_callsite_return_override =
                is_truncated_result_call(function, args, kwargs);
            if has_any_arg
                && has_multiple_methods
                && kwargs.is_empty()
                && !has_hof_callsite_return_inference
                && !has_known_callsite_return_override
            {
                return Ok(ValueType::Any);
            }
            if self.function_index_is_generated(method.global_index) {
                return Ok(ValueType::Any);
            }
            // Override return type for functions known to return DataType or specific struct types
            let mut return_type = match function {
                "zeros" | "ones" => self.infer_zeros_ones_array_type(args),
                "typeof" | "promote_type" | "promote_rule" | "eltype" | "keytype" | "valtype" => {
                    ValueType::DataType
                }
                "copy" | "Base.copy"
                    if args.len() == 1
                        && matches!(self.infer_expr_type(&args[0]), ValueType::Dict) =>
                {
                    ValueType::Dict
                }
                // `copy(s::Set{T})` returns a fresh `Set{T}` struct (Issue #6721),
                // so the result keeps the Set struct ValueType. Without this, the
                // parametric `copy(::Set{T})` method's primitive return metadata
                // widens to Any, and a following `x in c` would dispatch through
                // the `Any` path and loosely match `in(_, ::KeySet)`.
                "copy" | "Base.copy"
                    if args.len() == 1
                        && matches!(&self.infer_expr_type(&args[0]), ValueType::Struct(type_id)
                            if self.shared_ctx.get_struct_name(*type_id)
                                .is_some_and(|name| name == "Set" || name.starts_with("Set{"))) =>
                {
                    self.infer_expr_type(&args[0])
                }
                "truncated" | "Distributions.truncated"
                    if args.len() >= 2
                        || kwargs.iter().any(|(_, value)| {
                            !matches!(value, Expr::Literal(Literal::Nothing, _))
                        }) =>
                {
                    self.shared_ctx
                        .get_struct_type_id("Distributions.Truncated")
                        .or_else(|| self.shared_ctx.get_struct_type_id("Truncated"))
                        .map(ValueType::Struct)
                        .unwrap_or_else(|| method.return_type.clone())
                }
                "view" | "Base.view" => self
                    .infer_view_call_return_type(function, args, &arg_types)
                    .unwrap_or_else(|| method.return_type.clone()),
                // HOF (Higher-Order Functions) - call-site specialization for better type inference
                "map" | "Base.map" if args.len() == 2 => {
                    // map(f, arr) - infer return type based on f's return type
                    if let Some(ty) = self.infer_map_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "map" | "Base.map" if args.len() == 3 => {
                    // binary map(f, left, right) - infer from element-wise callable.
                    if let Some(ty) =
                        self.infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                    {
                        ty
                    } else if let Some(ty) = self
                        .infer_binary_map_call_return_type_from_julia_types(
                            &args[0],
                            &arg_types[1],
                            &arg_types[2],
                        )
                    {
                        ty
                    } else if method.param_count() > 2 {
                        // Element-type fallback sourced core-projection-first
                        // via the canonical inverse (Issue #6495, stage
                        // 6b-iii); `params.len()` is an arity read.
                        let left_param_ty = method.projected_param_julia_type(1).into_owned();
                        let right_param_ty = method.projected_param_julia_type(2).into_owned();
                        self.infer_binary_map_call_return_type_from_julia_types(
                            &args[0],
                            &left_param_ty,
                            &right_param_ty,
                        )
                        .unwrap_or_else(|| method.return_type.clone())
                    } else {
                        method.return_type.clone()
                    }
                }
                "map" | "Base.map" if args.len() >= 4 => {
                    // n-ary map(f, left, right, rest...) - infer from element-wise callable.
                    if let Some(ty) = self.infer_nary_map_call_return_type(&args[0], &args[1..]) {
                        ty
                    } else if let Some(ty) = self
                        .infer_nary_map_call_return_type_from_julia_types(&args[0], &arg_types[1..])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() == 2 => {
                    // unary broadcast(f, arr) - infer return type like map(f, arr)
                    if let Some(ty) = self.infer_map_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() == 3 => {
                    // binary broadcast(f, left, right) - infer from element-wise callable.
                    if let Some(ty) =
                        self.infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() >= 4 => {
                    // n-ary broadcast(f, left, right, rest...) - infer from element-wise callable.
                    if let Some(ty) = self.infer_nary_map_call_return_type(&args[0], &args[1..]) {
                        ty
                    } else if let Some(ty) = self
                        .infer_nary_map_call_return_type_from_julia_types(&args[0], &arg_types[1..])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "filter" | "Base.filter" if args.len() == 2 => {
                    // filter(pred, arr) - return type has same element type as input
                    if let Some(ty) = self.infer_filter_call_return_type(&args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "mapreduce" | "mapfoldl" | "mapfoldr" | "Base.mapreduce" | "Base.mapfoldl"
                | "Base.mapfoldr"
                    if args.len() >= 3 =>
                {
                    // mapreduce(f, op, itr) / mapfoldl / mapfoldr - infer from
                    // mapped element type and reducer when both are visible.
                    if let Some(ty) = self.infer_mapreduce_call_return_type(
                        &args[0],
                        &args[1],
                        &args[2],
                        args.get(3),
                    ) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "reduce" | "foldl" | "Base.reduce" | "Base.foldl" if args.len() >= 2 => {
                    // reduce(op, itr) or reduce(op, itr, init)
                    // Return type is the element type of the iterator
                    if let Some(ty) =
                        self.infer_reduce_call_return_type(&args[0], &args[1], args.get(2))
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "foldr" | "Base.foldr" if args.len() >= 2 => {
                    // foldr(op, itr) - same as reduce for return type inference
                    if let Some(ty) =
                        self.infer_reduce_call_return_type(&args[0], &args[1], args.get(2))
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                // Iterator wrapper functions return specific struct types
                "enumerate" => {
                    // enumerate(iter) returns Enumerate{typeof(iter)}
                    // Instantiate Enumerate{Any} since we don't track the concrete type
                    self.shared_ctx
                        .resolve_instantiation("Enumerate", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "zip" => {
                    // zip returns Zip/Zip3/... depending on arity (Issues #1990/#4281)
                    let any_types: Vec<JuliaType> =
                        (0..args.len()).map(|_| JuliaType::Any).collect();
                    let struct_name = match args.len() {
                        3 => "Zip3",
                        4 => "Zip4",
                        5 => "Zip5",
                        6 => "Zip6",
                        7 => "Zip7",
                        _ => "Zip", // 2 args (default)
                    };
                    self.shared_ctx
                        .resolve_instantiation(struct_name, &any_types)
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "take" => {
                    // take(iter, n) returns Take{typeof(iter)}
                    // Instantiate Take{Any} since we don't track concrete inner type
                    self.shared_ctx
                        .resolve_instantiation("Take", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "drop" => {
                    // drop(iter, n) returns Drop{typeof(iter)}
                    // Instantiate Drop{Any} since we don't track concrete inner type
                    self.shared_ctx
                        .resolve_instantiation("Drop", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "rest" => {
                    // rest(iter, state) returns Rest{typeof(iter), typeof(state)}.
                    // rest(iter) is the identity; preserve the method return type there.
                    if args.len() == 2 {
                        self.shared_ctx
                            .resolve_instantiation("Rest", &[JuliaType::Any, JuliaType::Any])
                            .map(ValueType::Struct)
                            .unwrap_or(method.return_type.clone())
                    } else {
                        method.return_type.clone()
                    }
                }
                "iterate" => {
                    // iterate(collection) and iterate(collection, state) return (element, state) or nothing
                    // For compilation purposes, treat as Tuple to enable proper tuple indexing (y[2])
                    // This is safe because code should check `y === nothing` before accessing y[2]
                    ValueType::Tuple
                }
                _ => method.return_type.clone(),
            };

            if matches!(return_type, ValueType::Any) {
                let arg_value_types: Vec<ValueType> =
                    args.iter().map(|arg| self.infer_expr_type(arg)).collect();
                if !arg_value_types
                    .iter()
                    .any(|ty| matches!(ty, ValueType::Any))
                {
                    if let Some(func_ir) = self
                        .shared_ctx
                        .function_ir_by_global_index
                        .get(&method.global_index)
                    {
                        let inferred = self.infer_shared_function_return_type_with_arg_types(
                            func_ir,
                            &arg_value_types,
                        );
                        if self.should_accept_body_reinferred_call_return_type(&inferred) {
                            return_type = inferred;
                        }
                    }
                }
            }

            // If constructor return inference is still `Any`, keep it unknown.
            // A type name alone does not prove the return identity: Julia outer
            // constructors are ordinary methods and may return another type.
            // Choosing a same-base registry entry also makes static dispatch
            // order-dependent and violates parametric invariance (Issue #11434).
            // Proven default-inner and call-site transfer-function results are
            // propagated by their constructor-specific paths before reaching
            // this generic tail.

            Ok(return_type)
        } else {
            if self.usings.contains("Random") && is_random_function(function) {
                return self.compile_builtin(&BuiltinOp::Seed, args);
            }

            // Note: mean is now Pure Julia (stdlib/Statistics/src/Statistics.jl)
            // It's dispatched through the method table like other user-defined functions.

            // Handle n-arg reducible operators (+ and *) when there's no method table
            // This happens when flattening produces +(a, b, c, ...) with no user-defined +
            if is_reducible_nary_operator(function) && args.len() > 2 {
                // Reduce to chained binary ops using builtin operators
                return self.compile_nary_builtin_reduction(function, args);
            }

            // Try to map to BuiltinOp first (handles types properly)
            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                return self.compile_builtin(&builtin_op, args);
            }
            if is_builtin_type_name(function) && args.len() != 1 {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::PushDataType(function.to_string()));
                self.emit(Instr::CallFunctionVariable(args.len()));
                return Ok(ValueType::Any);
            }
            // Fall back to string-based builtin call for functions not in BuiltinOp
            match self.compile_builtin_call(function, args) {
                Ok(ty) => Ok(ty),
                Err(CompileError::Msg(msg))
                    if msg.starts_with("Unknown function: ")
                        && self
                            .shared_ctx
                            .runtime_nominal_callable_names
                            .contains(function) =>
                {
                    let splat_mask = vec![false; args.len()];
                    self.compile_function_variable_call_with_kwargs(
                        function,
                        args,
                        kwargs,
                        &splat_mask,
                        kwargs_splat_mask,
                    )
                }
                Err(CompileError::Msg(msg))
                    if msg.starts_with("Unknown function: ")
                        && opaque_runtime_eval_targets_function(
                            &self.shared_ctx.opaque_runtime_eval_function_names,
                            function,
                        ) =>
                {
                    let splat_mask = vec![false; args.len()];
                    self.compile_runtime_global_function_call(
                        function,
                        args,
                        kwargs,
                        &splat_mask,
                        kwargs_splat_mask,
                    )
                }
                Err(CompileError::Msg(msg)) if msg.starts_with("Unknown function: ") => {
                    // `function` resolves to no method/builtin/global anywhere:
                    // upstream raises `UndefVarError` for a call to an
                    // undefined name, the same as reading it bare (Issue
                    // #10354's fixture-fallout measurement,
                    // `modules/module_selective_using_globals_7955.jl`; see
                    // docs/vm/EXCEPTION_PARITY.md). Previously raised via
                    // `ThrowError` (a generic `ErrorException` carrying this
                    // "Unknown function: " diagnostic message) — the "nearest
                    // error" every unresolved-name call site fell back to,
                    // diverging from upstream's exception TYPE even though
                    // both sides agree the call is invalid.
                    self.emit(Instr::ThrowUndefVarError(function.to_string()));
                    Ok(ValueType::Any)
                }
                Err(err) => Err(err),
            }
        }
    }
}

fn method_table_has_non_base_methods_for_opaque_eval(
    table: &crate::compile::method_table::MethodTable,
) -> bool {
    let base_function_count = table.base_function_count();
    table
        .methods
        .iter()
        .any(|method| !method.is_base_program_method(base_function_count))
}

fn opaque_runtime_eval_targets_function(
    names: &std::collections::HashSet<String>,
    function: &str,
) -> bool {
    names.contains(function)
        || function
            .rsplit('.')
            .next()
            .is_some_and(|short_name| names.contains(short_name))
}

/// CoreType-native port of the `Type{...}` parameter probe
/// (`matches!(ty, JuliaType::TypeOf(_))`) over the canonical `core_signature`
/// projection (Issue #6495, stage 6b-iii).
///
/// Image analysis: `JuliaType::TypeOf(_)` images exactly as
/// `CoreType::TypeOf(_)`. The only other spelling producing that image is a
/// `JuliaType::Struct("Type{..}")` name string (via `from_julia_name`'s
/// `"Type"` parametric arm), which lowering never emits for a `::Type{T}`
/// annotation and which the canonical inverse never reconstructs — pinned
/// over the Base corpus by `compile::cache::tests::`
/// `base_method_core_call_dispatch_heuristics_parity_issue_6495`.
pub(crate) fn core_is_typeof_param(core: &CoreType) -> bool {
    matches!(core, CoreType::TypeOf(_))
}

/// CoreType-native port of the generic `Type{T}`-with-`TypeVar` fallback
/// probe (`matches!(ty, JuliaType::TypeOf(inner) if inner is TypeVar)`) over
/// the canonical `core_signature` projection (Issue #6495, stage 6b-iii).
///
/// Known image-collision caveat (same as the stage-4 singleton scorers): a
/// parameter spelled `JuliaType::TypeOf(JuliaType::Struct("Q"))` — a
/// single-letter non-var name inside `Type{...}` — images as
/// `TypeOf(TypeVar)` and would satisfy the core probe where the legacy probe
/// did not. That spelling is unreachable from lowering and from the
/// canonical inverse; the parity gate + full suite referee (zero hits).
pub(crate) fn core_is_typeof_typevar_param(core: &CoreType) -> bool {
    matches!(core, CoreType::TypeOf(inner) if matches!(inner.as_ref(), CoreType::TypeVar(_)))
}

/// CoreType-native `Any`-parameter probe (Issue #6495, stage 6b-iii).
///
/// Accepted-divergence note (same as the stage-6a Any-count tie-breaker): a
/// parameter spelled `JuliaType::Struct("Any")` images as `CoreType::Any` —
/// unreachable from lowering (`from_name` resolves `Any`) and from the
/// canonical inverse; parity gate + suite referee.
pub(crate) fn core_is_any_param(core: &CoreType) -> bool {
    matches!(core, CoreType::Any)
}

/// Whether any declared parameter is a `Type{...}` pattern — the
/// `CallTypedDispatch` eligibility probe of the type-object dispatch
/// heuristics, read from the `core_signature` projection (Issue #6495,
/// stage 6b-iii).
fn method_has_typeof_param(m: &crate::compile::method_table::MethodSig) -> bool {
    m.any_param_matches(core_is_typeof_param)
}

/// Whether any declared parameter is the generic `Type{T}` (TypeVar) pattern
/// — the runtime-fallback method finder of the type-object dispatch
/// heuristics, read from the `core_signature` projection (Issue #6495,
/// stage 6b-iii).
fn method_has_typeof_typevar_param(m: &crate::compile::method_table::MethodSig) -> bool {
    m.any_param_matches(core_is_typeof_typevar_param)
}

/// Whether declared parameter `idx` exists and is `Any`, read from the
/// `core_signature` projection (Issue #6495, stage 6b-iii).
#[cfg(test)]
fn method_param_is_any_at(m: &crate::compile::method_table::MethodSig, idx: usize) -> bool {
    m.param_matches_at(idx, core_is_any_param)
}

fn method_param_is_any_at_call_position(
    m: &crate::compile::method_table::MethodSig,
    idx: usize,
) -> bool {
    m.param_matches_at_call_position(idx, core_is_any_param)
}

/// Whether declared parameter `idx` exists and is NOT `Any`, read from the
/// `core_signature` projection; `false` for out-of-range `idx` (preserving
/// the `zip`/`params.get` truncation of the legacy readers — Issue #6495,
/// stage 6b-iii).
fn method_param_is_not_any_at(m: &crate::compile::method_table::MethodSig, idx: usize) -> bool {
    m.param_matches_at(idx, |core| !core_is_any_param(core))
}

fn method_param_is_not_any_at_call_position(
    m: &crate::compile::method_table::MethodSig,
    idx: usize,
) -> bool {
    m.param_matches_at_call_position(idx, |core| !core_is_any_param(core))
}

/// Shared dispatch policy: does a call whose static dispatch selected `method`
/// need to defer to runtime multiple dispatch rather than statically bind it?
///
/// Used by BOTH the unqualified bare-call path (`compile_generic_dispatch_call`)
/// and the qualified `Module.f(x)` path (`compile_module_call_via_method_table`)
/// so a qualified call dispatches identically to the same unqualified call
/// (Issue #8158). A wide `Any` argument statically selects the catch-all
/// `f(::Any)`, but the runtime value may match a more-specific method; the
/// unqualified path already runtime-dispatched here, the qualified path did not —
/// so `SciMLBase._callbacks(cb::CallbackSet)` mis-dispatched to the `(cb,)`
/// catch-all and silently disabled every callback in a `CallbackSet`.
///
/// - single `Any` arg with multiple methods: the statically-selected method may
///   be the catch-all while the runtime value matches a more-specific method.
/// - multi-arg `Any` cases (case1/2/3) plus the abstract-array-family probe
///   (case4, Issue #6495 stages 7a/7c-ii).
pub(crate) fn should_runtime_dispatch(
    table: &crate::compile::method_table::MethodTable,
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
    has_any_arg: bool,
) -> bool {
    let has_multiple_methods = table.methods.len() > 1;
    let use_single_arg_runtime_dispatch =
        single_arg_runtime_dispatch_required(table, method, arg_types, has_any_arg);
    let use_multi_arg_runtime_dispatch = has_multiple_methods && args_len > 1 && {
        // The per-slot `Any` probes read the canonical `core_signature`
        // projection first (Issue #6495, stage 6b-iii); `param_count()`
        // comparisons are arity reads. Out-of-range slots are non-matches,
        // preserving the legacy `zip` truncation / `params.get` `Option` gates.
        let (case1, case2, case3) = if has_any_arg {
            let case1 = arg_types.iter().enumerate().any(|(idx, arg_ty)| {
                matches!(arg_ty, JuliaType::Any)
                    && method_param_is_not_any_at_call_position(method, idx)
            });
            let case2 = !case1 && {
                let matched_all_any = method.all_params_match(core_is_any_param);
                matched_all_any
                    && table.methods.iter().any(|m| {
                        m.accepts_arity(args_len)
                            && m.global_index != method.global_index
                            && m.any_param_matches(|core| !core_is_any_param(core))
                    })
            };
            let case3 = !case1
                && !case2
                && table.methods.iter().any(|m| {
                    m.accepts_arity(args_len)
                        && m.global_index != method.global_index
                        && (0..args_len).any(|idx| {
                            matches!(arg_types.get(idx), Some(JuliaType::Any))
                                && method_param_is_any_at_call_position(method, idx)
                                && method_param_is_not_any_at_call_position(m, idx)
                        })
                });
            (case1, case2, case3)
        } else {
            (false, false, false)
        };
        // Abstract-array-family probe sourced from the canonical
        // `core_signature` projection (Issue #6495, stages 7a/7c-ii).
        let case4 = table.methods.iter().any(|m| {
            m.global_index != method.global_index
                && m.accepts_arity(args_len)
                && (0..args_len).any(|idx| {
                    arg_types
                        .get(idx)
                        .is_some_and(is_rank_unknown_array_julia_type)
                        && m.param_matches_at_call_position(idx, core_is_abstract_array_family_type)
                })
        });
        let case5 = runtime_unknown_struct_arg_requires_dispatch(method, arg_types, args_len)
            || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
                method, arg_types, args_len,
            );
        case1 || case2 || case3 || case4 || case5
    };
    use_single_arg_runtime_dispatch || use_multi_arg_runtime_dispatch
}

fn single_arg_runtime_dispatch_required(
    table: &crate::compile::method_table::MethodTable,
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    has_any_arg: bool,
) -> bool {
    if table.methods.len() <= 1 || arg_types.len() != 1 {
        return false;
    }
    has_any_arg
        || table.methods.iter().any(|m| {
            m.global_index != method.global_index
                && m.accepts_arity(1)
                && arg_types
                    .first()
                    .is_some_and(is_rank_unknown_array_julia_type)
                && method_param_is_not_any_at_call_position(m, 0)
        })
        || runtime_unknown_struct_arg_requires_dispatch(method, arg_types, 1)
        || method_has_anonymous_bounded_parametric_struct_for_struct_arg(method, arg_types, 1)
}

pub(crate) fn should_use_dynamic_call_for_runtime_dispatch(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    runtime_unknown_struct_arg_requires_dispatch(method, arg_types, args_len)
        || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
            method, arg_types, args_len,
        )
}

fn runtime_unknown_struct_arg_requires_dispatch(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    let Some(core_params) = method.expanded_core_param_types_for_arity(args_len) else {
        return arg_types.iter().any(is_runtime_unknown_struct_arg);
    };
    core_params
        .iter()
        .zip(arg_types.iter())
        .any(|(param, arg)| {
            is_runtime_unknown_struct_arg(arg)
                && !julia_struct_arg_matches_param(
                    &crate::inference_core::core_type_to_julia_type(param),
                    arg,
                )
        })
}

fn julia_struct_arg_matches_param(param: &JuliaType, arg: &JuliaType) -> bool {
    match (param, arg) {
        (JuliaType::Struct(param_name), JuliaType::Struct(arg_name)) => {
            nominal_family_names_compatible(param_name, arg_name)
        }
        _ => param == arg,
    }
}

fn method_has_anonymous_bounded_parametric_struct_for_struct_arg(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    let Some(core_params) = method.expanded_core_param_types_for_arity(args_len) else {
        return false;
    };
    let where_vars = method
        .core_signature_type_vars()
        .into_iter()
        .map(|var| var.name)
        .collect::<std::collections::HashSet<_>>();
    core_params
        .iter()
        .zip(arg_types.iter())
        .any(|(param, arg)| {
            is_runtime_unknown_struct_arg(arg)
                && has_anonymous_bounded_typevar_inside_parametric_struct(param, &where_vars)
        })
        || method
            .projected_param_julia_types()
            .iter()
            .zip(arg_types.iter())
            .any(|(param, arg)| {
                is_runtime_unknown_struct_arg(arg)
                    && matches!(param, JuliaType::Struct(name) if name.contains("<:"))
            })
}

fn has_anonymous_bounded_typevar_inside_parametric_struct(
    ty: &CoreType,
    where_vars: &std::collections::HashSet<String>,
) -> bool {
    match ty {
        CoreType::Struct { params, .. } => params.iter().any(|param| {
            matches!(param, CoreType::TypeVar(var)
                if !where_vars.contains(&var.name)
                    && (var.lower_bound.is_some() || var.upper_bound.is_some()))
                || has_anonymous_bounded_typevar_inside_parametric_struct(param, where_vars)
        }),
        CoreType::Named(name) => name.contains("<:"),
        CoreType::Tuple(items) | CoreType::Union(items) => items
            .iter()
            .any(|item| has_anonymous_bounded_typevar_inside_parametric_struct(item, where_vars)),
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
            has_anonymous_bounded_typevar_inside_parametric_struct(inner, where_vars)
        }
        CoreType::VarargLen { element, len } => {
            has_anonymous_bounded_typevar_inside_parametric_struct(element, where_vars)
                || has_anonymous_bounded_typevar_inside_parametric_struct(len, where_vars)
        }
        CoreType::NamedTuple(fields) => fields.iter().any(|(_, field_ty)| {
            has_anonymous_bounded_typevar_inside_parametric_struct(field_ty, where_vars)
        }),
        CoreType::UnionAll { var, body } => {
            let mut nested_where_vars = where_vars.clone();
            nested_where_vars.insert(var.name.clone());
            has_anonymous_bounded_typevar_inside_parametric_struct(body, &nested_where_vars)
        }
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        core_is_any_param, core_is_typeof_param, core_is_typeof_typevar_param,
        method_has_typeof_param, method_has_typeof_typevar_param, method_param_is_any_at,
        method_param_is_not_any_at, should_runtime_dispatch,
    };
    use crate::bytecode::ValueType;
    use crate::compile::method_table::{MethodSig, MethodTable};
    use crate::inference_core::{core_type_to_julia_type, CoreType};
    use crate::types::{JuliaType, TypeParam};

    /// Round-tripping parameter spellings the call-dispatch heuristics see:
    /// the CoreType-native predicates must agree with the canonical inverse of
    /// the same core row (Issue #6495, stage 7c-ii).
    #[test]
    fn call_dispatch_predicates_match_canonical_inverse_issue_6495() {
        let shapes = vec![
            JuliaType::Any,
            JuliaType::Int64,
            JuliaType::Float64,
            JuliaType::String,
            JuliaType::DataType,
            JuliaType::Number,
            JuliaType::Struct("Complex{Float64}".to_string()),
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                Some("Real".to_string()),
            ))),
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]),
        ];
        for ty in shapes {
            let core = CoreType::from(&ty);
            let projected = core_type_to_julia_type(&core);
            assert_eq!(
                core_is_typeof_param(&core),
                matches!(projected, JuliaType::TypeOf(_)),
                "typeof_param diverges for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_typeof_typevar_param(&core),
                matches!(
                    &projected,
                    JuliaType::TypeOf(inner)
                        if matches!(inner.as_ref(), JuliaType::TypeVar(_, _))
                ),
                "typeof_typevar_param diverges for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_any_param(&core),
                matches!(projected, JuliaType::Any),
                "any_param diverges for {ty:?} (core {core:?})"
            );
        }
    }

    /// The method-level wrappers read the structured `core_signature` path;
    /// on a Bottom placeholder they report the conservative defaults (Issue
    /// #6495, stage 7c-ii).
    #[test]
    fn call_dispatch_method_probes_read_canonical_signature_issue_6495() {
        let make_params = |tys: Vec<JuliaType>| {
            tys.into_iter()
                .enumerate()
                .map(|(i, ty)| (format!("x{i}"), ty))
                .collect::<Vec<_>>()
        };
        let shape_rows = vec![
            vec![JuliaType::Any, JuliaType::Int64],
            vec![JuliaType::Any, JuliaType::Any],
            vec![
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Int64,
            ],
            vec![JuliaType::TypeOf(Box::new(JuliaType::Int64))],
            vec![JuliaType::Struct("Complex{Float64}".to_string())],
        ];
        for tys in shape_rows {
            let params = make_params(tys);
            let bottom = MethodSig::bottom_for_tests(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                None,
                None,
            );

            assert!(bottom.structured_arg_core_types().is_none());
            assert!(!method_has_typeof_param(&bottom));
            assert!(!method_has_typeof_typevar_param(&bottom));
            for i in 0..=bottom.param_count() {
                assert!(!method_param_is_any_at(&bottom, i));
                assert!(!method_param_is_not_any_at(&bottom, i));
            }
            assert!(bottom.projected_param_julia_types().is_empty());

            let sig = MethodSig::for_tests(
                0,
                7,
                params,
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            );
            assert!(sig.structured_arg_core_types().is_some());
            let row = sig.projected_param_julia_types();
            let expected = (
                row.iter().any(|ty| matches!(ty, JuliaType::TypeOf(_))),
                row.iter().any(|ty| {
                    matches!(
                        ty,
                        JuliaType::TypeOf(inner)
                            if matches!(inner.as_ref(), JuliaType::TypeVar(_, _))
                    )
                }),
                (0..=sig.param_count()) // one past the end: out-of-range is false
                    .map(|i| {
                        let at = row.get(i);
                        (
                            at.is_some_and(|ty| matches!(ty, JuliaType::Any)),
                            at.is_some_and(|ty| !matches!(ty, JuliaType::Any)),
                        )
                    })
                    .collect::<Vec<_>>(),
                row.clone(),
            );
            let structured = (
                method_has_typeof_param(&sig),
                method_has_typeof_typevar_param(&sig),
                (0..=sig.param_count())
                    .map(|i| {
                        (
                            method_param_is_any_at(&sig, i),
                            method_param_is_not_any_at(&sig, i),
                        )
                    })
                    .collect::<Vec<_>>(),
                sig.projected_param_julia_types(),
            );
            assert_eq!(expected, structured, "probe divergence for {row:?}");
        }
    }

    #[test]
    fn runtime_dispatch_probe_sees_vararg_call_positions_issue_8407() {
        let generic = MethodSig::for_tests(
            0,
            10,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("ys".to_string(), JuliaType::Any),
            ],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            Some(1),
            None,
        );
        let specific = MethodSig::for_tests(
            1,
            20,
            vec![
                (
                    "x".to_string(),
                    JuliaType::Struct("QuadGK.BatchIntegrand{Y, Nothing}".to_string()),
                ),
                ("y".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ("z".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                (
                    "rest".to_string(),
                    JuliaType::TypeVar("T".to_string(), None),
                ),
            ],
            ValueType::I64,
            None,
            false,
            vec![
                TypeParam::new("Y".to_string()),
                TypeParam::new("T".to_string()),
            ],
            CoreType::Bottom,
            Some(3),
            None,
        );
        let mut table = MethodTable::new("myq".to_string());
        table.add_method(generic);
        table.add_method(specific);
        let selected = table
            .dispatch(&[JuliaType::Any, JuliaType::Float64, JuliaType::Float64])
            .expect("dispatch");

        assert_eq!(selected.global_index, 10);
        assert!(should_runtime_dispatch(
            &table,
            selected,
            &[JuliaType::Any, JuliaType::Float64, JuliaType::Float64],
            3,
            true
        ));
    }
}
