//! Runtime `Value` ownership, rooting, and safepoint obligations for AoT.
//!
//! This is a contract layer, not a GC implementation.  The generated Rust
//! backend currently keeps dynamic `Value`s owned by value, while future native
//! backends must either prove the same ownership or root borrowed values across
//! helper calls that can allocate or otherwise invalidate runtime data.

use crate::aot::abi::{AotAbiClass, AotAbiValue};
use crate::aot::ir::{AotBuiltinOp, AotExpr, AotProgram, AotStmt};
use crate::aot::pass_pipeline::AotPassStage;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use std::collections::HashMap;

/// Ownership/rooting state for a value visible to AoT/native backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AotRuntimeValueClass {
    /// Plain backend-native scalar/aggregate/pointer with no runtime `Value` obligation.
    Native,
    /// Owned runtime `Value`; current generated Rust passes and stores these by value.
    RuntimeOwned,
    /// Borrowed runtime data that cannot survive an allocating helper unrooted.
    RuntimeBorrowed,
    /// Runtime data explicitly rooted across safepoints.
    Rooted,
    /// Temporary runtime value that must be consumed before the next safepoint.
    Temporary,
}

impl AotRuntimeValueClass {
    pub fn from_static_type(ty: &StaticType) -> Self {
        match AotAbiValue::from_static_type(ty).class() {
            AotAbiClass::RuntimeBoxed => AotRuntimeValueClass::RuntimeOwned,
            AotAbiClass::UnboxedScalar
            | AotAbiClass::NativeAggregate
            | AotAbiClass::NativePointer => AotRuntimeValueClass::Native,
        }
    }

    pub fn must_be_rooted_across_safepoint(self) -> bool {
        matches!(
            self,
            AotRuntimeValueClass::RuntimeBorrowed | AotRuntimeValueClass::Temporary
        )
    }

    pub fn satisfies_safepoint(self) -> bool {
        matches!(
            self,
            AotRuntimeValueClass::Native
                | AotRuntimeValueClass::RuntimeOwned
                | AotRuntimeValueClass::Rooted
        )
    }
}

/// Effect summary for generated runtime/native helper calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AotHelperEffect {
    /// Does not allocate and is not a safepoint for runtime `Value` liveness.
    NonAllocating,
    /// May allocate or call runtime dispatch, so borrowed runtime values must be rooted.
    AllocatingSafepoint,
    /// Effect is not modeled yet; native backends must treat it as a safepoint.
    UnknownSafepoint,
}

impl AotHelperEffect {
    pub fn is_safepoint(self) -> bool {
        matches!(
            self,
            AotHelperEffect::AllocatingSafepoint | AotHelperEffect::UnknownSafepoint
        )
    }
}

/// Classified helper call visible to the rooting verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotHelperCall {
    pub name: String,
    pub effect: AotHelperEffect,
}

/// One liveness obligation checked by the rooting verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotRootingObligation {
    pub function: String,
    pub value: String,
    pub class: AotRuntimeValueClass,
    pub helper: AotHelperCall,
}

/// Rooting facts collected for a function or test fixture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AotRootingPlan {
    obligations: Vec<AotRootingObligation>,
}

impl AotRootingPlan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_obligation(&mut self, obligation: AotRootingObligation) {
        self.obligations.push(obligation);
    }

    pub fn obligations(&self) -> &[AotRootingObligation] {
        &self.obligations
    }
}

/// True when the low-level Cranelift path would need runtime `Value`/managed pointer support.
///
/// Conservative: every non-scalar (heap/managed/aggregate or unknown) carrier
/// type needs the runtime `Value` model; only the natively-unboxed scalars
/// (integers, floats, `Bool`, `Char`, `Nothing`, `Missing`) are exempt. This
/// preserves the original `aot::JuliaType` set (`Str`/`Array`/`Tuple`/`Struct`/
/// `Any`) and additionally roots the heap-shaped `StaticType`-only carriers
/// (`Dict`/`Range`/`Function`/`Union`) — over-rooting is sound, under-rooting is
/// not (Issue #6598).
pub fn static_type_requires_rooting_model(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::Str
            | StaticType::Array { .. }
            | StaticType::Tuple(_)
            | StaticType::NamedTuple(_)
            | StaticType::Dict { .. }
            | StaticType::Range { .. }
            | StaticType::Generator { .. }
            | StaticType::Struct { .. }
            | StaticType::Function { .. }
            | StaticType::Union { .. }
            | StaticType::DataType
            | StaticType::Any
    )
}

