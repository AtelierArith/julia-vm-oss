use super::*;
use crate::aot::inference::{FunctionSignature, TypeInferenceEngine, TypedFunction, TypedProgram};
use crate::aot::ir::{AotBinOp, AotBuiltinOp, AotExpr, AotInlinePolicy, AotStmt, AotUnaryOp};
use crate::aot::types::StaticType;
use crate::ir::core::{
    Expr, Function, Literal, MetaAnnotation, Program, Stmt, StructDef, StructField,
};
use crate::types::{JuliaType, TypeExpr, TypeParam};

#[test]
fn test_function_info_new() {
    let info = FunctionInfo::new("test".to_string(), 0);
    assert_eq!(info.name, "test");
    assert_eq!(info.offset, 0);
    assert!(!info.is_recursive);
}

#[test]
fn test_analysis_result_new() {
    let result = AnalysisResult::new();
    assert!(result.functions.is_empty());
    assert!(result.entry_point.is_none());
}

#[test]
fn test_add_function() {
    let mut result = AnalysisResult::new();
    let info = FunctionInfo::new("main".to_string(), 0);
    result.add_function(info);
    assert!(result.get_function("main").is_some());
}

#[test]
fn test_analyzer_empty_ir_is_error() {
    let mut analyzer = CoreIrAnalyzer::new();
    // Empty bytes are not valid persisted Core IR, so this should return an error.
    let result = analyzer.analyze(&[]);
    assert!(result.is_err());
}

// ========== IR Conversion Tests ==========

use crate::ir::core::{BinaryOp, Block, TypedParam, UnaryOp};
use crate::span::Span;

fn test_span() -> Span {
    Span::new(0, 0, 1, 1, 0, 0)
}

fn empty_program() -> Program {
    Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    }
}

#[test]
fn test_convert_literal_int() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Int(42);
    let result = converter.convert_literal(&lit).unwrap();
    assert!(matches!(result, AotExpr::LitI64(42)));
}

#[test]
fn test_convert_literal_float() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Float(1.25);
    let result = converter.convert_literal(&lit).unwrap();
    assert!(
        matches!(&result, AotExpr::LitF64(_)),
        "Expected LitF64, got {:?}",
        result
    );
    if let AotExpr::LitF64(v) = result {
        assert!((v - 1.25).abs() < f64::EPSILON);
    }
}

#[test]
fn test_convert_literal_bool() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Bool(true);
    let result = converter.convert_literal(&lit).unwrap();
    assert!(matches!(result, AotExpr::LitBool(true)));
}

#[test]
fn test_convert_literal_missing_issue_6935() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Missing;
    let result = converter.convert_literal(&lit).unwrap();
    assert!(matches!(result, AotExpr::LitMissing));
}

#[test]
fn test_convert_literal_string() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Str("hello".to_string());
    let result = converter.convert_literal(&lit).unwrap();
    assert!(
        matches!(&result, AotExpr::LitStr(_)),
        "Expected LitStr, got {:?}",
        result
    );
    if let AotExpr::LitStr(s) = result {
        assert_eq!(s, "hello");
    }
}

#[test]
fn global_inf_nan_constants_convert_to_float_literals_issue_7017() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    for (name, is_f32) in [
        ("Inf", false),
        ("Inf64", false),
        ("NaN", false),
        ("NaN64", false),
        ("Inf32", true),
        ("NaN32", true),
    ] {
        let result = converter
            .convert_expr(&Expr::Var(name.to_string(), test_span()))
            .unwrap();
        match (result, is_f32, name.contains("NaN")) {
            (AotExpr::LitF64(value), false, true) => assert!(value.is_nan()),
            (AotExpr::LitF64(value), false, false) => assert_eq!(value, f64::INFINITY),
            (AotExpr::LitF32(value), true, true) => assert!(value.is_nan()),
            (AotExpr::LitF32(value), true, false) => assert_eq!(value, f32::INFINITY),
            (other, _, _) => panic!("unexpected constant conversion for {name}: {other:?}"),
        }
    }
}

#[test]
fn local_inf_nan_bindings_are_not_rewritten_issue_7017() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);
    converter
        .engine
        .env
        .insert("Inf".to_string(), StaticType::I64);
    converter
        .engine
        .env
        .insert("NaN".to_string(), StaticType::Str);

    let inf = converter
        .convert_expr(&Expr::Var("Inf".to_string(), test_span()))
        .unwrap();
    let nan = converter
        .convert_expr(&Expr::Var("NaN".to_string(), test_span()))
        .unwrap();

    assert!(matches!(
        inf,
        AotExpr::Var {
            name,
            ty: StaticType::I64
        } if name == "Inf"
    ));
    assert!(matches!(
        nan,
        AotExpr::Var {
            name,
            ty: StaticType::Str
        } if name == "NaN"
    ));
}

#[test]
fn expression_let_block_single_expr_still_converts_issue_7014() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let expr = Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Literal(Literal::Int(7), test_span()),
                span: test_span(),
            }],
            span: test_span(),
        },
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    assert!(matches!(result, AotExpr::LitI64(7)));
}

#[test]
fn expression_let_block_side_effects_reject_with_span_issue_7014() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let span = Span::new(5, 40, 2, 2, 5, 40);
    let expr = Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "println".to_string(),
                        args: vec![Expr::Literal(Literal::Str("side".to_string()), test_span())],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span: test_span(),
                    },
                    span: test_span(),
                },
                Stmt::Expr {
                    expr: Expr::Literal(Literal::Int(1), test_span()),
                    span: test_span(),
                },
            ],
            span,
        },
        span,
    };

    let err = converter.convert_expr(&expr).unwrap_err();
    match err {
        crate::aot::AotError::UnsupportedInstruction(diagnostic) => {
            assert!(diagnostic.message.contains("begin/let"));
            assert!(diagnostic.message.contains("#7014"));
            assert_eq!(diagnostic.span, Some(span));
            assert!(diagnostic
                .workaround
                .as_deref()
                .is_some_and(|workaround| workaround.contains("statement position")));
        }
        other => panic!("expected UnsupportedInstruction, got {other:?}"),
    }
}

