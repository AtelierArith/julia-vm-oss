use super::*;
use crate::aot::ir::AotBinOp;
use crate::aot::specialization::CodeInstanceKey;
use crate::aot::types::StaticType;
use crate::ir::core::{
    BinaryOp, Block, Expr, Function, Literal, Program, Stmt, StructDef, StructField, TypedParam,
    UnaryOp,
};
use crate::types::{JuliaType, TypeExpr};

#[test]
fn test_inference_result_new() {
    let result = InferenceResult::new();
    assert!(result.types.is_empty());
    assert!(result.is_fully_typed);
    assert!(result.needs_guard.is_empty());
}

#[test]
fn test_inference_result_bind() {
    let mut result = InferenceResult::new();
    result.bind("x".to_string(), StaticType::I64);
    assert_eq!(result.get_type("x"), Some(&StaticType::I64));
    assert!(result.is_fully_typed);

    result.bind("y".to_string(), StaticType::Any);
    assert!(!result.is_fully_typed);
}

#[test]
fn test_function_signature_level() {
    // Level 1: Fully static
    let sig = FunctionSignature::new(
        "f".to_string(),
        vec!["x".to_string()],
        vec![StaticType::I64],
        StaticType::I64,
    );
    assert_eq!(sig.inference_level, 1);
    assert!(sig.is_fully_static());

    // Level 4: Dynamic
    let sig = FunctionSignature::new(
        "g".to_string(),
        vec!["x".to_string()],
        vec![StaticType::Any],
        StaticType::Any,
    );
    assert_eq!(sig.inference_level, 4);
    assert!(!sig.is_fully_static());
}

#[test]
fn test_struct_type_info() {
    let mut info = StructTypeInfo::new("Point".to_string(), false);
    info.add_field("x".to_string(), StaticType::F64);
    info.add_field("y".to_string(), StaticType::F64);

    assert_eq!(info.get_field_type("x"), Some(&StaticType::F64));
    assert_eq!(info.get_field_type("z"), None);
}

#[test]
fn test_analyze_struct_type_expr_uses_static_projection_issue_5916() {
    let engine = TypeInferenceEngine::new();
    let span = test_span();
    let struct_def = StructDef {
        global_new_helpers: Vec::new(),
        name: "AotTypeExprProbe5916".to_string(),
        is_mutable: false,
        is_base_origin: false,
        type_params: vec![],
        parent_type: None,
        fields: vec![
            StructField {
                name: "matrix".to_string(),
                type_expr: Some(TypeExpr::Parameterized {
                    base: "Array".to_string(),
                    params: vec![
                        TypeExpr::Concrete(JuliaType::Int64),
                        TypeExpr::TypeVar("2".to_string()),
                    ],
                }),
                span,
            },
            StructField {
                name: "pair".to_string(),
                type_expr: Some(TypeExpr::Parameterized {
                    base: "Tuple".to_string(),
                    params: vec![
                        TypeExpr::Concrete(JuliaType::Int64),
                        TypeExpr::Concrete(JuliaType::String),
                    ],
                }),
                span,
            },
            StructField {
                name: "abstract_value".to_string(),
                type_expr: Some(TypeExpr::Concrete(JuliaType::Real)),
                span,
            },
        ],
        inner_constructors: vec![],
        span,
    };

    let info = engine
        .analyze_struct(&struct_def)
        .expect("analyze struct fields");
    assert_eq!(
        info.get_field_type("matrix"),
        Some(&StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(2),
        })
    );
    assert_eq!(
        info.get_field_type("pair"),
        Some(&StaticType::Tuple(vec![StaticType::I64, StaticType::Str]))
    );
    assert_eq!(
        info.get_field_type("abstract_value"),
        Some(&StaticType::Any)
    );
}

#[test]
fn test_typed_program() {
    let mut program = TypedProgram::new();

    let sig = FunctionSignature::new(
        "add".to_string(),
        vec!["a".to_string(), "b".to_string()],
        vec![StaticType::I64, StaticType::I64],
        StaticType::I64,
    );
    let typed_func = TypedFunction::new(sig);
    program.add_function(typed_func);

    assert!(program.get_functions("add").is_some());
    assert_eq!(program.inference_level, 1);
}

#[test]
fn test_engine_new() {
    let engine = TypeInferenceEngine::new();
    // Should have builtins registered
    assert!(engine.builtins.contains_key("sqrt"));
    assert!(engine.builtins.contains_key("println"));
}

#[test]
fn test_join_types() {
    let engine = TypeInferenceEngine::new();

    // Same type returns itself
    assert_eq!(
        engine.join_types(&StaticType::I64, &StaticType::I64),
        StaticType::I64
    );

    // Numeric types are promoted (not unioned)
    let joined = engine.join_types(&StaticType::I64, &StaticType::F64);
    assert_eq!(joined, StaticType::F64);

    // Any is the top element (absorbing): join(Any, T) = Any (Issue #3461)
    assert_eq!(
        engine.join_types(&StaticType::Any, &StaticType::I64),
        StaticType::Any
    );
    assert_eq!(
        engine.join_types(&StaticType::I64, &StaticType::Any),
        StaticType::Any
    );

    // Julia's nominal typejoin(String, Bool) is Any. The shared CoreType
    // lattice owns that decision; AoT projects the result dynamically.
    assert_eq!(
        engine.join_types(&StaticType::Str, &StaticType::Bool),
        StaticType::Any
    );
}

#[test]
fn test_meet_types() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(
        engine.meet_types(&StaticType::I64, &StaticType::I64),
        StaticType::I64
    );

    assert_eq!(
        engine.meet_types(&StaticType::Any, &StaticType::I64),
        StaticType::I64
    );
}

#[test]
fn test_issue_3912_meet_types_routes_through_core_typeintersect() {
    let engine = TypeInferenceEngine::new();

    // typeintersect(Union{Int64, Float64}, Int64) == Int64 in upstream Julia.
    // Previously AoT over-widened this to `Any`.
    assert_eq!(
        engine.meet_types(
            &StaticType::Union {
                variants: vec![StaticType::I64, StaticType::F64],
            },
            &StaticType::I64,
        ),
        StaticType::I64
    );

    // typeintersect(Int64, Float64) == Union{} (Bottom) in upstream Julia.
    // The shared core proves disjointness; AoT projects Bottom to the empty
    // union instead of silently widening to a misleading backend type.
    assert_eq!(
        engine.meet_types(&StaticType::I64, &StaticType::F64),
        StaticType::Union { variants: vec![] }
    );

    // typeintersect(Tuple{Int64, Float64}, Tuple{Int64, Float64}) == itself.
    let tuple = StaticType::Tuple(vec![StaticType::I64, StaticType::F64]);
    assert_eq!(engine.meet_types(&tuple, &tuple), tuple.clone());

    // `Any` stays absorbing-as-identity on meet: meet(Any, T) == T.
    assert_eq!(
        engine.meet_types(&StaticType::Any, &StaticType::Str),
        StaticType::Str
    );
    assert_eq!(
        engine.meet_types(&StaticType::Str, &StaticType::Any),
        StaticType::Str
    );
}