/// Classify a high-level AoT expression as native/owned runtime/borrowed/rooted.
pub fn classify_expr_value(expr: &AotExpr) -> AotRuntimeValueClass {
    match expr {
        AotExpr::Box(_) | AotExpr::CallDynamic { .. } | AotExpr::BinOpDynamic { .. } => {
            AotRuntimeValueClass::RuntimeOwned
        }
        _ => AotRuntimeValueClass::from_static_type(&expr.get_type()),
    }
}

/// Classify helper-call effects that matter for runtime `Value` liveness.
pub fn classify_helper_call(expr: &AotExpr) -> Option<AotHelperCall> {
    match expr {
        AotExpr::CallDynamic { function, .. } => Some(AotHelperCall {
            name: format!("dynamic call `{}`", function),
            effect: AotHelperEffect::AllocatingSafepoint,
        }),
        AotExpr::BinOpDynamic { op, .. } => Some(AotHelperCall {
            name: format!("dynamic binary `{}`", op),
            effect: AotHelperEffect::AllocatingSafepoint,
        }),
        AotExpr::CallStatic {
            function,
            args,
            return_ty,
            ..
        } if AotAbiValue::from_static_type(return_ty).needs_runtime_value()
            || args.iter().any(|arg| {
                AotAbiValue::from_static_type(&arg.get_type()).needs_runtime_value()
            }) =>
        {
            Some(AotHelperCall {
                name: format!("static runtime-value call `{}`", function),
                effect: AotHelperEffect::UnknownSafepoint,
            })
        }
        AotExpr::CallBuiltin { builtin, .. } => Some(AotHelperCall {
            name: format!("builtin `{}`", builtin_name(*builtin)),
            effect: classify_builtin_effect(*builtin),
        }),
        _ => None,
    }
}

/// Verify rooting obligations derived from high-level AoT IR.
pub fn verify_aot_rooting_obligations(stage: AotPassStage, program: &AotProgram) -> AotResult<()> {
    let plan = collect_rooting_obligations(program);
    verify_rooting_plan(stage, &plan)
}

/// Collect conservative liveness obligations for all named functions and main.
pub fn collect_rooting_obligations(program: &AotProgram) -> AotRootingPlan {
    let mut plan = AotRootingPlan::new();
    for function in &program.functions {
        let mut env = HashMap::new();
        for (name, ty) in &function.params {
            env.insert(name.clone(), AotRuntimeValueClass::from_static_type(ty));
        }
        collect_stmt_obligations(&function.name, &function.body, &mut env, &mut plan);
    }

    let mut main_env = HashMap::new();
    collect_stmt_obligations("<main>", &program.main, &mut main_env, &mut plan);
    plan
}

/// Verify an explicit rooting plan.  This is intentionally public so tests and
/// future ABI-lowered IR can exercise unsafe borrowed-value cases directly.
pub fn verify_rooting_plan(stage: AotPassStage, plan: &AotRootingPlan) -> AotResult<()> {
    for obligation in plan.obligations() {
        if obligation.helper.effect.is_safepoint()
            && obligation.class.must_be_rooted_across_safepoint()
        {
            return Err(AotError::InvalidIR(format!(
                "{} rooting verifier failed in `{}`: borrowed runtime value `{}` is live across {} ({:?}) without ownership/rooting",
                stage,
                obligation.function,
                obligation.value,
                obligation.helper.name,
                obligation.helper.effect
            )));
        }
    }
    Ok(())
}