#[test]
fn test_program_to_aot_ir_rejects_ccall_boundary_with_span() {
    let typed = TypedProgram::new();
    let mut program = empty_program();
    let span = Span::new(10, 28, 3, 3, 5, 23);
    program.main.stmts.push(Stmt::Expr {
        expr: Expr::Call {
            function: "ccall".to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        },
        span,
    });

    let err = program_to_aot_ir(&program, &typed).unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("ccall"));
    assert!(msg.contains("line 3, column 5"));
    assert!(msg.contains("native ABI lowering"));
}

#[test]
fn local_untyped_dict_construction_rejects_with_span_issue_7034() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let span = Span::new(20, 34, 4, 4, 2, 16);
    let call = Expr::Call {
        function: "Dict".to_string(),
        args: vec![],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let err = converter.convert_expr(&call).unwrap_err();
    match err {
        crate::aot::AotError::UnsupportedInstruction(diagnostic) => {
            assert!(
                diagnostic.message.contains("Dict"),
                "unexpected message: {}",
                diagnostic.message
            );
            assert!(
                diagnostic.message.contains("Issue #7034"),
                "unexpected message: {}",
                diagnostic.message
            );
            assert_eq!(diagnostic.span, Some(span));
            assert!(diagnostic
                .workaround
                .as_deref()
                .is_some_and(|workaround| workaround.contains("Dict{K,V}")));
        }
        other => panic!("expected UnsupportedInstruction, got {other:?}"),
    }
}

#[test]
fn local_dict_construction_lowers_to_hashmap_issue_7034() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let span = Span::new(20, 34, 4, 4, 2, 16);
    let call = Expr::Call {
        function: "Dict".to_string(),
        args: vec![Expr::Pair {
            key: Box::new(Expr::Literal(Literal::Str("a".to_string()), span)),
            value: Box::new(Expr::Literal(Literal::Int(1), span)),
            span,
        }],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let result = converter
        .convert_expr(&call)
        .expect("Dict pair construction should lower");
    match result {
        AotExpr::CallBuiltin {
            builtin,
            args,
            return_ty:
                StaticType::Dict {
                    key: dict_key,
                    value: dict_value,
                },
        } => {
            assert_eq!(builtin, AotBuiltinOp::Dict);
            assert_eq!(args.len(), 1);
            assert_eq!(*dict_key, StaticType::Str);
            assert_eq!(*dict_value, StaticType::I64);
        }
        other => panic!("expected Dict builtin, got {other:?}"),
    }
}

#[test]
fn local_typed_empty_dict_construction_lowers_to_hashmap_issue_7034() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let span = Span::new(20, 50, 4, 4, 2, 32);
    let call = Expr::Call {
        function: "Dict{String, Int64}".to_string(),
        args: vec![],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let result = converter
        .convert_expr(&call)
        .expect("typed empty Dict construction should lower");
    match result {
        AotExpr::CallBuiltin {
            builtin,
            return_ty:
                StaticType::Dict {
                    key: dict_key,
                    value: dict_value,
                },
            ..
        } => {
            assert_eq!(builtin, AotBuiltinOp::Dict);
            assert_eq!(*dict_key, StaticType::Str);
            assert_eq!(*dict_value, StaticType::I64);
        }
        other => panic!("expected Dict builtin, got {other:?}"),
    }
}

#[test]
fn local_set_construction_lowers_to_hashset_issue_7035() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let span = Span::new(20, 34, 4, 4, 2, 16);
    let call = Expr::Call {
        function: "Set".to_string(),
        args: vec![Expr::ArrayLiteral {
            elements: vec![
                Expr::Literal(Literal::Int(1), span),
                Expr::Literal(Literal::Int(2), span),
                Expr::Literal(Literal::Int(2), span),
            ],
            shape: vec![3],
            span,
        }],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let result = converter
        .convert_expr(&call)
        .expect("Set([Int...]) should lower to native Set IR");
    match result {
        AotExpr::SetFromIter { elem_ty, iter } => {
            assert_eq!(elem_ty, StaticType::I64);
            assert!(matches!(*iter, AotExpr::ArrayLit { .. }));
        }
        other => panic!("expected SetFromIter, got {other:?}"),
    }
}

#[test]
fn parametric_struct_definition_alone_is_skipped_issue_6975() {
    // A parametric struct that is merely *defined* (e.g. Base's `LogRange{T}`
    // reachable from unrelated machinery) but never constructed is now skipped
    // rather than failing the whole compile (Issue #7251). The use-site gate is
    // still enforced — see `unresolved_constructor_like_call_rejects_with_span_issue_6975`.
    let typed = TypedProgram::new();
    let mut program = empty_program();
    let span = Span::new(2, 28, 1, 1, 1, 27);
    program.structs.push(StructDef {
        name: "Box".to_string(),
        is_mutable: false,
        type_params: vec![TypeParam::new("T".to_string())],
        parent_type: None,
        fields: vec![StructField {
            name: "x".to_string(),
            type_expr: None,
            span,
        }],
        inner_constructors: vec![],
        span,
    });

    let aot = program_to_aot_ir(&program, &typed)
        .expect("a defined-but-unused parametric struct must be skipped, not rejected");
    assert!(
        !aot.structs.iter().any(|s| s.name == "Box"),
        "skipped parametric struct `Box` must not be emitted"
    );
}