#[test]
fn test_literal_type() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(engine.literal_type(&Literal::Int(42)), StaticType::I64);
    assert_eq!(engine.literal_type(&Literal::Float(1.25)), StaticType::F64);
    assert_eq!(engine.literal_type(&Literal::Bool(true)), StaticType::Bool);
    assert_eq!(
        engine.literal_type(&Literal::Str("hello".to_string())),
        StaticType::Str
    );
    assert_eq!(
        engine.literal_type(&Literal::Struct(
            "Complex{Bool}".to_string(),
            vec![Literal::Bool(false), Literal::Bool(true)]
        )),
        StaticType::Struct {
            type_id: 0,
            name: "Complex".to_string()
        }
    );
}

#[test]
fn test_issue_7761_datatype_literal_infers_any() {
    // Regression for Issue #7761: `literal_type` must cover `Literal::DataType`
    // (produced by macro expansion). Without the arm, the `--features aot` crate
    // fails to build with E0004 (non-exhaustive patterns). The DataType literal
    // maps to `StaticType::Any`, mirroring the `Module`/`Symbol`/`Expr` arms.
    let engine = TypeInferenceEngine::new();

    assert_eq!(
        engine.literal_type(&Literal::DataType("Int64".to_string())),
        StaticType::Any
    );
    assert_eq!(
        engine.literal_type(&Literal::Module("Base".to_string())),
        StaticType::Any
    );
}

#[test]
fn test_issue_3715_literals_preserve_wide_primitive_static_types() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(engine.literal_type(&Literal::Int128(42)), StaticType::I128);
    assert_eq!(
        engine.literal_type(&Literal::Float16(half::f16::from_f32(1.5))),
        StaticType::F16
    );
}

#[test]
fn test_numeric_promote_preserves_struct_with_numeric() {
    let engine = TypeInferenceEngine::new();
    let complex = StaticType::Struct {
        type_id: 0,
        name: "Complex".to_string(),
    };
    assert_eq!(
        engine.numeric_promote(&complex, &StaticType::F64),
        StaticType::Struct {
            type_id: 0,
            name: "Complex{Float64}".to_string(),
        }
    );
}

#[test]
fn test_binop_result_type() {
    let engine = TypeInferenceEngine::new();

    // Comparison returns Bool
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Eq, &StaticType::I64, &StaticType::I64),
        StaticType::Bool
    );

    // Division returns F64
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::I64, &StaticType::I64),
        StaticType::F64
    );

    // Numeric promotion
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::I64, &StaticType::F64),
        StaticType::F64
    );
}

#[test]
fn float32_type_preservation_issue_6941() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(
        engine.numeric_promote(&StaticType::F32, &StaticType::F32),
        StaticType::F32
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::F32, &StaticType::I64),
        StaticType::F32
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::I64, &StaticType::F32),
        StaticType::F32
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::F32, &StaticType::F64),
        StaticType::F64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::F32, &StaticType::I64),
        StaticType::F32
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::F32, &StaticType::F32),
        StaticType::F32
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::F32, &StaticType::F64),
        StaticType::F64
    );
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Add, &StaticType::F32, &StaticType::I64),
        StaticType::F32
    );
}

#[test]
fn test_bare_complex_numeric_promotion_preserves_concrete_param_issue_8795() {
    let engine = TypeInferenceEngine::new();
    let complex = StaticType::Struct {
        type_id: 0,
        name: "Complex".to_string(),
    };
    let complexf64 = StaticType::Struct {
        type_id: 0,
        name: "Complex{Float64}".to_string(),
    };

    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Mul, &StaticType::F64, &complex),
        complexf64
    );
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Add, &complex, &StaticType::F64),
        StaticType::Struct {
            type_id: 0,
            name: "Complex{Float64}".to_string(),
        }
    );
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Add, &complexf64, &complex),
        StaticType::Struct {
            type_id: 0,
            name: "Complex{Float64}".to_string(),
        }
    );
}

#[test]
fn test_call_result_type() {
    let engine = TypeInferenceEngine::new();

    // Known builtin
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::F64]),
        StaticType::F64
    );
    assert_eq!(engine.call_result_type("time_ns", &[]), StaticType::I64);

    // Type constructor
    assert_eq!(
        engine.call_result_type("Int64", &[StaticType::Any]),
        StaticType::I64
    );
}

#[test]
fn test_element_type() {
    let engine = TypeInferenceEngine::new();

    let arr = StaticType::Array {
        element: Box::new(StaticType::I64),
        ndims: Some(1),
    };
    assert_eq!(engine.element_type(&arr), StaticType::I64);

    let set = StaticType::Set {
        element: Box::new(StaticType::I64),
    };
    assert_eq!(engine.element_type(&set), StaticType::I64);

    assert_eq!(engine.element_type(&StaticType::Str), StaticType::Char);
}

// ========== Issue #999 Acceptance Criteria Tests ==========

/// Helper to create a span for test expressions
fn test_span() -> crate::span::Span {
    crate::span::Span::new(0, 0, 1, 1, 0, 0)
}

#[test]
fn test_infer_expr_binary_int_add() {
    // Acceptance: 1 + 2 → I64
    let engine = TypeInferenceEngine::new();
    let expr = Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        right: Box::new(Expr::Literal(Literal::Int(2), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::I64);
}

#[test]
fn test_infer_expr_binary_float_promotion() {
    // Acceptance: 1.0 + 2 → F64
    let engine = TypeInferenceEngine::new();
    let expr = Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(Literal::Float(1.0), test_span())),
        right: Box::new(Expr::Literal(Literal::Int(2), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::F64);
}

#[test]
fn test_infer_expr_comparison() {
    // Acceptance: x > 0 → Bool
    let mut engine = TypeInferenceEngine::new();
    // Add x to the environment as Int64
    engine.env.insert("x".to_string(), StaticType::I64);

    let expr = Expr::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expr::Var("x".to_string().into(), test_span())),
        right: Box::new(Expr::Literal(Literal::Int(0), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::Bool);
}

#[test]
fn test_infer_expr_array_literal() {
    // Acceptance: [1, 2, 3] → Array { element: I64 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::ArrayLiteral {
        elements: vec![
            Expr::Literal(Literal::Int(1), test_span()),
            Expr::Literal(Literal::Int(2), test_span()),
            Expr::Literal(Literal::Int(3), test_span()),
        ],
        shape: vec![3],
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::I64);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_infer_expr_tuple_literal() {
    // Acceptance: (1, "hello") → Tuple { elements: [I64, Str] }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TupleLiteral {
        elements: vec![
            Expr::Literal(Literal::Int(1), test_span()),
            Expr::Literal(Literal::Str("hello".to_string()), test_span()),
        ],
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Tuple(_)),
        "Expected Tuple type, got {:?}",
        ty
    );
    if let StaticType::Tuple(elements) = ty {
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], StaticType::I64);
        assert_eq!(elements[1], StaticType::Str);
    }
}

