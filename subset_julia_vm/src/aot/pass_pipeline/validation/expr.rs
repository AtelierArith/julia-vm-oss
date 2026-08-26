use super::error;
use crate::aot::ir::AotExpr;
use crate::aot::native_calls::is_native_call_target;
use crate::aot::pass_pipeline::AotPassStage;
use crate::aot::types::StaticType;
use crate::aot::AotResult;

pub(super) fn verify_expr(stage: AotPassStage, function: &str, expr: &AotExpr) -> AotResult<()> {
    match expr {
        AotExpr::Var { name, .. } => {
            if name.trim().is_empty() {
                error(stage, function, "variable reference has an empty name")
            } else {
                Ok(())
            }
        }
        AotExpr::BinOpStatic { left, right, .. } | AotExpr::BinOpDynamic { left, right, .. } => {
            verify_expr(stage, function, left)?;
            verify_expr(stage, function, right)
        }
        AotExpr::UnaryOp { operand, .. } => verify_expr(stage, function, operand),
        AotExpr::CallStatic {
            function: name,
            args,
            ..
        }
        | AotExpr::CallDynamic {
            function: name,
            args,
        } => {
            if name.trim().is_empty() {
                return error(stage, function, "call target name is empty");
            }
            if is_native_call_target(name) {
                return error(
                    stage,
                    function,
                    &format!(
                        "native call boundary `{}` reached AoT backend as an ordinary call",
                        name
                    ),
                );
            }
            verify_exprs(stage, function, args)
        }
        AotExpr::CallBuiltin { args, .. } => verify_exprs(stage, function, args),
        AotExpr::ArrayLit {
            elements, shape, ..
        } => {
            let expected = shape
                .iter()
                .try_fold(1usize, |acc, dim| acc.checked_mul(*dim));
            if expected != Some(elements.len()) {
                return error(
                    stage,
                    function,
                    &format!(
                        "array literal shape {:?} expects {:?} elements, got {}",
                        shape,
                        expected,
                        elements.len()
                    ),
                );
            }
            verify_exprs(stage, function, elements)
        }
        AotExpr::TupleLit { elements }
        | AotExpr::StructNew {
            fields: elements, ..
        } => verify_exprs(stage, function, elements),
        AotExpr::SetFromIter { iter, .. } => verify_expr(stage, function, iter),
        AotExpr::NamedTupleLit { fields } => {
            for (name, field) in fields {
                if name.trim().is_empty() {
                    return error(stage, function, "named tuple field has an empty name");
                }
                verify_expr(stage, function, field)?;
            }
            Ok(())
        }
        AotExpr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        }
        | AotExpr::Generator {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            if var.trim().is_empty() {
                return error(
                    stage,
                    function,
                    "comprehension/generator variable name is empty",
                );
            }
            verify_expr(stage, function, iter)?;
            if let Some(filter) = filter {
                verify_expr(stage, function, filter)?;
            }
            verify_expr(stage, function, body)
        }
        AotExpr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            if iterations.is_empty() {
                return error(stage, function, "multi-comprehension has no iterations");
            }
            for (var, iter) in iterations {
                if var.trim().is_empty() {
                    return error(stage, function, "comprehension variable name is empty");
                }
                verify_expr(stage, function, iter)?;
            }
            if let Some(filter) = filter {
                verify_expr(stage, function, filter)?;
            }
            verify_expr(stage, function, body)
        }
        AotExpr::Index {
            array,
            indices,
            is_tuple,
            ..
        } => {
            if indices.is_empty()
                && (*is_tuple
                    || !matches!(array.get_type(), StaticType::Array { ndims: Some(0), .. }))
            {
                return error(stage, function, "index expression has no indices");
            }
            verify_expr(stage, function, array)?;
            verify_exprs(stage, function, indices)
        }
        AotExpr::Range {
            start, stop, step, ..
        } => {
            verify_expr(stage, function, start)?;
            verify_expr(stage, function, stop)?;
            if let Some(step) = step {
                verify_expr(stage, function, step)?;
            }
            Ok(())
        }
        AotExpr::FieldAccess { object, field, .. } => {
            if field.trim().is_empty() {
                return error(stage, function, "field access has an empty field name");
            }
            verify_expr(stage, function, object)
        }
        AotExpr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            verify_expr(stage, function, condition)?;
            verify_expr(stage, function, then_expr)?;
            verify_expr(stage, function, else_expr)
        }
        AotExpr::Box(inner)
        | AotExpr::Unbox { value: inner, .. }
        | AotExpr::Convert { value: inner, .. } => verify_expr(stage, function, inner),
        AotExpr::Lambda {
            params,
            body,
            captures,
            ..
        } => {
            for (name, _) in params.iter().chain(captures.iter()) {
                if name.trim().is_empty() {
                    return error(stage, function, "lambda binding has an empty name");
                }
            }
            for (index, stmt) in body.iter().enumerate() {
                super::stmt::verify_stmt(stage, function, index, stmt)?;
            }
            Ok(())
        }
        AotExpr::LitI64(_)
        | AotExpr::LitI32(_)
        | AotExpr::LitF64(_)
        | AotExpr::LitF32(_)
        | AotExpr::LitBool(_)
        | AotExpr::LitStr(_)
        | AotExpr::LitChar(_)
        | AotExpr::LitNothing
        | AotExpr::LitMissing => Ok(()),
    }
}

fn verify_exprs(stage: AotPassStage, function: &str, exprs: &[AotExpr]) -> AotResult<()> {
    for expr in exprs {
        verify_expr(stage, function, expr)?;
    }
    Ok(())
}