#[test]
fn parametric_struct_constructor_lowers_to_instantiated_struct_issue_7040() {
    let mut typed = TypedProgram::new();
    let mut program = empty_program();
    let span = Span::new(2, 28, 1, 1, 1, 27);
    let struct_def = StructDef {
        name: "Box".to_string(),
        is_mutable: false,
        type_params: vec![TypeParam::new("T".to_string())],
        parent_type: None,
        fields: vec![StructField {
            name: "x".to_string(),
            type_expr: Some(TypeExpr::TypeVar("T".to_string())),
            span,
        }],
        inner_constructors: vec![],
        span,
    };
    let engine = TypeInferenceEngine::new();
    typed.add_struct(
        engine
            .analyze_struct(&struct_def)
            .expect("parametric struct should analyze"),
    );
    program.structs.push(struct_def);
    let converter = IrConverter::new(&typed, &program);

    let call = Expr::Call {
        function: "Box{Int64}".to_string(),
        args: vec![Expr::Literal(Literal::Int(41), test_span())],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    };

    let result = converter
        .convert_expr(&call)
        .expect("explicit parametric constructor should lower");
    assert!(matches!(
        result,
        AotExpr::StructNew { ref name, .. } if name == "Box{Int64}"
    ));

    let inferred_call = Expr::Call {
        function: "Box".to_string(),
        args: vec![Expr::Literal(Literal::Float(1.5), test_span())],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    };
    let result = converter
        .convert_expr(&inferred_call)
        .expect("bare parametric constructor should infer type parameters");
    assert!(matches!(
        result,
        AotExpr::StructNew { ref name, .. } if name == "Box{Float64}"
    ));
}

#[test]
fn unresolved_constructor_like_call_rejects_with_span_issue_6975() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);
    let span = Span::new(20, 26, 1, 1, 20, 26);
    let call = Expr::Call {
        function: "Box".to_string(),
        args: vec![Expr::Literal(Literal::Int(1), test_span())],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    };

    let err = converter.convert_expr(&call).unwrap_err();
    match err {
        crate::aot::AotError::UnsupportedInstruction(diagnostic) => {
            assert!(
                diagnostic.message.contains("Box"),
                "unexpected message: {}",
                diagnostic.message
            );
            assert!(
                diagnostic.message.contains("#6975"),
                "unexpected message: {}",
                diagnostic.message
            );
            assert_eq!(diagnostic.span, Some(span));
        }
        other => panic!("expected UnsupportedInstruction, got {other:?}"),
    }
}

#[test]
fn top_level_string_global_rejects_before_static_codegen_issue_7011() {
    let typed = TypedProgram::new();
    let mut program = empty_program();
    let span = Span::new(4, 11, 2, 2, 4, 11);
    program.main.stmts.push(Stmt::Assign {
        var: "x".to_string(),
        value: Expr::Literal(Literal::Str("a".to_string()), span),
        span,
    });

    let err = program_to_aot_ir(&program, &typed).unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("top-level global `x`"));
    assert!(msg.contains("String"));
    assert!(msg.contains("line 2, column 4"));
    assert!(msg.contains("let` block"));
}

#[test]
fn test_program_to_aot_ir_rejects_core_intrinsics_llvmcall_boundary() {
    let typed = TypedProgram::new();
    let mut program = empty_program();
    let span = Span::new(20, 64, 4, 4, 9, 53);
    program.main.stmts.push(Stmt::Expr {
        expr: Expr::ModuleCall {
            module: "Core.Intrinsics".to_string(),
            function: "llvmcall".to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        },
        span,
    });

    let err = program_to_aot_ir(&program, &typed).unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("Core.Intrinsics.llvmcall"));
    assert!(msg.contains("line 4, column 9"));
    assert!(msg.contains("safe subset"));
}

#[test]
fn test_convert_literal_int128_in_range() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Int128(i64::MAX as i128);
    let result = converter.convert_literal(&lit).unwrap();
    assert!(matches!(result, AotExpr::LitI64(v) if v == i64::MAX));
}

#[test]
fn test_convert_literal_int128_out_of_range_errors() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Int128(i64::MAX as i128 + 1);
    let err = converter.convert_literal(&lit).unwrap_err();
    assert!(format!("{}", err).contains("Int128 literal out of Int64 range"));
}

#[test]
fn test_convert_literal_struct_complex_bool_normalizes_to_complex() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let lit = Literal::Struct(
        "Complex{Bool}".to_string(),
        vec![Literal::Bool(false), Literal::Bool(true)],
    );
    let result = converter.convert_literal(&lit).unwrap();
    assert!(
        matches!(&result, AotExpr::StructNew { .. }),
        "Expected StructNew Complex, got {:?}",
        result
    );
    if let AotExpr::StructNew { name, fields } = result {
        assert_eq!(name, "Complex");
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0], AotExpr::LitF64(v) if v == 0.0));
        assert!(matches!(fields[1], AotExpr::LitF64(v) if v == 1.0));
    }
}

#[test]
fn test_convert_expr_var() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);
    converter
        .engine
        .env
        .insert("x".to_string(), StaticType::I64);

    let expr = Expr::Var("x".to_string(), test_span());
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::Var { .. }),
        "Expected Var, got {:?}",
        result
    );
    if let AotExpr::Var { name, ty } = result {
        assert_eq!(name, "x");
        assert_eq!(ty, StaticType::I64);
    }
}

#[test]
fn test_convert_expr_binary_op() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        right: Box::new(Expr::Literal(Literal::Int(2), test_span())),
        span: test_span(),
    };
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::BinOpStatic { .. }),
        "Expected BinOpStatic, got {:?}",
        result
    );
    if let AotExpr::BinOpStatic { op, result_ty, .. } = result {
        assert_eq!(op, AotBinOp::Add);
        assert_eq!(result_ty, StaticType::I64);
    }
}

#[test]
fn two_arg_operator_alias_calls_use_binary_path_issue_6980() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let div_expr = Expr::Call {
        function: "div".to_string(),
        args: vec![
            Expr::Literal(Literal::Bool(true), test_span()),
            Expr::Literal(Literal::Bool(true), test_span()),
        ],
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span: test_span(),
    };

    let result = converter.convert_expr(&div_expr).unwrap();
    assert!(
        matches!(&result, AotExpr::BinOpStatic { .. }),
        "Expected BinOpStatic for div, got {:?}",
        result
    );
    if let AotExpr::BinOpStatic { op, result_ty, .. } = result {
        assert_eq!(op, AotBinOp::IntDiv);
        assert_eq!(result_ty, StaticType::Bool);
    }

    let mod_expr = Expr::Call {
        function: "mod".to_string(),
        args: vec![
            Expr::Literal(Literal::Bool(true), test_span()),
            Expr::Literal(Literal::Bool(true), test_span()),
        ],
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span: test_span(),
    };

    let result = converter.convert_expr(&mod_expr).unwrap();
    assert!(
        matches!(&result, AotExpr::CallBuiltin { .. }),
        "Expected CallBuiltin for mod, got {:?}",
        result
    );
    if let AotExpr::CallBuiltin {
        builtin, return_ty, ..
    } = result
    {
        assert_eq!(builtin, AotBuiltinOp::Mod);
        assert_eq!(return_ty, StaticType::Bool);
    }
}