#[test]
fn test_infer_expr_ternary() {
    // Test: condition ? 1 : 2.0 → Union or promoted type
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("cond".to_string(), StaticType::Bool);

    let expr = Expr::Ternary {
        condition: Box::new(Expr::Var("cond".to_string().into(), test_span())),
        then_expr: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        else_expr: Box::new(Expr::Literal(Literal::Float(2.0), test_span())),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    // Either Union or the join of I64 and F64
    if let StaticType::Union { variants } = ty {
        assert!(variants.contains(&StaticType::I64));
        assert!(variants.contains(&StaticType::F64));
    } // Other join strategies are acceptable
}

#[test]
fn test_infer_expr_unary_neg() {
    // Test: -x preserves type
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("x".to_string(), StaticType::I64);

    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(Expr::Var("x".to_string().into(), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::I64);
}

#[test]
fn test_infer_expr_unary_not() {
    // Test: !x → Bool
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("x".to_string(), StaticType::Bool);

    let expr = Expr::UnaryOp {
        op: UnaryOp::Not,
        operand: Box::new(Expr::Var("x".to_string().into(), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::Bool);
}

#[test]
fn test_infer_expr_range() {
    // Test: 1:10 → Range { element: I64 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        stop: Box::new(Expr::Literal(Literal::Int(10), test_span())),
        step: None,
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Range { .. }),
        "Expected Range type, got {:?}",
        ty
    );
    if let StaticType::Range { element } = ty {
        assert_eq!(*element, StaticType::I64);
    }
}

#[test]
fn range_inference_uses_step_type_issue_6969() {
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        stop: Box::new(Expr::Literal(Literal::Int(2), test_span())),
        step: Some(Box::new(Expr::Literal(Literal::Float32(0.5), test_span()))),
        span: test_span(),
    };

    let ty = engine.infer_expr_type(&expr);
    assert_eq!(
        ty,
        StaticType::Range {
            element: Box::new(StaticType::F32)
        }
    );
}

#[test]
fn test_infer_expr_index() {
    // Test: arr[1] where arr is Array{I64} → I64
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert(
        "arr".to_string(),
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        },
    );

    let expr = Expr::Index {
        array: Box::new(Expr::Var("arr".to_string().into(), test_span())),
        indices: vec![Expr::Literal(Literal::Int(1), test_span())],
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::I64);
}

