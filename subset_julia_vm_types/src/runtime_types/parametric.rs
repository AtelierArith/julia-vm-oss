use std::collections::HashMap;

use crate::ir::core::StructDef;
use crate::types::{JuliaType, TypeExpr, TypeParam};

type ParametricResult<T> = Result<T, String>;

fn err<T>(message: impl Into<String>) -> ParametricResult<T> {
    Err(message.into())
}

fn record_binding(
    name: &str,
    actual: &JuliaType,
    inferred: &mut HashMap<String, JuliaType>,
) -> ParametricResult<()> {
    if let Some(existing) = inferred.get(name) {
        if existing == actual {
            return Ok(());
        }
        // The bindings differ. Only an imprecise `Any` placeholder may be
        // reconciled; two distinct concrete types do not unify (Issue #8102).
        if *existing != JuliaType::Any && *actual != JuliaType::Any {
            return err(format!(
                "Inconsistent type inference for {}: {} vs {}",
                name, existing, actual
            ));
        }
        // This occurrence is the unknown `Any`: keep the concrete binding.
        if *actual == JuliaType::Any {
            return Ok(());
        }
        // The prior binding was the unknown `Any`: refine to the concrete type
        // by falling through to the insert below.
    }
    inferred.insert(name.to_string(), actual.clone());
    Ok(())
}

pub fn bind_type_vars_from_expr(
    type_expr: &TypeExpr,
    actual: &JuliaType,
    param_names: &[&str],
    inferred: &mut HashMap<String, JuliaType>,
) -> ParametricResult<()> {
    match type_expr {
        TypeExpr::TypeVar(name) => {
            // Only bind names that are declared type parameters of the struct.
            if param_names.contains(&name.as_str()) {
                record_binding(name, actual, inferred)?;
            }
            Ok(())
        }
        TypeExpr::Concrete(_) => Ok(()),
        TypeExpr::RuntimeExpr(_) => Ok(()),
        TypeExpr::Parameterized { base, params } => {
            match (base.as_str(), actual) {
                // Tuple{T1, T2, ...} vs TupleOf([...])
                ("Tuple", JuliaType::TupleOf(elems)) if elems.len() == params.len() => {
                    for (p, e) in params.iter().zip(elems.iter()) {
                        bind_type_vars_from_expr(p, e, param_names, inferred)?;
                    }
                }
                // Array{T}/Vector{T} vs VectorOf(elem)
                ("Array" | "Vector", JuliaType::VectorOf(elem)) if !params.is_empty() => {
                    bind_type_vars_from_expr(&params[0], elem, param_names, inferred)?;
                }
                // Array{T}/Matrix{T} vs MatrixOf(elem)
                ("Array" | "Matrix", JuliaType::MatrixOf(elem)) if !params.is_empty() => {
                    bind_type_vars_from_expr(&params[0], elem, param_names, inferred)?;
                }
                // Parametric struct Foo{T,...} vs Struct("Foo{Int64,...}")
                (_, JuliaType::Struct(actual_name)) => {
                    if let Some(actual_params) = parse_struct_type_params(actual_name, base) {
                        if actual_params.len() == params.len() {
                            for (p, a) in params.iter().zip(actual_params.iter()) {
                                bind_type_vars_from_expr(p, a, param_names, inferred)?;
                            }
                        }
                    }
                }
                _ => {
                    // Shapes do not align (for example, actual is Any): leave
                    // embedded type vars unbound. The caller reports unbound
                    // params.
                }
            }
            Ok(())
        }
    }
}

fn bind_type_vars_from_param_bounds(
    type_params: &[TypeParam],
    inferred: &mut HashMap<String, JuliaType>,
) -> ParametricResult<()> {
    let mut changed = true;
    while changed {
        changed = false;
        for param in type_params {
            let Some(actual) = inferred.get(&param.name).cloned() else {
                continue;
            };
            let Some(bound_name) = param.get_upper_bound() else {
                continue;
            };
            let bound_pattern = JuliaType::from_name_or_struct(bound_name);
            let Some(bindings) = actual.extract_type_bindings(&bound_pattern, type_params) else {
                continue;
            };
            for (name, ty) in bindings {
                let previous = inferred.get(&name).cloned();
                record_binding(&name, &ty, inferred)?;
                if previous.as_ref() != inferred.get(&name) {
                    changed = true;
                }
            }
        }
    }
    Ok(())
}

/// Infer type arguments from constructor arguments for a parametric struct.
///
/// This is pure JuliaType/TypeExpr binding logic shared by compile-time
/// constructor inference and runtime parametric constructor dispatch.
pub fn infer_parametric_type_args(
    def: &StructDef,
    base_name: &str,
    arg_types: &[JuliaType],
) -> ParametricResult<Vec<JuliaType>> {
    if arg_types.len() != def.fields.len() {
        return err(format!(
            "{} constructor expects {} arguments, got {}",
            base_name,
            def.fields.len(),
            arg_types.len()
        ));
    }

    let mut inferred: HashMap<String, JuliaType> = HashMap::new();

    // Names declared as type parameters of this struct. Only these should be
    // treated as bindable type variables inside field type expressions.
    let param_names: Vec<&str> = def.type_params.iter().map(|p| p.name.as_str()).collect();

    for (field, arg_ty) in def.fields.iter().zip(arg_types.iter()) {
        if let Some(type_expr) = &field.type_expr {
            bind_type_vars_from_expr(type_expr, arg_ty, &param_names, &mut inferred)?;
        }
    }
    bind_type_vars_from_param_bounds(&def.type_params, &mut inferred)?;

    let mut result = Vec::new();
    for param in &def.type_params {
        let ty = inferred.get(&param.name).cloned().ok_or_else(|| {
            format!(
                "Cannot infer type parameter {} for {}",
                param.name, base_name
            )
        })?;
        result.push(ty);
    }

    Ok(result)
}

pub fn parse_struct_type_params(name: &str, base: &str) -> Option<Vec<JuliaType>> {
    let brace_idx = name.find('{')?;
    if &name[..brace_idx] != base {
        return None;
    }
    let close = name.rfind('}')?;
    if close <= brace_idx + 1 {
        return None;
    }
    let inner = &name[brace_idx + 1..close];
    let mut params = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                params.push(JuliaType::from_name_or_struct(inner[start..i].trim()));
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        params.push(JuliaType::from_name_or_struct(last));
    }
    if params.is_empty() {
        None
    } else {
        Some(params)
    }
}