#[test]
fn zeros_ones_type_argument_is_not_a_dimension_issue_7069() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Call {
        function: "zeros".to_string(),
        args: vec![
            Expr::Var("Int64".to_string(), test_span()),
            Expr::Literal(Literal::Int(3), test_span()),
        ],
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    let AotExpr::CallBuiltin {
        builtin,
        args,
        return_ty,
    } = result
    else {
        panic!("expected zeros builtin call");
    };
    assert_eq!(builtin, AotBuiltinOp::Zeros);
    assert_eq!(args.len(), 1);
    assert!(matches!(args[0], AotExpr::LitI64(3)));
    assert_eq!(
        return_ty,
        StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1)
        }
    );
}

#[test]
fn time_ns_builtin_converts_to_aot_builtin_issue_7059() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Builtin {
        name: crate::ir::core::BuiltinOp::TimeNs,
        args: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    let AotExpr::CallBuiltin {
        builtin,
        args,
        return_ty,
    } = result
    else {
        panic!("expected time_ns builtin call");
    };
    assert_eq!(builtin, AotBuiltinOp::TimeNs);
    assert!(args.is_empty());
    assert_eq!(return_ty, StaticType::I64);
}

#[test]
fn range_converter_uses_step_type_issue_6969() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        stop: Box::new(Expr::Literal(Literal::Int(2), test_span())),
        step: Some(Box::new(Expr::Literal(Literal::Float32(0.5), test_span()))),
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(
            result,
            AotExpr::Range {
                elem_ty: StaticType::F32,
                ..
            }
        ),
        "Expected Float32 range element type, got {:?}",
        result
    );
}

#[test]
fn test_convert_expr_complex_im_literal_folds_to_struct_new() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(Literal::Float(0.0), test_span())),
        right: Box::new(Expr::BinaryOp {
            op: BinaryOp::Mul,
            left: Box::new(Expr::Literal(Literal::Float(0.0), test_span())),
            right: Box::new(Expr::Literal(
                Literal::Struct(
                    "Complex{Bool}".to_string(),
                    vec![Literal::Bool(false), Literal::Bool(true)],
                ),
                test_span(),
            )),
            span: test_span(),
        }),
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::StructNew { .. }),
        "Expected folded Complex struct literal, got {:?}",
        result
    );
    if let AotExpr::StructNew { name, fields } = result {
        assert_eq!(name, "Complex");
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0], AotExpr::LitF64(v) if v == 0.0));
        assert!(matches!(fields[1], AotExpr::LitF64(v) if v == 0.0));
    }
}

#[test]
fn test_convert_expr_unary_op() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(Expr::Literal(Literal::Int(1), test_span())),
        span: test_span(),
    };
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::UnaryOp { .. }),
        "Expected UnaryOp, got {:?}",
        result
    );
    if let AotExpr::UnaryOp { op, result_ty, .. } = result {
        assert_eq!(op, AotUnaryOp::Neg);
        assert_eq!(result_ty, StaticType::I64);
    }
}

#[test]
fn test_convert_expr_array_literal() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::ArrayLiteral {
        elements: vec![
            Expr::Literal(Literal::Int(1), test_span()),
            Expr::Literal(Literal::Int(2), test_span()),
        ],
        shape: vec![2],
        span: test_span(),
    };
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::ArrayLit { .. }),
        "Expected ArrayLit, got {:?}",
        result
    );
    if let AotExpr::ArrayLit {
        elements,
        elem_ty,
        shape,
    } = result
    {
        assert_eq!(elements.len(), 2);
        assert_eq!(elem_ty, StaticType::I64);
        assert_eq!(shape, vec![2]);
    }
}

#[test]
fn test_convert_expr_tuple_literal() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::TupleLiteral {
        elements: vec![
            Expr::Literal(Literal::Int(1), test_span()),
            Expr::Literal(Literal::Str("hi".to_string()), test_span()),
        ],
        span: test_span(),
    };
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::TupleLit { .. }),
        "Expected TupleLit, got {:?}",
        result
    );
    if let AotExpr::TupleLit { elements } = result {
        assert_eq!(elements.len(), 2);
    }
}

#[test]
fn test_convert_stmt_assign_new() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let stmt = Stmt::Assign {
        var: "x".to_string(),
        value: Expr::Literal(Literal::Int(42), test_span()),
        span: test_span(),
    };
    let result = converter.convert_stmt(&stmt).unwrap();
    assert!(
        matches!(&result, AotStmt::Let { .. }),
        "Expected Let, got {:?}",
        result
    );
    if let AotStmt::Let {
        name,
        ty,
        is_mutable,
        ..
    } = result
    {
        assert_eq!(name, "x");
        assert_eq!(ty, StaticType::I64);
        assert!(is_mutable);
    }
}

#[test]
fn test_convert_stmt_assign_existing() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);
    converter.declared_locals.insert("x".to_string());
    converter
        .engine
        .env
        .insert("x".to_string(), StaticType::I64);

    let stmt = Stmt::Assign {
        var: "x".to_string(),
        value: Expr::Literal(Literal::Int(100), test_span()),
        span: test_span(),
    };
    let result = converter.convert_stmt(&stmt).unwrap();
    assert!(
        matches!(&result, AotStmt::Assign { .. }),
        "Expected Assign, got {:?}",
        result
    );
    if let AotStmt::Assign { target, .. } = result {
        assert!(
            matches!(&target, AotExpr::Var { .. }),
            "Expected Var target, got {:?}",
            target
        );
        if let AotExpr::Var { name, .. } = target {
            assert_eq!(name, "x");
        }
    }
}