#[test]
fn test_infer_expr_call_builtin() {
    // Test: sqrt(4.0) → F64
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Call {
        function: "sqrt".to_string().into(),
        args: vec![Expr::Literal(Literal::Float(4.0), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::F64);
}

#[test]
fn typeof_infers_datatype_issue_6973() {
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Call {
        function: "typeof".to_string().into(),
        args: vec![Expr::Literal(Literal::Int(1), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::DataType);
}

#[test]
fn zeros_ones_type_argument_sets_element_type_issue_7069() {
    let engine = TypeInferenceEngine::new();

    let zeros_expr = Expr::Call {
        function: "zeros".to_string().into(),
        args: vec![
            Expr::Var("Int64".to_string().into(), test_span()),
            Expr::Literal(Literal::Int(3), test_span()),
        ],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    assert_eq!(
        engine.infer_expr_type(&zeros_expr),
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1)
        }
    );

    let ones_expr = Expr::Call {
        function: "ones".to_string().into(),
        args: vec![
            Expr::Var("Bool".to_string().into(), test_span()),
            Expr::Literal(Literal::Int(2), test_span()),
            Expr::Literal(Literal::Int(3), test_span()),
        ],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    assert_eq!(
        engine.infer_expr_type(&ones_expr),
        StaticType::Array {
            element: Box::new(StaticType::Bool),
            ndims: Some(2)
        }
    );
}

#[test]
fn test_infer_expr_call_with_kwargs_dispatches_by_all_args() {
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Call {
        function: "range".to_string().into(),
        args: vec![
            Expr::Literal(Literal::Float(-2.0), test_span()),
            Expr::Literal(Literal::Float(1.0), test_span()),
        ],
        kwargs: vec![(
            "length".to_string().into(),
            Expr::Literal(Literal::Int(50), test_span()),
        )],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![false],
        span: test_span(),
    };

    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array<Float64,1>, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::F64);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_infer_expr_convert_any() {
    // Test: convert(Any, sqrt(4.0)) → F64 (not Any)
    // This is important because lowering wraps return values in convert(Any, value)
    let engine = TypeInferenceEngine::new();
    let inner_expr = Expr::Call {
        function: "sqrt".to_string().into(),
        args: vec![Expr::Literal(Literal::Float(4.0), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    let expr = Expr::Call {
        function: "convert".to_string().into(),
        args: vec![Expr::Var("Any".to_string().into(), test_span()), inner_expr],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    // convert(Any, sqrt(4.0)) should return F64, not Any
    assert_eq!(engine.infer_expr_type(&expr), StaticType::F64);
}

#[test]
fn test_infer_expr_logical_and() {
    // Test: true && false → Bool
    let engine = TypeInferenceEngine::new();
    let expr = Expr::BinaryOp {
        op: BinaryOp::And,
        left: Box::new(Expr::Literal(Literal::Bool(true), test_span())),
        right: Box::new(Expr::Literal(Literal::Bool(false), test_span())),
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::Bool);
}

// ========== Helper Method Tests ==========

#[test]
fn test_lookup_global_or_const() {
    let engine = TypeInferenceEngine::new();

    // Math constants
    assert_eq!(engine.lookup_global_or_const("pi"), StaticType::F64);
    assert_eq!(engine.lookup_global_or_const("π"), StaticType::F64);
    assert_eq!(engine.lookup_global_or_const("Inf"), StaticType::F64);
    assert_eq!(engine.lookup_global_or_const("NaN"), StaticType::F64);

    // Boolean constants
    assert_eq!(engine.lookup_global_or_const("true"), StaticType::Bool);
    assert_eq!(engine.lookup_global_or_const("false"), StaticType::Bool);

    // Special values
    assert_eq!(
        engine.lookup_global_or_const("nothing"),
        StaticType::Nothing
    );
    assert_eq!(
        engine.lookup_global_or_const("missing"),
        StaticType::Missing
    );

    // Unknown
    assert_eq!(
        engine.lookup_global_or_const("unknown_var"),
        StaticType::Any
    );
}

#[test]
fn test_infer_iterator_element_type_array() {
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert(
        "arr".to_string(),
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        },
    );

    let expr = Expr::Var("arr".to_string().into(), test_span());
    assert_eq!(engine.infer_iterator_element_type(&expr), StaticType::I64);
}

#[test]
fn test_infer_iterator_element_type_range() {
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        stop: Box::new(Expr::Literal(Literal::Int(10), test_span())),
        step: None,
        span: test_span(),
    };
    assert_eq!(engine.infer_iterator_element_type(&expr), StaticType::I64);
}

#[test]
fn test_infer_iterator_element_type_string() {
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("s".to_string(), StaticType::Str);

    let expr = Expr::Var("s".to_string().into(), test_span());
    assert_eq!(engine.infer_iterator_element_type(&expr), StaticType::Char);
}

#[test]
fn test_literal_to_static() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(engine.literal_to_static(&Literal::Int(42)), StaticType::I64);
    assert_eq!(
        engine.literal_to_static(&Literal::Float(1.25)),
        StaticType::F64
    );
    assert_eq!(
        engine.literal_to_static(&Literal::Bool(true)),
        StaticType::Bool
    );
    assert_eq!(
        engine.literal_to_static(&Literal::Str("test".to_string())),
        StaticType::Str
    );
    assert_eq!(
        engine.literal_to_static(&Literal::Char('x')),
        StaticType::Char
    );
    assert_eq!(
        engine.literal_to_static(&Literal::Nothing),
        StaticType::Nothing
    );
}

#[test]
fn test_unify_types() {
    let engine = TypeInferenceEngine::new();

    // Same type
    assert_eq!(
        engine.unify_types(&StaticType::I64, &StaticType::I64),
        StaticType::I64
    );

    // Numeric promotion
    assert_eq!(
        engine.unify_types(&StaticType::I64, &StaticType::F64),
        StaticType::F64
    );
    assert_eq!(
        engine.unify_types(&StaticType::I32, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.unify_types(&StaticType::F32, &StaticType::F64),
        StaticType::F64
    );

    // Non-numeric structural joins go through CoreType. Julia computes
    // Tuple{Real, Any}; StaticType has no abstract carriers, so those members
    // widen to Any without losing the enclosing tuple shape (Issue #10865).
    assert_eq!(
        engine.unify_types(
            &StaticType::Tuple(vec![StaticType::I64, StaticType::Str]),
            &StaticType::Tuple(vec![StaticType::F64, StaticType::Char])
        ),
        StaticType::Tuple(vec![StaticType::Any, StaticType::Any])
    );

    // Any is absorbing: unify(Any, T) = Any (Issue #3461)
    assert_eq!(
        engine.unify_types(&StaticType::Any, &StaticType::I64),
        StaticType::Any
    );
    assert_eq!(
        engine.unify_types(&StaticType::I64, &StaticType::Any),
        StaticType::Any
    );
}

#[test]
fn test_numeric_promote_bool() {
    let engine = TypeInferenceEngine::new();

    // `numeric_promote` is the pure promotion of the operand *types*: Julia's
    // `promote(true, true) === (true, true)` keeps `Bool` (Issue #9351). The
    // op-specific `true + true === 2::Int64` rule lives in `binop_result_type`,
    // exercised by `test_binop_result_type_with_bool`.
    assert_eq!(
        engine.numeric_promote(&StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::Bool, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::I64, &StaticType::Bool),
        StaticType::I64
    );

    // Bool promotes to Float64 when mixed with float
    assert_eq!(
        engine.numeric_promote(&StaticType::Bool, &StaticType::F64),
        StaticType::F64
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::F64, &StaticType::Bool),
        StaticType::F64
    );
}

#[test]
fn test_numeric_promote_unsigned() {
    let engine = TypeInferenceEngine::new();

    // Same-type narrow unsigned integers preserve their type, matching upstream
    // Julia (`UInt8(1) + UInt8(2) === UInt8(3)`) and the VM runtime (Issue #9351).
    assert_eq!(
        engine.numeric_promote(&StaticType::U8, &StaticType::U8),
        StaticType::U8
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::U64, &StaticType::I64),
        StaticType::U64 // Larger of the two
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::I64, &StaticType::U64),
        StaticType::U64
    );

    // Unsigned with float promotes to float
    assert_eq!(
        engine.numeric_promote(&StaticType::U32, &StaticType::F64),
        StaticType::F64
    );
}

#[test]
fn test_issue_3715_shared_primitive_numeric_adapter_preserves_wide_types() {
    let engine = TypeInferenceEngine::new();

    assert_eq!(engine.type_name_to_static("Int128"), StaticType::I128);
    assert_eq!(engine.type_name_to_static("UInt128"), StaticType::U128);
    assert_eq!(engine.type_name_to_static("Float16"), StaticType::F16);

    assert_eq!(
        StaticType::from_primitive_numeric(crate::inference_core::PrimitiveNumeric::Int128),
        StaticType::I128
    );
    assert_eq!(
        StaticType::from_primitive_numeric(crate::inference_core::PrimitiveNumeric::UInt128),
        StaticType::U128
    );
    assert_eq!(
        StaticType::from_primitive_numeric(crate::inference_core::PrimitiveNumeric::Float16),
        StaticType::F16
    );

    assert_eq!(
        engine.numeric_promote(&StaticType::I64, &StaticType::I128),
        StaticType::I128
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::U64, &StaticType::U128),
        StaticType::U128
    );
    assert_eq!(
        engine.numeric_promote(&StaticType::F16, &StaticType::I64),
        StaticType::F16
    );
}

#[test]
fn test_binop_result_type_with_bool() {
    let engine = TypeInferenceEngine::new();
    use crate::ir::core::BinaryOp;

    // Bool arithmetic returns numeric result
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::Bool, &StaticType::Bool),
        StaticType::I64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::Bool, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::Bool, &StaticType::F64),
        StaticType::F64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::IntDiv, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mod, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::Bool, &StaticType::I64),
        StaticType::Bool
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::Bool, &StaticType::U8),
        StaticType::Bool
    );

    // Comparisons still return Bool
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Eq, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
}