fn collect_stmt_obligations(
    function: &str,
    stmts: &[AotStmt],
    env: &mut HashMap<String, AotRuntimeValueClass>,
    plan: &mut AotRootingPlan,
) {
    for stmt in stmts {
        match stmt {
            AotStmt::Let {
                name, ty, value, ..
            } => {
                collect_expr_obligations(function, value, env, plan);
                let value_class = classify_expr_value(value);
                let declared_class = AotRuntimeValueClass::from_static_type(ty);
                env.insert(
                    name.clone(),
                    if value_class == AotRuntimeValueClass::Native {
                        declared_class
                    } else {
                        value_class
                    },
                );
            }
            AotStmt::Assign { target, value } => {
                collect_expr_obligations(function, value, env, plan);
                collect_expr_obligations(function, target, env, plan);
                if let AotExpr::Var { name, .. } = target {
                    env.insert(name.clone(), classify_expr_value(value));
                }
            }
            AotStmt::CompoundAssign { target, value, .. } => {
                collect_expr_obligations(function, target, env, plan);
                collect_expr_obligations(function, value, env, plan);
            }
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) | AotStmt::Return(Some(expr)) => {
                collect_expr_obligations(function, expr, env, plan);
            }
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_expr_obligations(function, condition, env, plan);
                let mut then_env = env.clone();
                collect_stmt_obligations(function, then_branch, &mut then_env, plan);
                if let Some(else_branch) = else_branch {
                    let mut else_env = env.clone();
                    collect_stmt_obligations(function, else_branch, &mut else_env, plan);
                }
            }
            AotStmt::While {
                condition, body, ..
            } => {
                collect_expr_obligations(function, condition, env, plan);
                let mut body_env = env.clone();
                collect_stmt_obligations(function, body, &mut body_env, plan);
            }
            AotStmt::ForRange {
                var,
                start,
                stop,
                step,
                body,
            } => {
                collect_expr_obligations(function, start, env, plan);
                collect_expr_obligations(function, stop, env, plan);
                if let Some(step) = step {
                    collect_expr_obligations(function, step, env, plan);
                }
                let mut body_env = env.clone();
                body_env.insert(var.clone(), AotRuntimeValueClass::Native);
                collect_stmt_obligations(function, body, &mut body_env, plan);
            }
            AotStmt::ForEach { var, iter, body } => {
                collect_expr_obligations(function, iter, env, plan);
                let mut body_env = env.clone();
                body_env.insert(var.clone(), classify_expr_value(iter));
                collect_stmt_obligations(function, body, &mut body_env, plan);
            }
            AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
        }
    }
}

fn collect_expr_obligations(
    function: &str,
    expr: &AotExpr,
    env: &HashMap<String, AotRuntimeValueClass>,
    plan: &mut AotRootingPlan,
) {
    if let Some(helper) = classify_helper_call(expr) {
        if helper.effect.is_safepoint() {
            let mut runtime_values: Vec<_> = env
                .iter()
                .filter(|(_, class)| **class != AotRuntimeValueClass::Native)
                .collect();
            runtime_values.sort_by_key(|(value, _)| *value);

            for (value, class) in runtime_values {
                plan.push_obligation(AotRootingObligation {
                    function: function.to_string(),
                    value: value.clone(),
                    class: *class,
                    helper: helper.clone(),
                });
            }
        }
    }

    match expr {
        AotExpr::BinOpStatic { left, right, .. } | AotExpr::BinOpDynamic { left, right, .. } => {
            collect_expr_obligations(function, left, env, plan);
            collect_expr_obligations(function, right, env, plan);
        }
        AotExpr::UnaryOp { operand, .. } => collect_expr_obligations(function, operand, env, plan),
        AotExpr::CallStatic { args, .. }
        | AotExpr::CallDynamic { args, .. }
        | AotExpr::CallBuiltin { args, .. } => {
            for arg in args {
                collect_expr_obligations(function, arg, env, plan);
            }
        }
        AotExpr::ArrayLit { elements, .. }
        | AotExpr::TupleLit { elements }
        | AotExpr::StructNew {
            fields: elements, ..
        } => {
            for elem in elements {
                collect_expr_obligations(function, elem, env, plan);
            }
        }
        AotExpr::SetFromIter { iter, .. } => {
            collect_expr_obligations(function, iter, env, plan);
        }
        AotExpr::NamedTupleLit { fields } => {
            for (_, field) in fields {
                collect_expr_obligations(function, field, env, plan);
            }
        }
        AotExpr::Comprehension {
            body, iter, filter, ..
        }
        | AotExpr::Generator {
            body, iter, filter, ..
        } => {
            collect_expr_obligations(function, iter, env, plan);
            if let Some(filter) = filter {
                collect_expr_obligations(function, filter, env, plan);
            }
            collect_expr_obligations(function, body, env, plan);
        }
        AotExpr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            for (_, iter) in iterations {
                collect_expr_obligations(function, iter, env, plan);
            }
            if let Some(filter) = filter {
                collect_expr_obligations(function, filter, env, plan);
            }
            collect_expr_obligations(function, body, env, plan);
        }
        AotExpr::Index { array, indices, .. } => {
            collect_expr_obligations(function, array, env, plan);
            for index in indices {
                collect_expr_obligations(function, index, env, plan);
            }
        }
        AotExpr::Range {
            start, stop, step, ..
        } => {
            collect_expr_obligations(function, start, env, plan);
            collect_expr_obligations(function, stop, env, plan);
            if let Some(step) = step {
                collect_expr_obligations(function, step, env, plan);
            }
        }
        AotExpr::FieldAccess { object, .. } => {
            collect_expr_obligations(function, object, env, plan);
        }
        AotExpr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_obligations(function, condition, env, plan);
            collect_expr_obligations(function, then_expr, env, plan);
            collect_expr_obligations(function, else_expr, env, plan);
        }
        AotExpr::Box(inner)
        | AotExpr::Unbox { value: inner, .. }
        | AotExpr::Convert { value: inner, .. } => {
            collect_expr_obligations(function, inner, env, plan);
        }
        AotExpr::Lambda { body, .. } => {
            let mut lambda_env = env.clone();
            collect_stmt_obligations(function, body, &mut lambda_env, plan);
        }
        AotExpr::LitI64(_)
        | AotExpr::LitI32(_)
        | AotExpr::LitF64(_)
        | AotExpr::LitF32(_)
        | AotExpr::LitBool(_)
        | AotExpr::LitStr(_)
        | AotExpr::LitChar(_)
        | AotExpr::LitNothing
        | AotExpr::LitMissing
        | AotExpr::Var { .. } => {}
    }
}

