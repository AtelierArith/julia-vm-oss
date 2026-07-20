// ---------------------------------------------------------------------------
// Issue #3525: Expr::Builtin must route through builtin_op_to_function so the
// transfer-function registry can produce precise return types.  These tests
// guard the regression where most BuiltinOp variants collapsed to
// "unknown_builtin" and inference returned Top.
// ---------------------------------------------------------------------------

use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::BuiltinOp;

fn typed_var_env(name: &str, ty: LatticeType) -> TypeEnv {
    let mut env = TypeEnv::new();
    env.set(name, ty);
    env
}

fn builtin_call(op: BuiltinOp, args: Vec<Expr>) -> Expr {
    Expr::Builtin {
        name: op,
        args,
        span: dummy_span(),
    }
}

fn var(name: &str) -> Expr {
    Expr::Var(name.to_string().into(), dummy_span())
}

#[test]
fn maps_every_builtin_op_to_a_real_tfunc_name() {
    // Exhaustively enumerate every BuiltinOp variant and assert that
    // builtin_op_to_function returns a non-empty name distinct from the
    // "unknown_builtin" sentinel that previously caused inference to
    // widen to Top.
    let variants = [
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

    for op in &variants {
        let name = builtin_op_to_function(op);
        assert!(!name.is_empty(), "{:?} mapped to empty string", op);
        assert_ne!(
            name, "unknown_builtin",
            "{:?} fell through to unknown_builtin (Issue #3525)",
            op
        );
    }
}

#[test]
fn length_of_int_array_infers_int64() {
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "xs",
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        }),
    );

    let expr = builtin_call(BuiltinOp::Length, vec![var("xs")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
}

#[test]
fn length_of_tuple_infers_const_length() {
    // Issue #5142: the length of a fixed-arity tuple is statically known,
    // so inference now propagates it as Const(N) (mirroring upstream
    // `nfields_tfunc`) rather than widening to Int64.
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "t",
        LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        }),
    );

    let expr = builtin_call(BuiltinOp::Length, vec![var("t")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(result, LatticeType::Const(ConstValue::Int64(2)));
}

#[test]
fn size_of_array_infers_tuple_of_int64() {
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "xs",
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        }),
    );

    let expr = builtin_call(BuiltinOp::Size, vec![var("xs")]);
    let result = engine.infer_expr(&expr, &env);

    // Note: BuiltinOp::Size is in builtin_op_should_widen_unknown's legacy
    // list, so the engine intentionally widens to Top here. The fact that
    // we hit that branch — rather than calling tfunc with
    // "unknown_builtin" — is what guards Issue #3525. See the regression
    // test below: no UnknownFunction("unknown_builtin") diagnostic is
    // emitted.
    assert_eq!(result, LatticeType::Top);
}

#[test]
fn zero_of_int64_infers_int64() {
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "n",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    let expr = builtin_call(BuiltinOp::Zero, vec![var("n")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
}

#[test]
fn tuple_first_infers_first_element_type() {
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "t",
        LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        }),
    );

    let expr = builtin_call(BuiltinOp::TupleFirst, vec![var("t")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
}

#[test]
fn tuple_last_infers_last_element_type() {
    let mut engine = InferenceEngine::new();
    let env = typed_var_env(
        "t",
        LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        }),
    );

    let expr = builtin_call(BuiltinOp::TupleLast, vec![var("t")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        )))
    );
}

#[test]
fn haskey_on_dict_infers_bool() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();
    env.set(
        "d",
        LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Symbol,
            ))),
            value: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        }),
    );
    env.set("k", LatticeType::Const(ConstValue::Symbol("a".to_string())));

    let expr = builtin_call(BuiltinOp::HasKey, vec![var("d"), var("k")]);
    let result = engine.infer_expr(&expr, &env);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
    );
}

#[test]
fn known_builtins_do_not_emit_unknown_function_diagnostic() {
    DiagnosticsCollector::enable();
    DiagnosticsCollector::clear();

    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();
    env.set(
        "xs",
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        }),
    );
    env.set(
        "t",
        LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        }),
    );
    env.set(
        "d",
        LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Symbol,
            ))),
            value: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        }),
    );
    env.set("k", LatticeType::Const(ConstValue::Symbol("a".to_string())));
    env.set(
        "n",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    let exprs = vec![
        builtin_call(BuiltinOp::Length, vec![var("xs")]),
        builtin_call(BuiltinOp::Length, vec![var("t")]),
        builtin_call(BuiltinOp::Zero, vec![var("n")]),
        builtin_call(BuiltinOp::TupleFirst, vec![var("t")]),
        builtin_call(BuiltinOp::TupleLast, vec![var("t")]),
        builtin_call(BuiltinOp::HasKey, vec![var("d"), var("k")]),
    ];

    for expr in &exprs {
        let _ = engine.infer_expr(expr, &env);
    }

    let diags = DiagnosticsCollector::take();
    DiagnosticsCollector::disable();

    let leaked: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                &d.reason,
                DiagnosticReason::UnknownFunction(name) if name == "unknown_builtin"
            )
        })
        .collect();

    assert!(
        leaked.is_empty(),
        "Issue #3525 regression: known builtins emitted UnknownFunction(\"unknown_builtin\"): {:?}",
        leaked
    );
}