/// Issue #9351: the AoT inference engine must preserve upstream narrow-int
/// result kinds (no force-widen of `Bool`/`≤UInt32` to `Int64`), converging
/// onto the same `promote_type` path used by the VM runtime and compiler.
/// Ground truth verified against upstream `julia` 1.12 and the sjulia VM:
///   Int8+Int8→Int8, Int8+Int16→Int16, Int8+UInt8→UInt8,
///   true+true→Int64, true-true→Int64, true*true→Bool.
#[test]
fn test_aot_narrow_int_promotion_parity_issue_9351() {
    let engine = TypeInferenceEngine::new();
    use crate::ir::core::BinaryOp;

    // Same-type and mixed narrow integers keep the promoted narrow kind.
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::I8, &StaticType::I8),
        StaticType::I8
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::I8, &StaticType::I8),
        StaticType::I8
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::I8, &StaticType::I16),
        StaticType::I16
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Sub, &StaticType::I16, &StaticType::I8),
        StaticType::I16
    );
    // Same width: unsigned wins (Julia `Int8 + UInt8 === UInt8`).
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::I8, &StaticType::U8),
        StaticType::U8
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::U16, &StaticType::U16),
        StaticType::U16
    );
    // Bool mixed with a narrow int promotes to that narrow int's type.
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::Bool, &StaticType::I8),
        StaticType::I8
    );

    // Bool `+`/`-` widen to Int64, but `*` stays Bool (`*` is `&`).
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Add, &StaticType::Bool, &StaticType::Bool),
        StaticType::I64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Sub, &StaticType::Bool, &StaticType::Bool),
        StaticType::I64
    );
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );

    // The AotBinOp variant used during multi-arg operator unfolding must agree.
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Add, &StaticType::I8, &StaticType::I8),
        StaticType::I8
    );
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Add, &StaticType::Bool, &StaticType::Bool),
        StaticType::I64
    );
    assert_eq!(
        engine.binop_result_type_static(&AotBinOp::Mul, &StaticType::Bool, &StaticType::Bool),
        StaticType::Bool
    );
}

#[test]
fn test_integer_type_with_float() {
    let engine = TypeInferenceEngine::new();

    // Integer division with floats returns Int64
    assert_eq!(
        engine.integer_type(&StaticType::F64, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.integer_type(&StaticType::I64, &StaticType::F64),
        StaticType::I64
    );

    // Both integers promote normally
    assert_eq!(
        engine.integer_type(&StaticType::I32, &StaticType::I64),
        StaticType::I64
    );
}

#[test]
fn test_infer_return_type_implicit_with_local_vars() {
    // Test: function calc_pi(N) with local variable prob::F64
    // and implicit return sqrt(6.0 / prob)
    use crate::ir::core::{Block, Literal, Stmt};

    let engine = TypeInferenceEngine::new();

    // Build: prob = 0.5; convert(Any, sqrt(6.0 / prob))
    let prob_assign = Stmt::Assign {
        var: "prob".to_string(),
        value: Expr::Literal(Literal::Float(0.5), test_span()),
        span: test_span(),
    };

    // sqrt(6.0 / prob)
    let sqrt_expr = Expr::Call {
        function: "sqrt".to_string().into(),
        args: vec![Expr::BinaryOp {
            op: crate::ir::core::BinaryOp::Div,
            left: Box::new(Expr::Literal(Literal::Float(6.0), test_span())),
            right: Box::new(Expr::Var("prob".to_string().into(), test_span())),
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    // convert(Any, sqrt(...))
    let convert_expr = Expr::Call {
        function: "convert".to_string().into(),
        args: vec![Expr::Var("Any".to_string().into(), test_span()), sqrt_expr],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let last_stmt = Stmt::Expr {
        expr: convert_expr,
        span: test_span(),
    };

    let block = Block {
        stmts: vec![prob_assign, last_stmt],
        span: test_span(),
    };

    // Infer return type: should be F64, not Any
    let return_type = engine.infer_return_type(&block, &[], &[]);
    assert_eq!(return_type, StaticType::F64);
}

fn empty_program(functions: Vec<Function>, main: Block) -> Program {
    Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: functions.into_iter().map(std::sync::Arc::new).collect(),
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main,
    }
}

fn union_return_function_issue_6939() -> Function {
    Function {
        new_struct_name: None,
        name: "choose".to_string(),
        params: vec![TypedParam::new(
            "flag".to_string(),
            Some(JuliaType::Bool),
            test_span(),
        )],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), test_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(1), test_span())),
                        span: test_span(),
                    }],
                    span: test_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(
                            Literal::Str("fallback".to_string()),
                            test_span(),
                        )),
                        span: test_span(),
                    }],
                    span: test_span(),
                }),
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    }
}

#[test]
fn union_return_inference_preserves_variants_issue_6939() {
    let engine = TypeInferenceEngine::new();
    let func = union_return_function_issue_6939();
    let return_ty =
        engine.infer_return_type(&func.body, &["flag".to_string()], &[StaticType::Bool]);

    assert_eq!(
        return_ty,
        StaticType::Union {
            variants: vec![StaticType::I64, StaticType::Str],
        }
    );

    let sig = FunctionSignature::new(
        "choose".to_string(),
        vec!["flag".to_string()],
        vec![StaticType::Bool],
        return_ty,
    );
    assert_eq!(sig.inference_level, 3);
    assert!(!sig.is_fully_static());
}

#[test]
fn union_return_static_call_has_no_dynamic_fallback_issue_6939() {
    let program = empty_program(
        vec![union_return_function_issue_6939()],
        Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "choose".to_string().into(),
                    args: vec![Expr::Literal(Literal::Bool(true), test_span())],
                    kwargs: vec![],
                    splat_mask: vec![false],
                    kwargs_splat_mask: vec![],
                    span: test_span(),
                },
                span: test_span(),
            }],
            span: test_span(),
        },
    );

    let result = crate::aot::compile_program(program, &crate::aot::CompileConfig::default())
        .expect("compile union-return program");

    assert_eq!(result.output.stats.dynamic_fallbacks, 0);
    assert!(result.output.dynamic_op_descriptions.is_empty());
    assert!(result.output.warnings.is_empty());
    assert!(
        result
            .output
            .rust_code
            .contains("pub fn choose(flag: bool) -> Value"),
        "{}",
        result.output.rust_code
    );
    assert!(result
        .output
        .rust_code
        .contains("return Value::from(1i64);"));
    assert!(result
        .output
        .rust_code
        .contains("return Value::from(\"fallback\".to_string());"));
}