#[test]
fn test_convert_stmt_return() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let stmt = Stmt::Return {
        value: Some(Expr::Literal(Literal::Int(42), test_span())),
        span: test_span(),
    };
    let result = converter.convert_stmt(&stmt).unwrap();
    assert!(
        matches!(&result, AotStmt::Return(Some(_))),
        "Expected Return with value, got {:?}",
        result
    );
}

#[test]
fn test_convert_stmt_if() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let stmt = Stmt::If {
        condition: Expr::Literal(Literal::Bool(true), test_span()),
        then_branch: Block {
            stmts: vec![],
            span: test_span(),
        },
        else_branch: None,
        span: test_span(),
    };
    let result = converter.convert_stmt(&stmt).unwrap();
    assert!(
        matches!(&result, AotStmt::If { .. }),
        "Expected If, got {:?}",
        result
    );
    if let AotStmt::If {
        then_branch,
        else_branch,
        ..
    } = result
    {
        assert!(then_branch.is_empty());
        assert!(else_branch.is_none());
    }
}

#[test]
fn test_convert_stmt_while() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let stmt = Stmt::While {
        condition: Expr::Literal(Literal::Bool(true), test_span()),
        body: Block {
            stmts: vec![],
            span: test_span(),
        },
        span: test_span(),
    };
    let result = converter.convert_stmt(&stmt).unwrap();
    assert!(
        matches!(&result, AotStmt::While { .. }),
        "Expected While, got {:?}",
        result
    );
    if let AotStmt::While { body, .. } = result {
        assert!(body.is_empty());
    }
}

#[test]
fn test_convert_stmt_break_continue() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let break_stmt = Stmt::Break { span: test_span() };
    let break_result = converter.convert_stmt(&break_stmt).unwrap();
    assert!(matches!(break_result, AotStmt::Break));

    let continue_stmt = Stmt::Continue { span: test_span() };
    let continue_result = converter.convert_stmt(&continue_stmt).unwrap();
    assert!(matches!(continue_result, AotStmt::Continue));
}

#[test]
fn test_convert_function_flattens_timed_stmt_body() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "timed_body".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Timed {
                body: Block {
                    stmts: vec![Stmt::Assign {
                        var: "x".to_string(),
                        value: Expr::Literal(Literal::Int(1), test_span()),
                        span: test_span(),
                    }],
                    span: test_span(),
                },
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.body.len(), 1);
    assert!(matches!(result.body[0], AotStmt::Let { .. }));
}

#[test]
fn test_convert_function_retains_inline_policy_from_meta() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "marked_noinline".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Meta {
                    annotation: MetaAnnotation {
                        name: "noinline".to_string(),
                        args: vec![],
                    },
                    span: test_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Literal(Literal::Int(1), test_span())),
                    span: test_span(),
                },
            ],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.inline_policy, AotInlinePolicy::Never);
    assert_eq!(result.body.len(), 1);
    assert!(matches!(result.body[0], AotStmt::Return(Some(_))));
}

#[test]
fn test_program_to_aot_ir_retains_wrapped_definition_inline_policy() {
    let src = r#"
@inline function wrapped_inline_aot_4286(x)
    return x
end

@noinline function wrapped_noinline_aot_4286(x)
    return x
end
"#;

    let mut parser = crate::parser::Parser::new().expect("parser");
    let outcome = parser.parse(src).expect("parse");
    let mut lowering = crate::lowering::Lowering::new(src);
    let program = lowering.lower(outcome).expect("lower");

    let inline_func = program
        .functions
        .iter()
        .find(|func| func.name == "wrapped_inline_aot_4286")
        .expect("wrapped inline function should be hoisted");
    assert!(matches!(
        inline_func.body.stmts.first(),
        Some(Stmt::Meta { annotation, .. }) if annotation.name == "inline"
    ));

    let noinline_func = program
        .functions
        .iter()
        .find(|func| func.name == "wrapped_noinline_aot_4286")
        .expect("wrapped noinline function should be hoisted");
    assert!(matches!(
        noinline_func.body.stmts.first(),
        Some(Stmt::Meta { annotation, .. }) if annotation.name == "noinline"
    ));

    let typed = TypedProgram::new();
    let aot = program_to_aot_ir(&program, &typed).unwrap();

    let inline_aot = aot
        .functions
        .iter()
        .find(|func| func.name == "wrapped_inline_aot_4286")
        .expect("wrapped inline function should convert to AoT IR");
    assert_eq!(inline_aot.inline_policy, AotInlinePolicy::Always);

    let noinline_aot = aot
        .functions
        .iter()
        .find(|func| func.name == "wrapped_noinline_aot_4286")
        .expect("wrapped noinline function should convert to AoT IR");
    assert_eq!(noinline_aot.inline_policy, AotInlinePolicy::Never);
}

#[test]
fn test_convert_function_flattens_let_block_assign_expr() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "timed_like".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings: vec![],
                    body: Block {
                        stmts: vec![
                            Stmt::Assign {
                                var: "#result#1".to_string(),
                                value: Expr::AssignExpr {
                                    var: "grid".to_string(),
                                    value: Box::new(Expr::Literal(Literal::Int(7), test_span())),
                                    span: test_span(),
                                },
                                span: test_span(),
                            },
                            Stmt::Expr {
                                expr: Expr::Call {
                                    function: "println".to_string(),
                                    args: vec![Expr::Var("#elapsed_s#3".to_string(), test_span())],
                                    kwargs: vec![],
                                    splat_mask: vec![false],
                                    kwargs_splat_mask: vec![],
                                    span: test_span(),
                                },
                                span: test_span(),
                            },
                            Stmt::Expr {
                                expr: Expr::Var("#result#1".to_string(), test_span()),
                                span: test_span(),
                            },
                        ],
                        span: test_span(),
                    },
                    span: test_span(),
                },
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.body.len(), 2);
    assert!(
        matches!(&result.body[0], AotStmt::Let { .. }),
        "Expected flattened let assignment to grid, got {:?}",
        result.body[0]
    );
    if let AotStmt::Let { name, .. } = &result.body[0] {
        assert_eq!(name, "grid");
    }
    assert!(
        matches!(
            &result.body[1],
            AotStmt::Expr(AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Println,
                ..
            })
        ),
        "Expected preserved side-effecting println call, got {:?}",
        result.body[1]
    );
}

