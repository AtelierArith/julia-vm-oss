//! Runtime method-signature projection helpers shared below the VM facade.
//!
//! These helpers operate only on bytecode-level [`FunctionInfo`] plus the
//! extracted type crate. Keeping them here prevents VM test hooks from being a
//! physical extraction blocker (Issue #9090).

use crate::{parse_parametric_params, FunctionInfo};
use subset_julia_vm_types::inference_core::{
    core_type_to_julia_type,
    dispatch_resolver::{dispatch_core_type_from_julia, embed_type_param_bounds},
};
use subset_julia_vm_types::{JuliaType, TypeParam};

/// Expand a method's declared parameter types for a concrete call arity.
///
/// Varargs methods store the declared vararg slot once. Runtime dispatch and
/// parity guards need the per-argument view for the actual arity being tested.
pub fn expanded_param_types_for_call(
    func: &FunctionInfo,
    arg_len: usize,
) -> Option<Vec<JuliaType>> {
    let Some(vararg_idx) = func.vararg_param_index else {
        return (func.param_julia_types.len() == arg_len).then(|| func.param_julia_types.clone());
    };

    if arg_len < vararg_idx {
        return None;
    }
    if let Some(fixed_count) = func.vararg_fixed_count {
        if arg_len != vararg_idx + fixed_count {
            return None;
        }
    }

    let vararg_ty = func
        .param_julia_types
        .get(vararg_idx)
        .cloned()
        .unwrap_or(JuliaType::Any);
    let vararg_ty = vararg_dispatch_element_type(vararg_ty);
    let mut expanded: Vec<_> = func
        .param_julia_types
        .iter()
        .take(vararg_idx)
        .cloned()
        .collect();
    for _ in vararg_idx..arg_len {
        expanded.push(vararg_ty.clone());
    }
    Some(expanded)
}

/// Derive the runtime type-name signature for calling `func` with `arity`
/// arguments.
///
/// Equality with the canonical compile-time `MethodSig` projection is pinned
/// by the Base-corpus gate
/// `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`
/// in `compile/cache.rs`.
pub fn derived_runtime_signature(func: &FunctionInfo, arity: usize) -> Option<Vec<String>> {
    Some(
        expanded_param_types_for_call(func, arity)?
            .iter()
            .map(|ty| render_runtime_candidate_type(ty, &func.type_params))
            .collect(),
    )
}

fn render_runtime_candidate_type(ty: &JuliaType, type_params: &[TypeParam]) -> String {
    let core = dispatch_core_type_from_julia(ty);
    let core = embed_type_param_bounds(core, type_params);
    core_type_to_julia_type(&core).to_string()
}

fn vararg_dispatch_element_type(ty: JuliaType) -> JuliaType {
    match ty {
        JuliaType::TupleOf(elements) if elements.len() == 1 => {
            elements.into_iter().next().unwrap_or(JuliaType::Any)
        }
        JuliaType::Struct(name) if name.starts_with("Tuple{") => {
            let params = parse_parametric_params(&name);
            if params.len() == 1 {
                JuliaType::from_name_or_struct(params[0].trim())
            } else {
                JuliaType::Struct(name)
            }
        }
        other => other,
    }
}