#[test]
fn abstract_return_static_call_has_no_dynamic_fallback_issue_6939() {
    let func = Function {
        new_struct_name: None,
        name: "abstract_real".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: Some(JuliaType::Real),
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(42), test_span())),
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let mut engine = TypeInferenceEngine::new();
    let typed = engine
        .analyze_program(&empty_program(
            vec![func.clone()],
            Block {
                stmts: vec![Stmt::Expr {
                    expr: Expr::Call {
                        function: "abstract_real".to_string().into(),
                        args: vec![],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: test_span(),
                    },
                    span: test_span(),
                }],
                span: test_span(),
            },
        ))
        .expect("analyze abstract-return program");
    let typed_func = typed
        .get_functions("abstract_real")
        .and_then(|funcs| funcs.first())
        .expect("abstract_real typed function");
    assert_eq!(typed_func.signature.return_type, StaticType::Any);
    assert_eq!(typed_func.signature.inference_level, 4);

    let program = empty_program(
        vec![func],
        Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "abstract_real".to_string().into(),
                    args: vec![],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: test_span(),
                },
                span: test_span(),
            }],
            span: test_span(),
        },
    );
    let result = crate::aot::compile_program(program, &crate::aot::CompileConfig::default())
        .expect("compile abstract-return program");

    assert_eq!(result.output.stats.dynamic_fallbacks, 0);
    assert!(result.output.dynamic_op_descriptions.is_empty());
    assert!(
        result
            .output
            .rust_code
            .contains("pub fn abstract_real() -> Value"),
        "{}",
        result.output.rust_code
    );
    assert!(result
        .output
        .rust_code
        .contains("return Value::from(42i64);"));
}

// ========== Issue #1189: TypedEmptyArray Type Inference ==========

#[test]
fn test_typed_empty_array_int64() {
    // Test: Int64[] → Array { element: I64, ndims: 1 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "Int64".to_string().into(),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::I64);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_typed_empty_array_float64() {
    // Test: Float64[] → Array { element: F64, ndims: 1 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "Float64".to_string().into(),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::F64);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_typed_empty_array_bool() {
    // Test: Bool[] → Array { element: Bool, ndims: 1 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "Bool".to_string().into(),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::Bool);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_typed_empty_array_string() {
    // Test: String[] → Array { element: Str, ndims: 1 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "String".to_string().into(),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::Str);
        assert_eq!(ndims, Some(1));
    }
}