#[test]
fn statement_letblock_drops_plain_result_alias_issue_8499() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "timed_plain_result".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::LetBlock {
                        bindings: vec![],
                        body: Block {
                            stmts: vec![
                                Stmt::Assign {
                                    var: "result".to_string(),
                                    value: Expr::AssignExpr {
                                        var: "xs".to_string(),
                                        value: Box::new(Expr::ArrayLiteral {
                                            elements: vec![
                                                Expr::Literal(Literal::Int(1), test_span()),
                                                Expr::Literal(Literal::Int(2), test_span()),
                                            ],
                                            shape: vec![2],
                                            span: test_span(),
                                        }),
                                        span: test_span(),
                                    },
                                    span: test_span(),
                                },
                                Stmt::Expr {
                                    expr: Expr::Call {
                                        function: "println".to_string(),
                                        args: vec![Expr::Literal(
                                            Literal::Str(" seconds".to_string()),
                                            test_span(),
                                        )],
                                        kwargs: vec![],
                                        splat_mask: vec![false],
                                        kwargs_splat_mask: vec![],
                                        span: test_span(),
                                    },
                                    span: test_span(),
                                },
                                Stmt::Expr {
                                    expr: Expr::Var("result".to_string(), test_span()),
                                    span: test_span(),
                                },
                            ],
                            span: test_span(),
                        },
                        span: test_span(),
                    },
                    span: test_span(),
                },
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "println".to_string(),
                        args: vec![Expr::Index {
                            array: Box::new(Expr::Var("xs".to_string(), test_span())),
                            indices: vec![Expr::Literal(Literal::Int(1), test_span())],
                            span: test_span(),
                        }],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span: test_span(),
                    },
                    span: test_span(),
                },
            ],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.body.len(), 3);
    assert!(
        !result
            .body
            .iter()
            .any(|stmt| matches!(stmt, AotStmt::Let { name, .. } if name == "result")),
        "statement-position @time passthrough slot must not move the assigned value: {:?}",
        result.body
    );
    assert!(matches!(
        &result.body[0],
        AotStmt::Let { name, .. } if name == "xs"
    ));
}

#[test]
fn test_convert_materialized_broadcast_mul_to_helper_call() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    converter.engine.env.insert(
        "im".to_string(),
        StaticType::Struct {
            type_id: 0,
            name: "Complex64".to_string(),
        },
    );
    converter.engine.env.insert(
        "ys".to_string(),
        StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(1),
        },
    );

    let expr = Expr::Call {
        function: "materialize".to_string(),
        args: vec![Expr::Call {
            function: "Broadcasted".to_string(),
            args: vec![
                Expr::FunctionRef {
                    name: "*".to_string(),
                    span: test_span(),
                },
                Expr::TupleLiteral {
                    elements: vec![
                        Expr::Var("im".to_string(), test_span()),
                        Expr::Var("ys".to_string(), test_span()),
                    ],
                    span: test_span(),
                },
            ],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::CallStatic { .. }),
        "Expected CallStatic helper, got {:?}",
        result
    );
    if let AotExpr::CallStatic { function, args, .. } = result {
        assert_eq!(function, "__aot_broadcast_mul_scalar_vec");
        assert_eq!(args.len(), 3);
    }
}

#[test]
fn test_convert_builtin_call() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Call {
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Float(4.0), test_span())],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    let result = converter.convert_expr(&expr).unwrap();
    assert!(
        matches!(&result, AotExpr::CallBuiltin { .. }),
        "Expected CallBuiltin, got {:?}",
        result
    );
    if let AotExpr::CallBuiltin {
        builtin, return_ty, ..
    } = result
    {
        assert_eq!(builtin, AotBuiltinOp::Sqrt);
        assert_eq!(return_ty, StaticType::F64);
    }
}

#[test]
fn test_convert_expr_call_includes_kwargs_in_static_dispatch() {
    let mut typed = TypedProgram::new();
    let sig = FunctionSignature::new(
        "range".to_string(),
        vec![
            "start".to_string(),
            "stop".to_string(),
            "length".to_string(),
        ],
        vec![StaticType::F64, StaticType::F64, StaticType::I64],
        StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(1),
        },
    );
    typed.add_function(TypedFunction::new(sig));

    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

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

    let result = converter.convert_expr(&expr).unwrap();
    // range(start, stop; length=n) is now intercepted as Linspace builtin (Issue #3413)
    assert!(
        matches!(
            &result,
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Linspace,
                ..
            }
        ),
        "Expected CallBuiltin Linspace, got {:?}",
        result
    );
    if let AotExpr::CallBuiltin { args, .. } = result {
        assert_eq!(args.len(), 3);
        assert!(matches!(args[2], AotExpr::LitI64(50)));
    }
}