fn classify_builtin_effect(builtin: AotBuiltinOp) -> AotHelperEffect {
    match builtin {
        AotBuiltinOp::Push
        | AotBuiltinOp::PushFirst
        | AotBuiltinOp::Insert
        | AotBuiltinOp::Append
        | AotBuiltinOp::Dict
        | AotBuiltinOp::Collect
        | AotBuiltinOp::Zeros
        | AotBuiltinOp::Ones
        | AotBuiltinOp::Map
        | AotBuiltinOp::Filter
        | AotBuiltinOp::Reduce
        | AotBuiltinOp::StringConcat
        | AotBuiltinOp::Linspace => AotHelperEffect::AllocatingSafepoint,
        AotBuiltinOp::Println | AotBuiltinOp::Print | AotBuiltinOp::Rand | AotBuiltinOp::Randn => {
            AotHelperEffect::UnknownSafepoint
        }
        _ => AotHelperEffect::NonAllocating,
    }
}

fn builtin_name(builtin: AotBuiltinOp) -> &'static str {
    match builtin {
        AotBuiltinOp::Sqrt => "sqrt",
        AotBuiltinOp::Sin => "sin",
        AotBuiltinOp::Cos => "cos",
        AotBuiltinOp::Tan => "tan",
        AotBuiltinOp::Asin => "asin",
        AotBuiltinOp::Acos => "acos",
        AotBuiltinOp::Atan => "atan",
        AotBuiltinOp::Atan2 => "atan",
        AotBuiltinOp::Exp => "exp",
        AotBuiltinOp::Log => "log",
        AotBuiltinOp::Abs => "abs",
        AotBuiltinOp::Floor => "floor",
        AotBuiltinOp::Ceil => "ceil",
        AotBuiltinOp::Round => "round",
        AotBuiltinOp::Trunc => "trunc",
        AotBuiltinOp::Min => "min",
        AotBuiltinOp::Max => "max",
        AotBuiltinOp::IsLess => "isless",
        AotBuiltinOp::Clamp => "clamp",
        AotBuiltinOp::Sign => "sign",
        AotBuiltinOp::Signbit => "signbit",
        AotBuiltinOp::Copysign => "copysign",
        AotBuiltinOp::Div => "div",
        AotBuiltinOp::Mod => "mod",
        AotBuiltinOp::Rem => "rem",
        AotBuiltinOp::Fld => "fld",
        AotBuiltinOp::Cld => "cld",
        AotBuiltinOp::Isnan => "isnan",
        AotBuiltinOp::Isinf => "isinf",
        AotBuiltinOp::Isfinite => "isfinite",
        AotBuiltinOp::Length => "length",
        AotBuiltinOp::Size => "size",
        AotBuiltinOp::Ndims => "ndims",
        AotBuiltinOp::Push => "push!",
        AotBuiltinOp::Pop => "pop!",
        AotBuiltinOp::PushFirst => "pushfirst!",
        AotBuiltinOp::PopFirst => "popfirst!",
        AotBuiltinOp::Insert => "insert!",
        AotBuiltinOp::DeleteAt => "deleteat!",
        AotBuiltinOp::Append => "append!",
        AotBuiltinOp::First => "first",
        AotBuiltinOp::Last => "last",
        AotBuiltinOp::TupleFirst => "first",
        AotBuiltinOp::TupleLast => "last",
        AotBuiltinOp::IsEmpty => "isempty",
        AotBuiltinOp::In => "in",
        AotBuiltinOp::Dict => "Dict",
        AotBuiltinOp::HasKey => "haskey",
        AotBuiltinOp::DictGet => "get",
        AotBuiltinOp::Collect => "collect",
        AotBuiltinOp::Zeros => "zeros",
        AotBuiltinOp::Ones => "ones",
        AotBuiltinOp::Reshape => "reshape",
        AotBuiltinOp::Sum => "sum",
        AotBuiltinOp::Map => "map",
        AotBuiltinOp::Filter => "filter",
        AotBuiltinOp::Reduce => "reduce",
        AotBuiltinOp::MapReduce => "mapreduce",
        AotBuiltinOp::ForEach => "foreach",
        AotBuiltinOp::Any => "any",
        AotBuiltinOp::All => "all",
        AotBuiltinOp::StringLength => "length",
        AotBuiltinOp::Uppercase => "uppercase",
        AotBuiltinOp::Lowercase => "lowercase",
        AotBuiltinOp::Occursin => "occursin",
        AotBuiltinOp::StartsWith => "startswith",
        AotBuiltinOp::EndsWith => "endswith",
        AotBuiltinOp::Println => "println",
        AotBuiltinOp::Print => "print",
        AotBuiltinOp::TimeNs => "time_ns",
        AotBuiltinOp::TypeOf => "typeof",
        AotBuiltinOp::Isa => "isa",
        AotBuiltinOp::Rand => "rand",
        AotBuiltinOp::Randn => "randn",
        AotBuiltinOp::Sitofp => "sitofp",
        AotBuiltinOp::Fptosi => "fptosi",
        AotBuiltinOp::Throw => "throw",
        AotBuiltinOp::StringConcat => "string",
        AotBuiltinOp::Abs2 => "abs2",
        AotBuiltinOp::Real => "real",
        AotBuiltinOp::Imag => "imag",
        AotBuiltinOp::Adjoint => "adjoint",
        AotBuiltinOp::Linspace => "linspace",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_static_type_is_runtime_owned_in_current_rust_backend() {
        assert_eq!(
            AotRuntimeValueClass::from_static_type(&StaticType::Any),
            AotRuntimeValueClass::RuntimeOwned
        );
    }

    #[test]
    fn heap_shaped_static_types_require_rooting_model_issue_6989() {
        let heap_like_types = vec![
            StaticType::Str,
            StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            },
            StaticType::Tuple(vec![StaticType::I64, StaticType::Str]),
            StaticType::Dict {
                key: Box::new(StaticType::Str),
                value: Box::new(StaticType::I64),
            },
            StaticType::Range {
                element: Box::new(StaticType::I64),
            },
            StaticType::Struct {
                type_id: 1,
                name: "Point".to_string(),
            },
            StaticType::Function {
                params: vec![StaticType::I64],
                ret: Box::new(StaticType::I64),
            },
            StaticType::Union {
                variants: vec![StaticType::I64, StaticType::Str],
            },
            StaticType::Any,
        ];

        for ty in heap_like_types {
            assert!(
                static_type_requires_rooting_model(&ty),
                "{:?} should require the conservative rooting model",
                ty
            );
        }

        for ty in [
            StaticType::I64,
            StaticType::F64,
            StaticType::Bool,
            StaticType::Char,
            StaticType::Nothing,
            StaticType::Missing,
        ] {
            assert!(
                !static_type_requires_rooting_model(&ty),
                "{:?} should stay native and avoid over-rooting",
                ty
            );
        }
    }

    #[test]
    fn dynamic_call_is_allocating_safepoint() {
        let expr = AotExpr::CallDynamic {
            function: "f".to_string(),
            args: vec![AotExpr::LitI64(1)],
        };

        let helper = classify_helper_call(&expr).unwrap();
        assert_eq!(helper.effect, AotHelperEffect::AllocatingSafepoint);
    }

    #[test]
    fn verifier_rejects_borrowed_runtime_value_across_safepoint() {
        let mut plan = AotRootingPlan::new();
        plan.push_obligation(AotRootingObligation {
            function: "f".to_string(),
            value: "x".to_string(),
            class: AotRuntimeValueClass::RuntimeBorrowed,
            helper: AotHelperCall {
                name: "dynamic call `g`".to_string(),
                effect: AotHelperEffect::AllocatingSafepoint,
            },
        });

        let err = verify_rooting_plan(AotPassStage::BeforeBackendCodegen, &plan).unwrap_err();
        assert!(err.to_string().contains("borrowed runtime value `x`"));
    }

    #[test]
    fn verifier_accepts_owned_runtime_value_across_safepoint() {
        let mut plan = AotRootingPlan::new();
        plan.push_obligation(AotRootingObligation {
            function: "f".to_string(),
            value: "x".to_string(),
            class: AotRuntimeValueClass::RuntimeOwned,
            helper: AotHelperCall {
                name: "dynamic call `g`".to_string(),
                effect: AotHelperEffect::AllocatingSafepoint,
            },
        });

        verify_rooting_plan(AotPassStage::BeforeBackendCodegen, &plan).unwrap();
    }

    #[test]
    fn conservative_liveness_cost_tracks_runtime_values_not_native_scalars_issue_6989() {
        let mut program = AotProgram::new();
        for index in 0..64 {
            program.main.push(AotStmt::Let {
                name: format!("n{}", index),
                ty: StaticType::I64,
                value: AotExpr::LitI64(index),
                is_mutable: false,
            });
        }
        program.main.push(AotStmt::Let {
            name: "x".to_string(),
            ty: StaticType::Any,
            value: AotExpr::CallDynamic {
                function: "make_x".to_string(),
                args: vec![],
            },
            is_mutable: false,
        });
        program.main.push(AotStmt::Let {
            name: "y".to_string(),
            ty: StaticType::Any,
            value: AotExpr::CallDynamic {
                function: "make_y".to_string(),
                args: vec![],
            },
            is_mutable: false,
        });
        program.main.push(AotStmt::Expr(AotExpr::CallDynamic {
            function: "use_values".to_string(),
            args: vec![
                AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::Any,
                },
                AotExpr::Var {
                    name: "y".to_string(),
                    ty: StaticType::Any,
                },
            ],
        }));

        let plan = collect_rooting_obligations(&program);
        let values: Vec<_> = plan
            .obligations()
            .iter()
            .map(|obligation| obligation.value.as_str())
            .collect();

        assert_eq!(plan.obligations().len(), 3);
        assert_eq!(values, vec!["x", "x", "y"]);
        assert!(plan
            .obligations()
            .iter()
            .all(|obligation| obligation.class == AotRuntimeValueClass::RuntimeOwned));
        verify_rooting_plan(AotPassStage::BeforeBackendCodegen, &plan).unwrap();
    }

    #[test]
    fn collecting_program_obligations_keeps_current_owned_value_contract() {
        let mut program = AotProgram::new();
        program.main.push(AotStmt::Let {
            name: "x".to_string(),
            ty: StaticType::Any,
            value: AotExpr::CallDynamic {
                function: "make_value".to_string(),
                args: vec![],
            },
            is_mutable: false,
        });
        program.main.push(AotStmt::Expr(AotExpr::CallDynamic {
            function: "use_value".to_string(),
            args: vec![AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::Any,
            }],
        }));

        let plan = collect_rooting_obligations(&program);
        assert_eq!(plan.obligations().len(), 1);
        assert_eq!(
            plan.obligations()[0].class,
            AotRuntimeValueClass::RuntimeOwned
        );
        verify_rooting_plan(AotPassStage::BeforeBackendCodegen, &plan).unwrap();
    }
}