#[test]
fn test_typed_empty_array_int_alias() {
    // Test: Int[] → Array { element: I64, ndims: 1 } (Int is alias for Int64)
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "Int".to_string().into(),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    assert!(
        matches!(&ty, StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        ty
    );
    if let StaticType::Array { element, ndims } = ty {
        assert_eq!(*element, StaticType::I64);
        assert_eq!(ndims, Some(1));
    }
}

// ========== Issue #1190: Call-site Type Propagation for Arrays ==========

#[test]
fn test_call_site_array_specialization_single_type() {
    // When a function is called with Vec<i64> at all call sites,
    // the parameter should be specialized to Array{I64}
    let mut engine = TypeInferenceEngine::new();

    // Simulate call site collection: array_sum called with Vec<i64>
    engine.specializations.enqueue(CodeInstanceKey::new(
        "array_sum",
        vec![StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        }],
    ));

    // Create a function with untyped parameter
    let func = Function {
        new_struct_name: None,
        name: "array_sum".to_string(),
        params: vec![crate::ir::core::TypedParam {
            name: "arr".to_string(),
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span: test_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let signature = engine.infer_function_signature(&func);
    assert_eq!(signature.param_types.len(), 1);
    assert!(
        matches!(&signature.param_types[0], StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        signature.param_types[0]
    );
    if let StaticType::Array { element, ndims } = &signature.param_types[0] {
        assert_eq!(**element, StaticType::I64);
        assert_eq!(*ndims, Some(1));
    }
}

#[test]
fn explicit_any_parameter_is_not_call_site_specialized_issue_7071() {
    let mut engine = TypeInferenceEngine::new();
    engine
        .specializations
        .enqueue(CodeInstanceKey::new("fallback", vec![StaticType::I64]));

    let func = Function {
        new_struct_name: None,
        name: "fallback".to_string(),
        params: vec![crate::ir::core::TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Any),
            is_varargs: false,
            vararg_count: None,
            span: test_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let signature = engine.infer_function_signature(&func);
    assert_eq!(signature.param_types, vec![StaticType::Any]);
}

#[test]
fn test_call_site_array_specialization_multiple_numeric_types() {
    // When a function is called with Vec<i64> and Vec<f64>,
    // the element type should be promoted to F64
    let mut engine = TypeInferenceEngine::new();

    // Simulate call site collection: process_array called with Vec<i64> and Vec<f64>
    engine.specializations.enqueue(CodeInstanceKey::new(
        "process_array",
        vec![StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        }],
    ));
    engine.specializations.enqueue(CodeInstanceKey::new(
        "process_array",
        vec![StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(1),
        }],
    ));

    // Create a function with untyped parameter
    let func = Function {
        new_struct_name: None,
        name: "process_array".to_string(),
        params: vec![crate::ir::core::TypedParam {
            name: "arr".to_string(),
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span: test_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let signature = engine.infer_function_signature(&func);
    assert_eq!(signature.param_types.len(), 1);
    assert!(
        matches!(&signature.param_types[0], StaticType::Array { .. }),
        "Expected Array type, got {:?}",
        signature.param_types[0]
    );
    if let StaticType::Array { element, ndims } = &signature.param_types[0] {
        // Element type should be promoted to F64
        assert_eq!(**element, StaticType::F64);
        assert_eq!(*ndims, Some(1));
    }
}

#[test]
fn test_type_name_to_static_all_types() {
    let engine = TypeInferenceEngine::new();

    // Test all supported type names
    assert_eq!(engine.type_name_to_static("Int"), StaticType::I64);
    assert_eq!(engine.type_name_to_static("Int8"), StaticType::I8);
    assert_eq!(engine.type_name_to_static("Int16"), StaticType::I16);
    assert_eq!(engine.type_name_to_static("Int32"), StaticType::I32);
    assert_eq!(engine.type_name_to_static("Int64"), StaticType::I64);
    assert_eq!(engine.type_name_to_static("Int128"), StaticType::I128);
    assert_eq!(engine.type_name_to_static("UInt8"), StaticType::U8);
    assert_eq!(engine.type_name_to_static("UInt16"), StaticType::U16);
    assert_eq!(engine.type_name_to_static("UInt32"), StaticType::U32);
    assert_eq!(engine.type_name_to_static("UInt64"), StaticType::U64);
    assert_eq!(engine.type_name_to_static("UInt128"), StaticType::U128);
    assert_eq!(engine.type_name_to_static("Float16"), StaticType::F16);
    assert_eq!(engine.type_name_to_static("Float32"), StaticType::F32);
    assert_eq!(engine.type_name_to_static("Float64"), StaticType::F64);
    assert_eq!(engine.type_name_to_static("Bool"), StaticType::Bool);
    assert_eq!(engine.type_name_to_static("String"), StaticType::Str);
    assert_eq!(engine.type_name_to_static("Char"), StaticType::Char);
    assert_eq!(engine.type_name_to_static("Any"), StaticType::Any);
    assert_eq!(engine.type_name_to_static("Nothing"), StaticType::Nothing);
    assert_eq!(engine.type_name_to_static("Missing"), StaticType::Missing);
    assert_eq!(
        engine.type_name_to_static("Vector{Int64}"),
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        }
    );
    assert_eq!(
        engine.type_name_to_static("Matrix{Float64}"),
        StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(2),
        }
    );
    assert_eq!(
        engine.type_name_to_static("Matrix"),
        StaticType::Array {
            element: Box::new(StaticType::Any),
            ndims: Some(2),
        }
    );
    assert_eq!(
        engine.type_name_to_static("Array{Int64, 2}"),
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(2),
        }
    );

    // Unknown types should map to Any
    assert_eq!(engine.type_name_to_static("Unknown"), StaticType::Any);
}

#[test]
fn convert_call_inference_uses_type_name_not_constructor_function_issue_7495() {
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("x".to_string(), StaticType::F64);
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);

    let convert_int = Expr::Call {
        function: "convert".to_string().into(),
        args: vec![
            Expr::Var("Int64".to_string().into(), span),
            Expr::Var("x".to_string().into(), span),
        ],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };
    assert_eq!(engine.infer_expr_type(&convert_int), StaticType::I64);

    let convert_any = Expr::Call {
        function: "convert".to_string().into(),
        args: vec![
            Expr::Var("Any".to_string().into(), span),
            Expr::Var("x".to_string().into(), span),
        ],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };
    assert_eq!(engine.infer_expr_type(&convert_any), StaticType::F64);
}

#[test]
fn operator_call_inference_types_collatz_condition_issue_7504() {
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("n".to_string(), StaticType::I64);
    let span = crate::span::Span::new(0, 0, 1, 1, 1, 1);

    let modulo = Expr::Call {
        function: "%".to_string().into(),
        args: vec![
            Expr::Var("n".to_string().into(), span),
            Expr::Literal(Literal::Int(2), span),
        ],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };
    assert_eq!(engine.infer_expr_type(&modulo), StaticType::I64);

    let condition = Expr::Call {
        function: "==".to_string().into(),
        args: vec![modulo, Expr::Literal(Literal::Int(0), span)],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };
    assert_eq!(engine.infer_expr_type(&condition), StaticType::Bool);

    let int_div = Expr::Call {
        function: "div".to_string().into(),
        args: vec![
            Expr::Var("n".to_string().into(), span),
            Expr::Literal(Literal::Int(2), span),
        ],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };
    assert_eq!(engine.infer_expr_type(&int_div), StaticType::I64);
}

#[test]
fn test_broadcast_call_site_specializes_mandelbrot_escape_complex_param() {
    let src = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2
    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)
    C = xs' .+ im .* ys
    mandelbrot_escape.(C, Ref(maxiter))
end

function main()
    mandelbrot_grid(50, 25, 50)
end
"#;

    let mut parser = crate::parser::Parser::new().expect("parser");
    let outcome = parser.parse(src).expect("parse");
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = crate::lowering::Lowering::new(src);
    let program = lowering.lower(outcome).expect("lower");
    let mut engine = TypeInferenceEngine::new();
    let typed = engine.analyze_program(&program).expect("analyze");
    let sig = &typed
        .get_functions("mandelbrot_escape")
        .expect("mandelbrot_escape")
        .first()
        .expect("first")
        .signature;

    assert_eq!(
        sig.param_types[0],
        StaticType::Struct {
            type_id: 0,
            name: "Complex{Float64}".to_string()
        },
        "call sites: {:?}",
        engine
            .specializations
            .observed_args_for("mandelbrot_escape")
    );
}

#[test]
fn test_code_instance_dependencies_record_nested_calls() {
    let src = r#"
function inner(x)
    x + 1
end

function caller(y)
    inner(y)
end

caller(41)
"#;

    let mut parser = crate::parser::Parser::new().expect("parser");
    let outcome = parser.parse(src).expect("parse");
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = crate::lowering::Lowering::new(src);
    let program = lowering.lower(outcome).expect("lower");

    let mut engine = TypeInferenceEngine::new();
    engine.analyze_program(&program).expect("analyze");

    let caller = CodeInstanceKey::new("caller", vec![StaticType::I64]);
    let inner = CodeInstanceKey::new("inner", vec![StaticType::I64]);
    let caller_instance = engine
        .specializations
        .get(&caller)
        .expect("caller specialization");

    assert!(
        caller_instance.dependencies.contains(&inner),
        "dependencies: {:?}",
        caller_instance.dependencies
    );

    let inner_instance = engine
        .specializations
        .get(&inner)
        .expect("inner specialization");
    assert!(inner_instance.source.is_some());
    assert_eq!(inner_instance.return_type, Some(StaticType::I64));
}

// Issue #3462 — Float32 preservation and Bool widening
#[test]
fn test_div_float32_preservation() {
    let engine = TypeInferenceEngine::new();
    use crate::ir::core::BinaryOp;

    // Float32 / Float32 -> Float32 (Issue #3462)
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::F32, &StaticType::F32),
        StaticType::F32
    );
    // Float64 / Float64 -> Float64
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::F64, &StaticType::F64),
        StaticType::F64
    );
    // Int / Int -> Float64
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::I64, &StaticType::I64),
        StaticType::F64
    );
    // Float32 / Int -> Float32
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::F32, &StaticType::I64),
        StaticType::F32
    );
    // Float64 dominates Float32
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Div, &StaticType::F32, &StaticType::F64),
        StaticType::F64
    );
}

#[test]
fn test_pow_float32_preservation() {
    let engine = TypeInferenceEngine::new();
    use crate::ir::core::BinaryOp;

    // Float32 ^ Int -> Float32 (Issue #3462)
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::F32, &StaticType::I64),
        StaticType::F32
    );
    // Float32 ^ Float32 -> Float32
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::F32, &StaticType::F32),
        StaticType::F32
    );
    // Float32 ^ Float64 -> Float64
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::F32, &StaticType::F64),
        StaticType::F64
    );
    // Int ^ Int -> Int (same type)
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Pow, &StaticType::I64, &StaticType::I64),
        StaticType::I64
    );
}

#[test]
fn test_sqrt_float32_preservation() {
    let engine = TypeInferenceEngine::new();

    // sqrt(Float32) -> Float32 (Issue #3462)
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::F32]),
        StaticType::F32
    );
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::F64]),
        StaticType::F64
    );
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::I64]),
        StaticType::F64
    );
}