#[test]
fn test_convert_expr_callsite_inline_policy_wrapper() {
    let mut typed = TypedProgram::new();
    let sig = FunctionSignature::new(
        "callee_inline_policy_4286".to_string(),
        vec!["x".to_string()],
        vec![StaticType::I64],
        StaticType::I64,
    );
    typed.add_function(TypedFunction::new(sig));

    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Call {
        function: "#__sjulia_inline__".to_string(),
        args: vec![Expr::Call {
            function: "callee_inline_policy_4286".to_string(),
            args: vec![Expr::Literal(Literal::Int(1), test_span())],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    assert!(matches!(
        result,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Always,
            ..
        }
    ));
}

#[test]
fn test_convert_expr_callsite_nested_policy_uses_innermost() {
    let mut typed = TypedProgram::new();
    let sig = FunctionSignature::new(
        "callee_nested_policy_4286".to_string(),
        vec!["x".to_string()],
        vec![StaticType::I64],
        StaticType::I64,
    );
    typed.add_function(TypedFunction::new(sig));

    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let inner = Expr::Call {
        function: "#__sjulia_noinline__".to_string(),
        args: vec![Expr::Call {
            function: "callee_nested_policy_4286".to_string(),
            args: vec![Expr::Literal(Literal::Int(1), test_span())],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    let outer = Expr::Call {
        function: "#__sjulia_inline__".to_string(),
        args: vec![inner],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&outer).unwrap();
    assert!(matches!(
        result,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Never,
            ..
        }
    ));
}

#[test]
fn test_convert_expr_callsite_policy_applies_to_nested_static_calls() {
    let mut typed = TypedProgram::new();
    for name in ["callee_block_policy_a_4286", "callee_block_policy_b_4286"] {
        let sig = FunctionSignature::new(
            name.to_string(),
            vec!["x".to_string()],
            vec![StaticType::I64],
            StaticType::I64,
        );
        typed.add_function(TypedFunction::new(sig));
    }

    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let expr = Expr::Call {
        function: "#__sjulia_inline__".to_string(),
        args: vec![Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(Expr::Call {
                function: "callee_block_policy_a_4286".to_string(),
                args: vec![Expr::Literal(Literal::Int(1), test_span())],
                kwargs: vec![],
                splat_mask: vec![false],
                kwargs_splat_mask: vec![],
                span: test_span(),
            }),
            right: Box::new(Expr::Call {
                function: "callee_block_policy_b_4286".to_string(),
                args: vec![Expr::Literal(Literal::Int(2), test_span())],
                kwargs: vec![],
                splat_mask: vec![false],
                kwargs_splat_mask: vec![],
                span: test_span(),
            }),
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    let AotExpr::BinOpStatic { left, right, .. } = result else {
        panic!("expected BinOpStatic with annotated static calls");
    };
    assert!(matches!(
        *left,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Always,
            ..
        }
    ));
    assert!(matches!(
        *right,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Always,
            ..
        }
    ));
}

#[test]
fn test_convert_expr_callsite_policy_keeps_inner_policy_in_block() {
    let mut typed = TypedProgram::new();
    for name in [
        "callee_block_inner_policy_a_4286",
        "callee_block_inner_policy_b_4286",
    ] {
        let sig = FunctionSignature::new(
            name.to_string(),
            vec!["x".to_string()],
            vec![StaticType::I64],
            StaticType::I64,
        );
        typed.add_function(TypedFunction::new(sig));
    }

    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    let left_inner = Expr::Call {
        function: "#__sjulia_noinline__".to_string(),
        args: vec![Expr::Call {
            function: "callee_block_inner_policy_a_4286".to_string(),
            args: vec![Expr::Literal(Literal::Int(1), test_span())],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };
    let expr = Expr::Call {
        function: "#__sjulia_inline__".to_string(),
        args: vec![Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(left_inner),
            right: Box::new(Expr::Call {
                function: "callee_block_inner_policy_b_4286".to_string(),
                args: vec![Expr::Literal(Literal::Int(2), test_span())],
                kwargs: vec![],
                splat_mask: vec![false],
                kwargs_splat_mask: vec![],
                span: test_span(),
            }),
            span: test_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: test_span(),
    };

    let result = converter.convert_expr(&expr).unwrap();
    let AotExpr::BinOpStatic { left, right, .. } = result else {
        panic!("expected BinOpStatic with annotated static calls");
    };
    assert!(matches!(
        *left,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Never,
            ..
        }
    ));
    assert!(matches!(
        *right,
        AotExpr::CallStatic {
            inline_policy: AotInlinePolicy::Always,
            ..
        }
    ));
}

#[test]
fn test_convert_simple_function() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "add".to_string(),
        params: vec![
            TypedParam::new("x".to_string(), None, test_span()),
            TypedParam::new("y".to_string(), None, test_span()),
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string(), test_span())),
                    right: Box::new(Expr::Var("y".to_string(), test_span())),
                    span: test_span(),
                }),
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.name, "add");
    assert_eq!(result.params.len(), 2);
    assert_eq!(result.body.len(), 1);
}

#[test]
fn implicit_function_body_result_converts_to_aot_expr_issue_7010() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let mut converter = IrConverter::new(&typed, &program);

    let func = Function {
        name: "add".to_string(),
        params: vec![
            TypedParam::new("x".to_string(), Some(JuliaType::Int64), test_span()),
            TypedParam::new("y".to_string(), Some(JuliaType::Int64), test_span()),
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string(), test_span())),
                    right: Box::new(Expr::Var("y".to_string(), test_span())),
                    span: test_span(),
                },
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let result = converter.convert_function(&func).unwrap();
    assert_eq!(result.body.len(), 1);
    assert!(
        matches!(result.body[0], AotStmt::Expr(AotExpr::BinOpStatic { .. })),
        "implicit final expression should convert directly, got {:?}",
        result.body[0]
    );
}

#[test]
fn test_convert_lambda_function_ref() {
    let typed = TypedProgram::new();

    // Create a program with a lambda function: __lambda_0__ = x -> x + 1
    let lambda_func = Function {
        name: "__lambda_0__".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, test_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string(), test_span())),
                    right: Box::new(Expr::Literal(Literal::Int(1), test_span())),
                    span: test_span(),
                }),
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![lambda_func],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let converter = IrConverter::new(&typed, &program);

    // Convert a FunctionRef pointing to the lambda
    let func_ref = Expr::FunctionRef {
        name: "__lambda_0__".to_string(),
        span: test_span(),
    };

    let result = converter.convert_expr(&func_ref).unwrap();
    assert!(
        matches!(&result, AotExpr::Lambda { .. }),
        "Expected Lambda, got {:?}",
        result
    );
    if let AotExpr::Lambda {
        params, captures, ..
    } = result
    {
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "x");
        // No captures since x is a parameter
        assert!(captures.is_empty());
    }
}

#[test]
fn test_convert_lambda_with_capture() {
    let typed = TypedProgram::new();

    // Create a program with a lambda that captures 'a': __lambda_0__ = x -> x + a
    let lambda_func = Function {
        name: "__lambda_0__".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, test_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string(), test_span())),
                    right: Box::new(Expr::Var("a".to_string(), test_span())),
                    span: test_span(),
                }),
                span: test_span(),
            }],
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![lambda_func],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let mut converter = IrConverter::new(&typed, &program);
    // Set up outer scope with variable 'a'
    converter
        .engine
        .env
        .insert("a".to_string(), StaticType::I64);

    // Convert a FunctionRef pointing to the lambda
    let func_ref = Expr::FunctionRef {
        name: "__lambda_0__".to_string(),
        span: test_span(),
    };

    let result = converter.convert_expr(&func_ref).unwrap();
    assert!(
        matches!(&result, AotExpr::Lambda { .. }),
        "Expected Lambda, got {:?}",
        result
    );
    if let AotExpr::Lambda {
        params, captures, ..
    } = result
    {
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "x");
        // Should capture 'a'
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].0, "a");
        assert_eq!(captures[0].1, StaticType::I64);
    }
}

