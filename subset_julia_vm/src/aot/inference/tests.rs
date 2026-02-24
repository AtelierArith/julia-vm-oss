use super::*;
use crate::aot::types::StaticType;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, UnaryOp};

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

    // Any is treated as unknown - use the other type
    assert_eq!(
        engine.join_types(&StaticType::Any, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.join_types(&StaticType::I64, &StaticType::Any),
        StaticType::I64
    );

    // Non-numeric types create a union
    let joined = engine.join_types(&StaticType::Str, &StaticType::Bool);
    assert!(matches!(joined, StaticType::Union { .. }));
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
fn test_numeric_promote_preserves_struct_with_numeric() {
    let engine = TypeInferenceEngine::new();
    let complex = StaticType::Struct {
        type_id: 0,
        name: "Complex".to_string(),
    };
    assert_eq!(engine.numeric_promote(&complex, &StaticType::F64), complex);
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
fn test_call_result_type() {
    let engine = TypeInferenceEngine::new();

    // Known builtin
    assert_eq!(
        engine.call_result_type("sqrt", &[StaticType::F64]),
        StaticType::F64
    );

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
        left: Box::new(Expr::Var("x".to_string(), test_span())),
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
        condition: Box::new(Expr::Var("cond".to_string(), test_span())),
        then_expr: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        else_expr: Box::new(Expr::Literal(Literal::Float(2.0), test_span())),
        span: test_span(),
    };
    let ty = engine.infer_expr_type(&expr);
    // Either Union or the join of I64 and F64
    match ty {
        StaticType::Union { variants } => {
            assert!(variants.contains(&StaticType::I64));
            assert!(variants.contains(&StaticType::F64));
        }
        _ => {} // Other join strategies are acceptable
    }
}

#[test]
fn test_infer_expr_unary_neg() {
    // Test: -x preserves type
    let mut engine = TypeInferenceEngine::new();
    engine.env.insert("x".to_string(), StaticType::I64);

    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(Expr::Var("x".to_string(), test_span())),
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
        operand: Box::new(Expr::Var("x".to_string(), test_span())),
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
        array: Box::new(Expr::Var("arr".to_string(), test_span())),
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
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Float(4.0), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    assert_eq!(engine.infer_expr_type(&expr), StaticType::F64);
}

#[test]
fn test_infer_expr_call_with_kwargs_dispatches_by_all_args() {
    let engine = TypeInferenceEngine::new();
    let expr = Expr::Call {
        function: "range".to_string(),
        args: vec![
            Expr::Literal(Literal::Float(-2.0), test_span()),
            Expr::Literal(Literal::Float(1.0), test_span()),
        ],
        kwargs: vec![(
            "length".to_string(),
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
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Float(4.0), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    let expr = Expr::Call {
        function: "convert".to_string(),
        args: vec![Expr::Var("Any".to_string(), test_span()), inner_expr],
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

    let expr = Expr::Var("arr".to_string(), test_span());
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

    let expr = Expr::Var("s".to_string(), test_span());
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

    // With Any
    assert_eq!(
        engine.unify_types(&StaticType::Any, &StaticType::I64),
        StaticType::I64
    );
    assert_eq!(
        engine.unify_types(&StaticType::I64, &StaticType::Any),
        StaticType::I64
    );
}

#[test]
fn test_numeric_promote_bool() {
    let engine = TypeInferenceEngine::new();

    // Bool promotes to Int64 in arithmetic
    assert_eq!(
        engine.numeric_promote(&StaticType::Bool, &StaticType::Bool),
        StaticType::I64
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

    // Unsigned integers promote correctly
    assert_eq!(
        engine.numeric_promote(&StaticType::U8, &StaticType::U8),
        StaticType::I64 // Small integers promote to I64 for safety
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

    // Comparisons still return Bool
    assert_eq!(
        engine.binop_result_type(&BinaryOp::Eq, &StaticType::Bool, &StaticType::Bool),
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
        function: "sqrt".to_string(),
        args: vec![Expr::BinaryOp {
            op: crate::ir::core::BinaryOp::Div,
            left: Box::new(Expr::Literal(Literal::Float(6.0), test_span())),
            right: Box::new(Expr::Var("prob".to_string(), test_span())),
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    // convert(Any, sqrt(...))
    let convert_expr = Expr::Call {
        function: "convert".to_string(),
        args: vec![Expr::Var("Any".to_string(), test_span()), sqrt_expr],
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

// ========== Issue #1189: TypedEmptyArray Type Inference ==========

#[test]
fn test_typed_empty_array_int64() {
    // Test: Int64[] → Array { element: I64, ndims: 1 }
    let engine = TypeInferenceEngine::new();
    let expr = Expr::TypedEmptyArray {
        element_type: "Int64".to_string(),
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
        element_type: "Float64".to_string(),
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
        element_type: "Bool".to_string(),
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
        element_type: "String".to_string(),
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
        element_type: "Int".to_string(),
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
    engine.call_sites.insert(
        "array_sum".to_string(),
        vec![vec![StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        }]],
    );

    // Create a function with untyped parameter
    let func = Function {
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
fn test_call_site_array_specialization_multiple_numeric_types() {
    // When a function is called with Vec<i64> and Vec<f64>,
    // the element type should be promoted to F64
    let mut engine = TypeInferenceEngine::new();

    // Simulate call site collection: process_array called with Vec<i64> and Vec<f64>
    engine.call_sites.insert(
        "process_array".to_string(),
        vec![
            vec![StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            }],
            vec![StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(1),
            }],
        ],
    );

    // Create a function with untyped parameter
    let func = Function {
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
    assert_eq!(engine.type_name_to_static("Int64"), StaticType::I64);
    assert_eq!(engine.type_name_to_static("Int32"), StaticType::I32);
    assert_eq!(engine.type_name_to_static("Float64"), StaticType::F64);
    assert_eq!(engine.type_name_to_static("Float32"), StaticType::F32);
    assert_eq!(engine.type_name_to_static("Bool"), StaticType::Bool);
    assert_eq!(engine.type_name_to_static("String"), StaticType::Str);
    assert_eq!(engine.type_name_to_static("Char"), StaticType::Char);
    assert_eq!(engine.type_name_to_static("Any"), StaticType::Any);
    assert_eq!(engine.type_name_to_static("Nothing"), StaticType::Nothing);

    // Unknown types should map to Any
    assert_eq!(engine.type_name_to_static("Unknown"), StaticType::Any);
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
            name: "Complex".to_string()
        },
        "call sites: {:?}",
        engine.call_sites.get("mandelbrot_escape")
    );
}