// Issue #3463 — size() tuple arity from ndims
#[test]
fn test_size_tuple_arity() {
    let src = r#"
function size_1d(A::Vector{Float64})
    size(A)
end
function size_2d(A::Matrix{Float64})
    size(A)
end
"#;
    let mut parser = crate::parser::Parser::new().expect("parser");
    let outcome = parser.parse(src).expect("parse");
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = crate::lowering::Lowering::new(src);
    let program = lowering.lower(outcome).expect("lower");
    let mut engine = TypeInferenceEngine::new();
    let typed = engine.analyze_program(&program).expect("analyze");

    let ret_1d = typed
        .get_functions("size_1d")
        .and_then(|fns| fns.first().map(|f| f.signature.return_type.clone()))
        .expect("size_1d return type");
    assert_eq!(
        ret_1d,
        StaticType::Tuple(vec![StaticType::I64]),
        "1D array size should be Tuple{{Int64}}"
    );

    let ret_2d = typed
        .get_functions("size_2d")
        .and_then(|fns| fns.first().map(|f| f.signature.return_type.clone()))
        .expect("size_2d return type");
    assert_eq!(
        ret_2d,
        StaticType::Tuple(vec![StaticType::I64, StaticType::I64]),
        "2D array size should be Tuple{{Int64, Int64}}"
    );
}

// Issue #3465 — String * non-String should not infer String
#[test]
fn test_string_mul_non_string_does_not_infer_str() {
    let engine = TypeInferenceEngine::new();
    use crate::ir::core::BinaryOp;

    // String * String -> String (valid)
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::Str, &StaticType::Str),
        StaticType::Str
    );
    // String * Int should NOT return Str (Issue #3465)
    assert_ne!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::Str, &StaticType::I64),
        StaticType::Str
    );
    // Int * String should NOT return Str
    assert_ne!(
        engine.binop_result_type(&BinaryOp::Mul, &StaticType::I64, &StaticType::Str),
        StaticType::Str
    );
}

// Issue #3466 — Complex{Float32} abs2 returns Float32
#[test]
fn test_abs2_complex_float32_returns_f32() {
    let engine = TypeInferenceEngine::new();

    let complexf32 = StaticType::Struct {
        type_id: 0,
        name: "Complex{Float32}".to_string(),
    };
    let complexf64 = StaticType::Struct {
        type_id: 0,
        name: "Complex{Float64}".to_string(),
    };
    let complex = StaticType::Struct {
        type_id: 0,
        name: "Complex".to_string(),
    };

    assert_eq!(
        engine.call_result_type("abs2", &[complexf32]),
        StaticType::F32
    );
    assert_eq!(
        engine.call_result_type("abs2", &[complexf64]),
        StaticType::F64
    );
    assert_eq!(engine.call_result_type("abs2", &[complex]), StaticType::F64);
}

// Issue #3464 — broadcast result type inference
#[test]
fn test_broadcast_general_vector_vector() {
    let engine = TypeInferenceEngine::new();

    // Vector{Int64} .+ Vector{Int64} -> Vector{Int64}
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::F32]),
        StaticType::F32,
        "sqrt(F32) should return F32"
    );
}

// Issue #3541 — Array{Any} signatures should match typed arrays.
#[test]
fn test_call_result_type_array_any_matches_typed_array_3541() {
    let engine = TypeInferenceEngine::new();
    let arr_i64 = StaticType::Array {
        element: Box::new(StaticType::I64),
        ndims: Some(1),
    };
    let arr_f64 = StaticType::Array {
        element: Box::new(StaticType::F64),
        ndims: Some(1),
    };
    // `length` is registered with Array{Any, ndims:None}; it should still match.
    assert_eq!(
        engine.call_result_type("length", std::slice::from_ref(&arr_i64)),
        StaticType::I64,
        "length(Array{{Int64}}) should return I64 via Array{{Any}} wildcard"
    );
    assert_eq!(
        engine.call_result_type("length", std::slice::from_ref(&arr_f64)),
        StaticType::I64,
        "length(Array{{Float64}}) should return I64 via Array{{Any}} wildcard"
    );

    // size and pop! similarly.
    let size_ty = engine.call_result_type("size", std::slice::from_ref(&arr_i64));
    assert!(matches!(size_ty, StaticType::Tuple(_)));

    let pop_ty = engine.call_result_type("pop!", std::slice::from_ref(&arr_i64));
    // pop! on Array{Any} returns Any; that is the conservative behavior expected.
    assert_eq!(pop_ty, StaticType::Any);
}

// ========== Issue #3542: AoT return inference for final assignment ==========

#[test]
fn test_issue_3542_final_assignment_returns_assigned_value() {
    // function f()
    //     x = 42
    // end
    // The function's return value is the assignment value, i.e., I64.
    use crate::ir::core::{Block, Stmt};

    let engine = TypeInferenceEngine::new();

    let assign = Stmt::Assign {
        var: "x".to_string(),
        value: Expr::Literal(Literal::Int(42), test_span()),
        span: test_span(),
    };

    let block = Block {
        stmts: vec![assign],
        span: test_span(),
    };

    let return_type = engine.infer_return_type(&block, &[], &[]);
    assert_eq!(return_type, StaticType::I64);
}

#[test]
fn test_issue_3542_final_assignment_with_string() {
    // function f()
    //     y = "hello"
    // end
    // Return type should be Str, not Nothing.
    use crate::ir::core::{Block, Stmt};

    let engine = TypeInferenceEngine::new();

    let assign = Stmt::Assign {
        var: "y".to_string(),
        value: Expr::Literal(Literal::Str("hello".to_string()), test_span()),
        span: test_span(),
    };

    let block = Block {
        stmts: vec![assign],
        span: test_span(),
    };

    let return_type = engine.infer_return_type(&block, &[], &[]);
    assert_eq!(return_type, StaticType::Str);
}

#[test]
fn test_issue_3542_final_assignment_after_other_stmts() {
    // function f()
    //     a = 1
    //     b = 2
    //     z = 3.0
    // end
    // Return type should be F64 (the last assignment's value).
    use crate::ir::core::{Block, Stmt};

    let engine = TypeInferenceEngine::new();

    let block = Block {
        stmts: vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: Expr::Literal(Literal::Int(1), test_span()),
                span: test_span(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: Expr::Literal(Literal::Int(2), test_span()),
                span: test_span(),
            },
            Stmt::Assign {
                var: "z".to_string(),
                value: Expr::Literal(Literal::Float(3.0), test_span()),
                span: test_span(),
            },
        ],
        span: test_span(),
    };

    let return_type = engine.infer_return_type(&block, &[], &[]);
    assert_eq!(return_type, StaticType::F64);
}