#[test]
fn lambda_multi_statement_body_rejects_with_span_issue_6938() {
    let typed = TypedProgram::new();
    let body_span = Span::new(10, 40, 2, 3, 5, 8);

    let lambda_func = Function {
        name: "__lambda_0__".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, test_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string(), test_span())),
                        right: Box::new(Expr::Literal(Literal::Int(1), test_span())),
                        span: test_span(),
                    },
                    span: test_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string(), test_span())),
                    span: test_span(),
                },
            ],
            span: body_span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    };

    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![lambda_func],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };
    let converter = IrConverter::new(&typed, &program);
    let func_ref = Expr::FunctionRef {
        name: "__lambda_0__".to_string(),
        span: test_span(),
    };

    let err = converter.convert_expr(&func_ref).unwrap_err();

    match err {
        crate::aot::AotError::UnsupportedInstruction(diagnostic) => {
            assert!(
                diagnostic.message.contains("multi-statement body"),
                "unexpected message: {}",
                diagnostic.message
            );
            assert_eq!(diagnostic.span, Some(body_span));
            assert!(diagnostic
                .workaround
                .as_deref()
                .is_some_and(|workaround| workaround.contains("single expression")));
        }
        other => panic!("expected UnsupportedInstruction, got {other:?}"),
    }
}

#[test]
fn test_is_lambda_function() {
    let typed = TypedProgram::new();
    let program = empty_program();
    let converter = IrConverter::new(&typed, &program);

    assert!(converter.is_lambda_function("__lambda_0__"));
    assert!(converter.is_lambda_function("__lambda_123__"));
    assert!(!converter.is_lambda_function("regular_function"));
    assert!(!converter.is_lambda_function("lambda"));
    assert!(!converter.is_lambda_function("_lambda_0__"));
}

// ========== Core IR Analyzer Tests ==========

fn make_call_expr(name: &str) -> Expr {
    Expr::Call {
        function: name.to_string(),
        args: vec![],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: test_span(),
    }
}

fn make_function(name: &str, calls: Vec<&str>) -> Function {
    let stmts: Vec<Stmt> = calls
        .into_iter()
        .map(|c| Stmt::Expr {
            expr: make_call_expr(c),
            span: test_span(),
        })
        .collect();

    Function {
        name: name.to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts,
            span: test_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: test_span(),
    }
}

#[test]
fn test_analyzer_find_functions() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![make_function("foo", vec![]), make_function("bar", vec![])],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let result = analyzer.analyze_program(&program);

    assert_eq!(result.functions.len(), 2);
    assert!(result.get_function("foo").is_some());
    assert!(result.get_function("bar").is_some());
}

#[test]
fn test_analyzer_call_graph() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![
            make_function("foo", vec!["bar", "baz"]),
            make_function("bar", vec!["baz"]),
            make_function("baz", vec![]),
        ],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![Stmt::Expr {
                expr: make_call_expr("foo"),
                span: test_span(),
            }],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let result = analyzer.analyze_program(&program);

    // Check foo's calls
    let foo_info = result.get_function("foo").unwrap();
    assert!(foo_info.calls.contains("bar"));
    assert!(foo_info.calls.contains("baz"));

    // Check bar's calls
    let bar_info = result.get_function("bar").unwrap();
    assert!(bar_info.calls.contains("baz"));

    // Check baz has no calls
    let baz_info = result.get_function("baz").unwrap();
    assert!(baz_info.calls.is_empty());
}

#[test]
fn test_analyzer_direct_recursion() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![
            make_function("factorial", vec!["factorial"]), // Calls itself
            make_function("normal", vec![]),
        ],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let result = analyzer.analyze_program(&program);

    // factorial should be detected as recursive
    let factorial_info = result.get_function("factorial").unwrap();
    assert!(factorial_info.is_recursive);

    // normal should not be recursive
    let normal_info = result.get_function("normal").unwrap();
    assert!(!normal_info.is_recursive);
}

#[test]
fn test_analyzer_mutual_recursion() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![
            make_function("is_even", vec!["is_odd"]), // A calls B
            make_function("is_odd", vec!["is_even"]), // B calls A
        ],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let result = analyzer.analyze_program(&program);

    // Both should be detected as recursive due to mutual recursion
    let is_even_info = result.get_function("is_even").unwrap();
    assert!(is_even_info.is_recursive);

    let is_odd_info = result.get_function("is_odd").unwrap();
    assert!(is_odd_info.is_recursive);
}

#[test]
fn test_analyzer_entry_point() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![
            make_function("main_func", vec![]),
            make_function("helper", vec![]),
        ],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![Stmt::Expr {
                expr: make_call_expr("main_func"),
                span: test_span(),
            }],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let result = analyzer.analyze_program(&program);

    // Entry point should be the first function called from main
    assert_eq!(result.entry_point, Some("main_func".to_string()));
}

#[test]
fn test_analyzer_get_call_graph() {
    let program = Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: vec![
            make_function("a", vec!["b", "c"]),
            make_function("b", vec!["c"]),
            make_function("c", vec![]),
        ],
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: Block {
            stmts: vec![],
            span: test_span(),
        },
    };

    let mut analyzer = CoreIrAnalyzer::new();
    let _ = analyzer.analyze_program(&program);

    let call_graph = analyzer.get_call_graph();

    // Check the call graph edges
    assert!(call_graph.get("a").unwrap().contains("b"));
    assert!(call_graph.get("a").unwrap().contains("c"));
    assert!(call_graph.get("b").unwrap().contains("c"));
    assert!(call_graph.get("c").unwrap().is_empty());
}
