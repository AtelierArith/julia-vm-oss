use super::*;
use crate::aot::codegen::CAbiExport;
use crate::aot::ir::{
    AotBinOp, AotBuiltinOp, AotEnum, AotExpr, AotFunction, AotGlobal, AotInlinePolicy, AotProgram,
    AotStmt, AotStruct, AotUnaryOp, CompoundAssignOp,
};
use crate::aot::types::StaticType;

fn generated_pub_fn_sections(generated: &str, prefixes: &[&str]) -> String {
    let mut sections = String::new();
    let mut lines = generated.lines();

    while let Some(line) = lines.next() {
        if !prefixes.iter().any(|prefix| line.starts_with(prefix)) {
            continue;
        }

        if !sections.is_empty() {
            sections.push('\n');
        }
        sections.push_str(line);
        sections.push('\n');

        for line in lines.by_ref() {
            sections.push_str(line);
            sections.push('\n');
            if line == "}" {
                break;
            }
        }
    }

    sections.trim_end().to_string()
}

#[test]
fn test_aot_codegen_literal_expressions() {
    let codegen = AotCodeGenerator::default_config();

    // Integer literal
    let expr = AotExpr::LitI64(42);
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "42i64");

    // Float literal
    let expr = AotExpr::LitF64(1.25);
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("1.25"));

    // Bool literal
    let expr = AotExpr::LitBool(true);
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "true");

    // String literal
    let expr = AotExpr::LitStr("hello".to_string());
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("hello"));

    // Nothing literal
    let expr = AotExpr::LitNothing;
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "()");

    // Missing literal
    let expr = AotExpr::LitMissing;
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "Value::Missing");
}

#[test]
fn nothing_and_nullable_union_codegen_issue_6979() {
    let codegen = AotCodeGenerator::default_config();
    assert_eq!(
        codegen.emit_expr_to_string(&AotExpr::LitNothing).unwrap(),
        "()"
    );

    let nullable_i64 = StaticType::Union {
        variants: vec![StaticType::I64, StaticType::Nothing],
    };

    let mut program = AotProgram::new();
    let mut noop = AotFunction::new("noop".to_string(), vec![], StaticType::Nothing);
    noop.body.push(AotStmt::Return(Some(AotExpr::LitNothing)));
    program.add_function(noop);

    let mut maybe = AotFunction::new(
        "maybe".to_string(),
        vec![("flag".to_string(), StaticType::Bool)],
        nullable_i64.clone(),
    );
    maybe.body.push(AotStmt::Expr(AotExpr::Ternary {
        condition: Box::new(AotExpr::Var {
            name: "flag".to_string(),
            ty: StaticType::Bool,
        }),
        then_expr: Box::new(AotExpr::LitI64(1)),
        else_expr: Box::new(AotExpr::LitNothing),
        result_ty: nullable_i64.clone(),
    }));
    program.add_function(maybe);

    let mut explicit_nothing =
        AotFunction::new("explicit_nothing".to_string(), vec![], nullable_i64);
    explicit_nothing
        .body
        .push(AotStmt::Return(Some(AotExpr::LitNothing)));
    program.add_function(explicit_nothing);

    let mut codegen = AotCodeGenerator::default_config();
    let generated = codegen.generate_program(&program).unwrap();
    assert!(generated.contains("pub fn noop() -> ()"));
    assert!(generated.contains("return ();"));
    assert!(generated.contains("pub fn maybe(flag: bool) -> Value"));
    assert!(
        generated.contains("if flag { Value::from(1i64) } else { Value::from(()) }"),
        "{generated}"
    );
    assert!(generated.contains("pub fn explicit_nothing() -> Value"));
    assert!(generated.contains("return Value::from(());"));
}

#[test]
fn char_literals_escape_valid_unicode_scalars_issue_6967() {
    let codegen = AotCodeGenerator::default_config();

    for (expr, expected) in [
        (AotExpr::LitChar('\''), r#"'\''"#),
        (AotExpr::LitChar('\\'), r#"'\\'"#),
        (AotExpr::LitChar('\n'), r#"'\n'"#),
        (AotExpr::LitChar('\u{e9}'), r#"'\u{e9}'"#),
        (AotExpr::LitChar('\u{1f600}'), r#"'\u{1f600}'"#),
    ] {
        assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), expected);
    }

    let invalid_julia_char = AotExpr::Convert {
        value: Box::new(AotExpr::LitI64(0xd800)),
        target_ty: StaticType::Char,
    };
    let err = codegen
        .emit_expr_to_string(&invalid_julia_char)
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("invalid Unicode code points"), "{message}");
    assert!(message.contains("Issue #6967"), "{message}");
}

#[test]
fn test_aot_codegen_variable() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "x");
}

#[test]
fn typeof_codegen_emits_julia_datatype_carrier_issue_7015() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TypeOf,
        args: vec![AotExpr::LitI64(1)],
        return_ty: StaticType::DataType,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "Value::DataType(\"Int64\".to_string())");
}

#[test]
fn typeof_codegen_uses_runtime_value_type_name_for_any_issue_7015() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TypeOf,
        args: vec![AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::Any,
        }],
        return_ty: StaticType::DataType,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "Value::DataType(x.type_name().to_string())");
}

#[test]
fn typeof_codegen_uses_runtime_value_type_name_for_union_issue_7075() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TypeOf,
        args: vec![AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::Union {
                variants: vec![StaticType::I64, StaticType::Str],
            },
        }],
        return_ty: StaticType::DataType,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "Value::DataType(x.type_name().to_string())");
}

#[test]
fn datatype_field_access_is_gated_issue_7068() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::FieldAccess {
        object: Box::new(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::TypeOf,
            args: vec![AotExpr::LitI64(1)],
            return_ty: StaticType::DataType,
        }),
        field: "parameters".to_string(),
        field_ty: StaticType::Any,
    };

    let err = codegen.emit_expr_to_string(&expr).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("DataType field access `.parameters`"),
        "{message}"
    );
    assert!(message.contains("Issue #7068"), "{message}");
}

#[test]
fn complex_im_uses_lexical_shadowing_issue_6966() {
    let codegen = AotCodeGenerator::default_config();
    let complex_ty = StaticType::Struct {
        type_id: 0,
        name: "Complex".to_string(),
    };

    let global_im = AotExpr::Var {
        name: "im".to_string(),
        ty: complex_ty.clone(),
    };
    assert_eq!(codegen.emit_expr_to_string(&global_im).unwrap(), "im");

    let local_im = AotExpr::Var {
        name: "im".to_string(),
        ty: StaticType::I64,
    };
    assert_eq!(codegen.emit_expr_to_string(&local_im).unwrap(), "im");

    let mut program = AotProgram::new();
    let mut complex = AotStruct::new("Complex".to_string(), false);
    complex.add_field("re".to_string(), StaticType::F64);
    complex.add_field("im".to_string(), StaticType::F64);
    program.add_struct(complex);

    let mut echo = AotFunction::new(
        "echo_im".to_string(),
        vec![("im".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    echo.body.push(AotStmt::Return(Some(AotExpr::Var {
        name: "im".to_string(),
        ty: StaticType::I64,
    })));
    program.add_function(echo);
    program.main.push(AotStmt::Expr(global_im));

    let mut codegen = AotCodeGenerator::default_config();
    let generated = codegen.generate_program(&program).unwrap();
    assert!(generated.contains("const im: Complex = Complex::<f64> { re: 0.0, im: 1.0 };"));
    assert!(!generated.contains("const IM: Complex"));
    assert!(generated.contains("pub fn echo_im(im: i64) -> i64"));
    assert!(generated.contains("return im;"));
    assert!(generated.contains("    im;"));
    assert!(!generated.contains("return IM;"));
}

#[test]
fn escape_rust_ident_covers_keywords_issue_6934() {
    for keyword in [
        "as",
        "async",
        "await",
        "dyn",
        "fn",
        "gen",
        "macro_rules",
        "try",
        "union",
        "yield",
    ] {
        assert_eq!(escape_rust_ident(keyword), format!("r#{}", keyword));
    }

    for keyword in ["self", "super", "crate", "Self"] {
        assert_eq!(escape_rust_ident(keyword), format!("_{}", keyword));
    }

    assert_eq!(escape_rust_ident("1abc"), "_1abc");
    assert_eq!(escape_rust_ident("has-dash"), "has_dash");
}

#[test]
fn test_aot_codegen_binary_op() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::LitI64(1)),
        right: Box::new(AotExpr::LitI64(2)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("1i64"));
    assert!(result.contains("wrapping_add"));
    assert!(result.contains("2i64"));
}

#[test]
fn int64_arithmetic_emits_wrapping_ops_issue_6940() {
    let codegen = AotCodeGenerator::default_config();

    for (op, expected) in [
        (AotBinOp::Add, "wrapping_add"),
        (AotBinOp::Sub, "wrapping_sub"),
        (AotBinOp::Mul, "wrapping_mul"),
    ] {
        let expr = AotExpr::BinOpStatic {
            op,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        };

        let result = codegen.emit_expr_to_string(&expr).unwrap();
        assert_eq!(result, format!("(x).{}(y)", expected));
    }
}

#[test]
fn subtype_operator_is_gated_issue_6936() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Subtype,
        left: Box::new(AotExpr::Var {
            name: "Int".to_string(),
            ty: StaticType::Any,
        }),
        right: Box::new(AotExpr::Var {
            name: "Number".to_string(),
            ty: StaticType::Any,
        }),
        result_ty: StaticType::Bool,
    };

    let err = codegen.emit_expr_to_string(&expr).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("subtype operator"));
    assert!(message.contains("<:"));
}

#[test]
fn test_aot_codegen_unary_op() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::UnaryOp {
        op: AotUnaryOp::Neg,
        operand: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("-"));
    assert!(result.contains("5i64"));
}

#[test]
fn checked_numeric_conversions_gate_unsafe_rust_as_issue_6968() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::Convert {
        value: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I32,
        }),
        target_ty: StaticType::I64,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "(x as i64)");

    let expr = AotExpr::Convert {
        value: Box::new(AotExpr::Var {
            name: "flag".to_string(),
            ty: StaticType::Bool,
        }),
        target_ty: StaticType::F64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(flag as u8 as f64)"
    );

    // float→int / narrowing / sign / numeric→Bool now emit InexactError-checked
    // conversions instead of being gated (Issue #7038).
    for (value, target_ty, marker) in [
        (AotExpr::LitF64(1.5), StaticType::I64, "InexactError: Int64"),
        (AotExpr::LitF32(1.5), StaticType::I32, "InexactError: Int32"),
        (
            AotExpr::Var {
                name: "wide".to_string(),
                ty: StaticType::I64,
            },
            StaticType::I32,
            "InexactError: trunc(Int32",
        ),
        (
            AotExpr::Var {
                name: "unsigned".to_string(),
                ty: StaticType::U64,
            },
            StaticType::I64,
            "InexactError: trunc(Int64",
        ),
        (
            AotExpr::Var {
                name: "count".to_string(),
                ty: StaticType::I64,
            },
            StaticType::Bool,
            "InexactError: Bool",
        ),
    ] {
        let expr = AotExpr::Convert {
            value: Box::new(value),
            target_ty,
        };
        let code = codegen
            .emit_expr_to_string(&expr)
            .expect("checked conversion must now compile");
        assert!(code.contains(marker), "expected `{marker}` in: {code}");
    }

    // fptosi now lowers to the same InexactError-checked float→int conversion
    // (Issue #7038).
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Fptosi,
        args: vec![AotExpr::LitF64(1.5)],
        return_ty: StaticType::I64,
    };
    let code = codegen
        .emit_expr_to_string(&expr)
        .expect("fptosi must now lower to a checked conversion");
    assert!(
        code.contains("InexactError: Int64"),
        "expected checked fptosi, got: {code}"
    );
}

#[test]
fn test_aot_codegen_function_call() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::CallStatic {
        function: "add".to_string(),
        args: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("add(1i64, 2i64)"));
}

#[test]
fn test_aot_codegen_builtin_call() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Sqrt,
        args: vec![AotExpr::LitF64(4.0)],
        return_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("_sjulia_sqrt_arg < 0.0_f64"));
    assert!(result.contains("RuntimeError::domain_error"));
    assert!(result.contains(".sqrt()"));

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Log,
        args: vec![AotExpr::LitF64(1.0)],
        return_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("_sjulia_log_arg < 0.0_f64"));
    assert!(result.contains("RuntimeError::domain_error"));
    assert!(result.contains(".ln()"));
}

#[test]
fn array_shape_builtins_use_static_rank_issue_6959() {
    let codegen = AotCodeGenerator::default_config();
    let matrix_ty = StaticType::Array {
        element: Box::new(StaticType::F64),
        ndims: Some(2),
    };
    let matrix = || AotExpr::Var {
        name: "mat".to_string(),
        ty: matrix_ty.clone(),
    };

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Length,
        args: vec![matrix()],
        return_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_arr = &mat; (_sjulia_arr.len() as i64) * if _sjulia_arr.is_empty() { 0i64 } else { _sjulia_arr[0].len() as i64 } }"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Size,
        args: vec![matrix()],
        return_ty: StaticType::Tuple(vec![StaticType::I64, StaticType::I64]),
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_arr = &mat; (_sjulia_arr.len() as i64, if _sjulia_arr.is_empty() { 0i64 } else { _sjulia_arr[0].len() as i64 }) }"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Size,
        args: vec![matrix(), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_arr = &mat; let _sjulia_dim = 2i64; if _sjulia_dim < 1 { subset_julia_vm_runtime::error::aot_throw(\"Dimension out of range\"); } else if _sjulia_dim == 1 { _sjulia_arr.len() as i64 } else if _sjulia_dim == 2 { if _sjulia_arr.is_empty() { 0i64 } else { _sjulia_arr[0].len() as i64 } } else { 1i64 } }"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Ndims,
        args: vec![matrix()],
        return_ty: StaticType::I64,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "2i64");

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Length,
        args: vec![AotExpr::Var {
            name: "tensor".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(3),
            },
        }],
        return_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_arr_0 = &tensor; let _sjulia_dim_0 = _sjulia_arr_0.len(); let _sjulia_dim_1 = if _sjulia_dim_0 == 0usize { 0usize } else { _sjulia_arr_0[0].len() }; let _sjulia_dim_2 = if _sjulia_dim_0 == 0usize || _sjulia_dim_1 == 0usize { 0usize } else { _sjulia_arr_0[0][0].len() }; (_sjulia_dim_0 * _sjulia_dim_1 * _sjulia_dim_2) as i64 }"
    );
}

#[test]
fn array_shape_uses_static_rank_not_runtime_probe_issue_6961() {
    let codegen = AotCodeGenerator::default_config();
    let matrix_ty = StaticType::Array {
        element: Box::new(StaticType::F64),
        ndims: Some(2),
    };

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Length,
        args: vec![AotExpr::Var {
            name: "flat_name".to_string(),
            ty: matrix_ty.clone(),
        }],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("let _sjulia_arr = &flat_name"), "{result}");
    assert!(
        result.contains("_sjulia_arr[0].len()"),
        "2D rank should select the matrix path without inspecting the source spelling: {result}"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Ndims,
        args: vec![AotExpr::Var {
            name: "flat_name".to_string(),
            ty: matrix_ty,
        }],
        return_ty: StaticType::I64,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "2i64");

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Length,
        args: vec![AotExpr::Var {
            name: "maybe_nested_vec".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: None,
            },
        }],
        return_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "maybe_nested_vec.len() as i64"
    );
}

#[test]
fn three_dimensional_arrays_codegen_supported_issue_7843() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::ArrayLit {
        elements: (1..=8).map(AotExpr::LitI64).collect(),
        shape: vec![2, 2, 2],
        elem_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "vec![vec![vec![1i64, 5i64], vec![3i64, 7i64]], vec![vec![2i64, 6i64], vec![4i64, 8i64]]]"
    );

    let tensor_ty = StaticType::Array {
        element: Box::new(StaticType::F64),
        ndims: Some(3),
    };
    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "tensor".to_string(),
            ty: tensor_ty,
        }),
        indices: vec![AotExpr::LitI64(1), AotExpr::LitI64(1), AotExpr::LitI64(1)],
        elem_ty: StaticType::F64,
        is_tuple: false,
    };
    let emitted = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(
        emitted.contains("let _sjulia_arr_0 = &tensor;"),
        "{emitted}"
    );
    assert!(emitted.contains("let _sjulia_idx_2 = 1i64;"), "{emitted}");
    assert!(
        emitted.contains("BoundsError({:?}, ({}, {}, {}))"),
        "{emitted}"
    );
    assert!(
        emitted.contains("_sjulia_arr_2[(_sjulia_idx_2 - 1) as usize].clone()"),
        "{emitted}"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Zeros,
        args: vec![AotExpr::LitI64(2), AotExpr::LitI64(2), AotExpr::LitI64(2)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(3),
        },
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "vec![vec![vec![0.0_f64; 2i64 as usize]; 2i64 as usize]; 2i64 as usize]"
    );
}

#[test]
fn random_builtins_use_runtime_rng_contract_issue_7036() {
    let codegen = AotCodeGenerator::default_config();

    let rand_expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Rand,
        args: vec![],
        return_ty: StaticType::F64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&rand_expr).unwrap(),
        "__sjulia_aot_rand()"
    );

    let randn_expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Randn,
        args: vec![],
        return_ty: StaticType::F64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&randn_expr).unwrap(),
        "__sjulia_aot_randn()"
    );

    let rand_vec = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Rand,
        args: vec![AotExpr::LitI64(3)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(1),
        },
    };
    let emitted = codegen.emit_expr_to_string(&rand_vec).unwrap();
    assert!(emitted.contains("__sjulia_aot_rand()"), "{emitted}");
    assert!(emitted.contains("collect::<Vec<_>>()"), "{emitted}");

    let randn_mat = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Randn,
        args: vec![AotExpr::LitI64(2), AotExpr::LitI64(2)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(2),
        },
    };
    let emitted = codegen.emit_expr_to_string(&randn_mat).unwrap();
    assert!(emitted.contains("__sjulia_aot_randn()"), "{emitted}");
    assert!(
        emitted.matches("collect::<Vec<_>>()").count() >= 2,
        "{emitted}"
    );
}

#[test]
fn zeros_ones_preserve_declared_element_type_issue_6956() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Zeros,
        args: vec![AotExpr::LitI64(3)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(1),
        },
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "vec![0i32; 3i64 as usize]"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Ones,
        args: vec![AotExpr::LitI64(2), AotExpr::LitI64(4)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::U8),
            ndims: Some(2),
        },
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(0..2i64 as usize).map(|_| vec![1u8; 4i64 as usize]).collect::<Vec<_>>()"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Zeros,
        args: vec![AotExpr::LitI64(2)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::F32),
            ndims: Some(1),
        },
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "vec![0.0_f32; 2i64 as usize]"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Ones,
        args: vec![AotExpr::LitI64(2)],
        return_ty: StaticType::Array {
            element: Box::new(StaticType::Bool),
            ndims: Some(1),
        },
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "vec![true; 2i64 as usize]"
    );
}

#[test]
fn map_filter_clone_non_copy_elements_issue_6957_6958() {
    let codegen = AotCodeGenerator::default_config();
    let string_vec_ty = StaticType::Array {
        element: Box::new(StaticType::Str),
        ndims: Some(1),
    };

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Map,
        args: vec![
            AotExpr::Var {
                name: "normalize".to_string(),
                ty: StaticType::Function {
                    params: vec![StaticType::Str],
                    ret: Box::new(StaticType::Str),
                },
            },
            AotExpr::Var {
                name: "names".to_string(),
                ty: string_vec_ty.clone(),
            },
        ],
        return_ty: string_vec_ty.clone(),
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "names.iter().cloned().map(|x| normalize(x)).collect::<Vec<_>>()"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Filter,
        args: vec![
            AotExpr::Var {
                name: "keep".to_string(),
                ty: StaticType::Function {
                    params: vec![StaticType::Str],
                    ret: Box::new(StaticType::Bool),
                },
            },
            AotExpr::Var {
                name: "names".to_string(),
                ty: string_vec_ty.clone(),
            },
        ],
        return_ty: string_vec_ty,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "names.iter().cloned().filter(|x| keep((*x).clone())).collect::<Vec<_>>()"
    );
}

#[test]
fn tuple_first_last_use_tuple_fields_issue_6963() {
    let codegen = AotCodeGenerator::default_config();
    let tuple_ty = StaticType::Tuple(vec![StaticType::I64, StaticType::Str, StaticType::Bool]);

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TupleFirst,
        args: vec![AotExpr::Var {
            name: "t".to_string(),
            ty: tuple_ty.clone(),
        }],
        return_ty: StaticType::I64,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "t.0");

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TupleLast,
        args: vec![AotExpr::Var {
            name: "t".to_string(),
            ty: tuple_ty,
        }],
        return_ty: StaticType::Bool,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "t.2");

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::TupleFirst,
        args: vec![AotExpr::Var {
            name: "empty".to_string(),
            ty: StaticType::Tuple(vec![]),
        }],
        return_ty: StaticType::Any,
    };
    let err = codegen.emit_expr_to_string(&expr).unwrap_err();
    assert!(err.to_string().contains("empty tuple"));
}

#[test]
fn tuple_dynamic_index_is_gated_issue_6962() {
    let codegen = AotCodeGenerator::default_config();
    let tuple_ty = StaticType::Tuple(vec![StaticType::I64, StaticType::Str]);

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "t".to_string(),
            ty: tuple_ty.clone(),
        }),
        indices: vec![AotExpr::LitI64(2)],
        elem_ty: StaticType::Str,
        is_tuple: true,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "t.1");

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "t".to_string(),
            ty: tuple_ty.clone(),
        }),
        indices: vec![AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        }],
        elem_ty: StaticType::Any,
        is_tuple: true,
    };
    let message = codegen.emit_expr_to_string(&expr).unwrap_err().to_string();
    assert!(message.contains("constant integer index"), "{message}");
    assert!(message.contains("Issue #6962"), "{message}");

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "t".to_string(),
            ty: tuple_ty,
        }),
        indices: vec![AotExpr::LitI64(3)],
        elem_ty: StaticType::Any,
        is_tuple: true,
    };
    let generated = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(generated.contains("aot_throw"), "{generated}");
    assert!(generated.contains("BoundsError"), "{generated}");
}

#[test]
fn dynamic_value_index_handles_tuple_and_array_10464() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "value".to_string(),
            ty: StaticType::Any,
        }),
        indices: vec![AotExpr::LitI64(2)],
        elem_ty: StaticType::Any,
        is_tuple: false,
    };

    let generated = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(generated, "(value).destructure_index(2i64)");
}

#[test]
fn array_index_emits_bounds_guard_issue_7062() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "arr".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            },
        }),
        indices: vec![AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        }],
        elem_ty: StaticType::I64,
        is_tuple: false,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("let _sjulia_arr = &arr"), "{result}");
    assert!(result.contains("_sjulia_idx < 1"), "{result}");
    assert!(result.contains("BoundsError({:?}, ({},))"), "{result}");
    assert!(
        result.contains("_sjulia_arr[(_sjulia_idx - 1) as usize].clone()"),
        "{result}"
    );

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "mat".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(2),
            },
        }),
        indices: vec![
            AotExpr::LitI64(1),
            AotExpr::Var {
                name: "j".to_string(),
                ty: StaticType::I64,
            },
        ],
        elem_ty: StaticType::I64,
        is_tuple: false,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(
        result.contains("let _sjulia_row = &_sjulia_arr"),
        "{result}"
    );
    assert!(result.contains("_sjulia_j < 1"), "{result}");
    assert!(result.contains("BoundsError({:?}, ({}, {}))"), "{result}");
    assert!(
        result.contains("_sjulia_row[(_sjulia_j - 1) as usize].clone()"),
        "{result}"
    );

    let expr = AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "mat".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(2),
            },
        }),
        indices: vec![AotExpr::Var {
            name: "k".to_string(),
            ty: StaticType::I64,
        }],
        elem_ty: StaticType::I64,
        is_tuple: false,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("let _sjulia_linear_idx = k"), "{result}");
    assert!(
        result.contains("let _sjulia_rows = _sjulia_arr.len()"),
        "{result}"
    );
    assert!(
        result.contains("let _sjulia_row = _sjulia_zero_idx % _sjulia_rows"),
        "{result}"
    );
    assert!(
        result.contains("let _sjulia_col = _sjulia_zero_idx / _sjulia_rows"),
        "{result}"
    );
    assert!(
        result.contains("_sjulia_arr[_sjulia_row][_sjulia_col].clone()"),
        "{result}"
    );
}

#[test]
fn range_expr_lowers_to_lazy_iterators_issue_7039() {
    let codegen = AotCodeGenerator::default_config();
    assert_eq!(
        codegen.type_to_rust(&StaticType::Range {
            element: Box::new(StaticType::I64)
        }),
        "SjuliaRange<i64>"
    );
    assert_eq!(
        codegen.type_to_rust(&StaticType::Range {
            element: Box::new(StaticType::Char)
        }),
        "SjuliaCharRange"
    );

    let expr = AotExpr::Range {
        start: Box::new(AotExpr::LitI64(1)),
        stop: Box::new(AotExpr::LitI64(0)),
        step: None,
        elem_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("SjuliaRange::new(1i64, 0i64, _sjulia_range_step)"));
    assert!(result.contains("_sjulia_range_step == 0i64"));
    assert!(!result.contains("_sjulia_range_values"));

    let expr = AotExpr::Range {
        start: Box::new(AotExpr::LitI64(6)),
        stop: Box::new(AotExpr::LitI64(1)),
        step: Some(Box::new(AotExpr::LitI64(-2))),
        elem_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("let _sjulia_range_step = -2i64"));
    assert!(result.contains("SjuliaRange::new(6i64, 1i64, _sjulia_range_step)"));

    let expr = AotExpr::Range {
        start: Box::new(AotExpr::LitF64(0.0)),
        stop: Box::new(AotExpr::LitF64(1.0)),
        step: Some(Box::new(AotExpr::LitF64(0.5))),
        elem_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("_sjulia_range_step == 0.0_f64"));
    assert!(result.contains("SjuliaRange::new(0_f64, 1_f64, _sjulia_range_step)"));

    let expr = AotExpr::Range {
        start: Box::new(AotExpr::LitI64(1)),
        stop: Box::new(AotExpr::LitI64(2)),
        step: Some(Box::new(AotExpr::LitF32(0.5))),
        elem_ty: StaticType::F32,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("(1i64 as f32)"));
    assert!(result.contains("(2i64 as f32)"));
    assert!(result.contains("0.5_f32"));

    let expr = AotExpr::Range {
        start: Box::new(AotExpr::LitChar('a')),
        stop: Box::new(AotExpr::LitChar('z')),
        step: None,
        elem_ty: StaticType::Char,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(
        result.contains("SjuliaCharRange::new('a', 'z')"),
        "{result}"
    );
}

#[test]
fn test_aot_codegen_array_literal() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::ArrayLit {
        elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2), AotExpr::LitI64(3)],
        elem_ty: StaticType::I64,
        shape: vec![3],
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("vec!["));
    assert!(result.contains("1i64"));
    assert!(result.contains("2i64"));
    assert!(result.contains("3i64"));
}

#[test]
fn test_aot_codegen_tuple_literal() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::TupleLit {
        elements: vec![AotExpr::LitI64(1), AotExpr::LitF64(2.0)],
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("("));
    assert!(result.contains(")"));
}

#[test]
fn test_aot_codegen_simple_function() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut func = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    func.body.push(AotStmt::Return(Some(AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "y".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    })));

    let result = codegen.generate_function(&func).unwrap();
    assert!(result.contains("pub fn add(x: i64, y: i64) -> i64"));
    assert!(result.contains("return"));
    assert!(result.contains("wrapping_add"));
}

#[test]
fn test_aot_codegen_if_statement() {
    let mut codegen = AotCodeGenerator::default_config();

    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Lt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(10)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Expr(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Println,
            args: vec![AotExpr::LitStr("less".to_string())],
            return_ty: StaticType::Nothing,
        })],
        else_branch: Some(vec![AotStmt::Expr(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Println,
            args: vec![AotExpr::LitStr("greater".to_string())],
            return_ty: StaticType::Nothing,
        })]),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("if (x < 10i64)"));
    assert!(result.contains("} else {"));
}

#[test]
fn test_aot_codegen_for_range() {
    let mut codegen = AotCodeGenerator::default_config();

    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(1),
        stop: AotExpr::LitI64(10),
        step: None,
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for i in 1i64..=10i64"));
}

#[test]
fn test_aot_codegen_while_loop() {
    let mut codegen = AotCodeGenerator::default_config();

    let stmt = AotStmt::While {
        condition: AotExpr::LitBool(true),
        body: vec![AotStmt::Break],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("loop {"));
    assert!(result.contains("break;"));
}

#[test]
fn test_aot_codegen_struct() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut s = AotStruct::new("Point".to_string(), false);
    s.add_field("x".to_string(), StaticType::F64);
    s.add_field("y".to_string(), StaticType::F64);

    codegen.emit_struct(&s).unwrap();
    let result = &codegen.output;
    assert!(result.contains("pub struct Point"));
    assert!(result.contains("pub x: f64"));
    assert!(result.contains("pub y: f64"));
    assert!(result.contains("impl Point"));
    assert!(result.contains("pub fn new"));
}

#[test]
fn struct_definitions_emit_in_dependency_order_issue_6974() {
    let mut outer = AotStruct::new("Outer".to_string(), false);
    outer.add_field(
        "inner".to_string(),
        StaticType::Struct {
            type_id: 2,
            name: "Inner".to_string(),
        },
    );
    let mut wrapper = AotStruct::new("Wrapper".to_string(), false);
    wrapper.add_field(
        "items".to_string(),
        StaticType::Array {
            element: Box::new(StaticType::Struct {
                type_id: 1,
                name: "Outer".to_string(),
            }),
            ndims: Some(1),
        },
    );
    let mut inner = AotStruct::new("Inner".to_string(), false);
    inner.add_field("x".to_string(), StaticType::I64);

    let mut program = AotProgram::new();
    program.add_struct(wrapper);
    program.add_struct(outer);
    program.add_struct(inner);

    let mut codegen = AotCodeGenerator::default_config();
    let generated = codegen.generate_program(&program).unwrap();
    let inner_pos = generated.find("pub struct Inner").unwrap();
    let outer_pos = generated.find("pub struct Outer").unwrap();
    let wrapper_pos = generated.find("pub struct Wrapper").unwrap();
    assert!(inner_pos < outer_pos, "{generated}");
    assert!(outer_pos < wrapper_pos, "{generated}");
}

#[test]
fn cyclic_struct_dependencies_are_gated_issue_6974() {
    let mut a = AotStruct::new("A".to_string(), false);
    a.add_field(
        "b".to_string(),
        StaticType::Struct {
            type_id: 2,
            name: "B".to_string(),
        },
    );
    let mut b = AotStruct::new("B".to_string(), false);
    b.add_field(
        "a".to_string(),
        StaticType::Struct {
            type_id: 1,
            name: "A".to_string(),
        },
    );

    let mut program = AotProgram::new();
    program.add_struct(a);
    program.add_struct(b);

    let mut codegen = AotCodeGenerator::default_config();
    let message = codegen.generate_program(&program).unwrap_err().to_string();
    assert!(message.contains("cyclic struct dependency"), "{message}");
    assert!(message.contains("Issue #6974"), "{message}");
}

#[test]
fn test_aot_codegen_enum() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut e = AotEnum::new("Color".to_string());
    e.add_member("red".to_string(), 0);
    e.add_member("green".to_string(), 1);
    e.add_member("blue".to_string(), 2);

    codegen.emit_enum(&e).unwrap();
    let result = &codegen.output;
    assert!(result.contains("pub type Color = i32;"));
    // Member constants keep their Julia names (no uppercasing) so references
    // resolve (Issue #7050).
    assert!(result.contains("pub const red: Color = 0;"));
    assert!(result.contains("pub const green: Color = 1;"));
    assert!(result.contains("pub const blue: Color = 2;"));
}

#[test]
fn test_aot_codegen_program_with_enum() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut program = AotProgram::new();

    // Add an enum
    let mut e = AotEnum::new("Direction".to_string());
    e.add_member("north".to_string(), 0);
    e.add_member("south".to_string(), 1);
    program.add_enum(e);

    // Add main statement
    program.main.push(AotStmt::Expr(AotExpr::LitNothing));

    let result = codegen.generate_program(&program).unwrap();
    assert!(result.contains("pub type Direction = i32;"));
    assert!(result.contains("pub const north: Direction = 0;"));
    assert!(result.contains("pub const south: Direction = 1;"));
}

#[test]
fn test_aot_codegen_complete_program() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut program = AotProgram::new();

    // Add a function
    let mut func = AotFunction::new(
        "square".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    func.body.push(AotStmt::Return(Some(AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    })));
    program.add_function(func);

    // Add main statements
    program.main.push(AotStmt::Expr(AotExpr::CallStatic {
        function: "square".to_string(),
        args: vec![AotExpr::LitI64(5)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    }));

    let result = codegen.generate_program(&program).unwrap();
    assert!(result.contains("Auto-generated"));
    assert!(result.contains("#![allow(unused_imports)]"));
    assert!(result.contains("#![allow(unused_must_use)]"));
    assert!(result.contains("#![allow(clippy::needless_range_loop)]"));
    assert!(result.contains("#![allow(clippy::no_effect)]"));
    assert!(result
        .contains("const _: [(); subset_julia_vm_runtime::AOT_RUNTIME_ABI_VERSION] = [(); 2];"));
    assert!(result.contains("fn __sjulia_format_float64(value: f64) -> String"));
    assert!(result.contains("fn __sjulia_format_float32(value: f32) -> String"));
    assert!(result.contains("pub fn square"));
    assert!(result.contains("pub fn main()"));
}

#[test]
fn generated_rust_function_snapshot_issue_7000() {
    let mut codegen = AotCodeGenerator::new(CodegenConfig::release());

    let mut func = AotFunction::new(
        "square".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    func.body.push(AotStmt::Return(Some(AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    })));

    let result = codegen.generate_function(&func).unwrap();
    insta::assert_snapshot!(result.trim_end(), @r###"
pub fn square(x: i64) -> i64 {
    return (x).wrapping_mul(x);
}
"###);
}

#[test]
fn generated_rust_program_snapshot_issue_7000() {
    let mut codegen = AotCodeGenerator::new(CodegenConfig::release());
    let mut program = AotProgram::new();

    let mut add_i64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    add_i64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        })));
    program.add_function(add_i64);

    let mut add_f64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    );
    add_f64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::F64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::F64,
            }),
            result_ty: StaticType::F64,
        })));
    program.add_function(add_f64);

    program.main.push(AotStmt::Expr(AotExpr::CallStatic {
        function: "add".to_string(),
        args: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    }));

    let generated = codegen.generate_program(&program).unwrap();
    let snapshot = generated_pub_fn_sections(
        &generated,
        &[
            "pub fn add_i64_i64",
            "pub fn add_f64_f64",
            "pub fn add(",
            "pub fn main(",
        ],
    );

    insta::assert_snapshot!(snapshot, @r###"
pub fn add_i64_i64(x: i64, y: i64) -> i64 {
    return (x).wrapping_add(y);
}

pub fn add_f64_f64(x: f64, y: f64) -> f64 {
    return (x + y);
}

pub fn add(arg0: Value, arg1: Value) -> RuntimeResult<Value> {
    match (arg0, arg1) {
        (Value::I64(arg0), Value::I64(arg1)) => Ok(Value::from(add_i64_i64(arg0, arg1))),
        (Value::F64(arg0), Value::F64(arg1)) => Ok(Value::from(add_f64_f64(arg0, arg1))),
        (arg0, arg1) => Err(RuntimeError::method_error(format!("add({}, {})", arg0.type_name(), arg1.type_name()))),
    }
}

pub fn main() {
    add_i64_i64(1i64, 2i64);
}
	"###);
}

#[test]
fn c_abi_export_direct_entry_issue_6990() {
    let mut config = CodegenConfig::release();
    config
        .c_abi_exports
        .push(CAbiExport::new("square", "square"));
    let mut codegen = AotCodeGenerator::new(config);
    let mut program = AotProgram::new();

    let mut square = AotFunction::new(
        "square".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    square.body.push(AotStmt::Return(Some(AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    })));
    program.add_function(square);

    let generated = codegen.generate_program(&program).unwrap();
    assert!(generated.contains(
        "#[no_mangle]\npub extern \"C\" fn square(x: i64) -> i64 {\n    return (x).wrapping_mul(x);\n}"
    ));
}

#[test]
fn c_abi_export_alias_wrapper_issue_6990() {
    let mut config = CodegenConfig::release();
    config
        .c_abi_exports
        .push(CAbiExport::new("sjulia_add_i64", "add_i64_i64"));
    let mut codegen = AotCodeGenerator::new(config);
    let mut program = AotProgram::new();

    let mut add_i64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    add_i64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        })));
    program.add_function(add_i64);

    let mut add_f64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    );
    add_f64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::F64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::F64,
            }),
            result_ty: StaticType::F64,
        })));
    program.add_function(add_f64);

    let generated = codegen.generate_program(&program).unwrap();
    assert!(generated.contains("pub fn add_i64_i64(x: i64, y: i64) -> i64"));
    assert!(generated.contains(
        "#[no_mangle]\npub extern \"C\" fn sjulia_add_i64(x: i64, y: i64) -> i64 {\n    add_i64_i64(x, y)\n}"
    ));
}

#[test]
fn c_abi_export_resolves_overload_by_arg_types_issue_7078() {
    let mut config = CodegenConfig::release();
    config.c_abi_exports.push(CAbiExport::with_arg_types(
        "sjulia_add_i64",
        "add",
        vec![StaticType::I64, StaticType::I64],
    ));
    config.c_abi_exports.push(CAbiExport::with_arg_types(
        "sjulia_add_f64",
        "add",
        vec![StaticType::F64, StaticType::F64],
    ));
    let mut codegen = AotCodeGenerator::new(config);
    let mut program = AotProgram::new();

    let mut add_i64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    add_i64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        })));
    program.add_function(add_i64);

    let mut add_f64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    );
    add_f64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::F64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::F64,
            }),
            result_ty: StaticType::F64,
        })));
    program.add_function(add_f64);

    let generated = codegen.generate_program(&program).unwrap();
    assert!(generated.contains(
        "#[no_mangle]\npub extern \"C\" fn sjulia_add_i64(x: i64, y: i64) -> i64 {\n    add_i64_i64(x, y)\n}"
    ));
    assert!(generated.contains(
        "#[no_mangle]\npub extern \"C\" fn sjulia_add_f64(x: f64, y: f64) -> f64 {\n    add_f64_f64(x, y)\n}"
    ));
}

#[test]
fn c_abi_export_rejects_ambiguous_function_issue_6990() {
    let mut config = CodegenConfig::release();
    config.c_abi_exports.push(CAbiExport::new("add", "add"));
    let mut codegen = AotCodeGenerator::new(config);
    let mut program = AotProgram::new();

    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    ));

    let err = codegen.generate_program(&program).unwrap_err();
    assert!(err.to_string().contains("C ABI export `add` is ambiguous"));
}

#[test]
fn c_abi_export_rejects_non_c_stable_type_issue_6990() {
    let mut config = CodegenConfig::release();
    config
        .c_abi_exports
        .push(CAbiExport::new("takes_string", "takes_string"));
    let mut codegen = AotCodeGenerator::new(config);
    let mut program = AotProgram::new();

    program.add_function(AotFunction::new(
        "takes_string".to_string(),
        vec![("s".to_string(), StaticType::Str)],
        StaticType::I64,
    ));

    let err = codegen.generate_program(&program).unwrap_err();
    assert!(err
        .to_string()
        .contains("non-C-stable parameter 1 of type `String`"));
}

#[test]
fn test_aot_codegen_global_name_preserves_original_case() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::with_init(
        "x".to_string(),
        StaticType::I64,
        AotExpr::LitI64(1),
    ));
    program.main.push(AotStmt::Expr(AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::I64,
    }));

    let result = codegen.generate_program(&program).unwrap();
    // The global static now carries the `__sjulia_global_` collision-free prefix
    // (Issue #7242), but the original lowercase name is preserved after it (not
    // uppercased to `X`); the reference is rewritten to match.
    assert!(result.contains("static __sjulia_global_x: i64 = 1i64;"));
    assert!(result.contains("__sjulia_global_x;"));
    assert!(!result.contains("static __sjulia_global_X: i64"));
}

#[test]
fn string_global_static_initializer_is_rejected_issue_7011() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::with_init(
        "x".to_string(),
        StaticType::Str,
        AotExpr::LitStr("a".to_string()),
    ));

    let err = codegen.generate_program(&program).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("global `x`"));
    assert!(message.contains("String"));
    assert!(message.contains("const Rust static initializer"));
    assert!(!message.contains("to_string"));
}

#[test]
fn uninitialized_global_is_rejected_issue_6937() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::new("x".to_string(), StaticType::I64));

    let err = codegen.generate_program(&program).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("uninitialized global `x`"));
    assert!(message.contains("initialize the global before AoT compilation"));
    assert!(!message.contains("TODO: static"));
}

#[test]
fn test_aot_codegen_multidispatch_emits_runtime_dispatcher() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();

    let mut add_i64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    add_i64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        })));
    program.add_function(add_i64);

    let mut add_f64 = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    );
    add_f64
        .body
        .push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::F64,
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: StaticType::F64,
            }),
            result_ty: StaticType::F64,
        })));
    program.add_function(add_f64);

    let result = codegen.generate_program(&program).unwrap();

    assert!(result.contains("pub fn add_i64_i64(x: i64, y: i64) -> i64"));
    assert!(result.contains("pub fn add_f64_f64(x: f64, y: f64) -> f64"));
    assert!(result.contains("pub fn add(arg0: Value, arg1: Value) -> RuntimeResult<Value>"));
    assert!(result.contains(
        "(Value::I64(arg0), Value::I64(arg1)) => Ok(Value::from(add_i64_i64(arg0, arg1)))"
    ));
    assert!(result.contains(
        "(Value::F64(arg0), Value::F64(arg1)) => Ok(Value::from(add_f64_f64(arg0, arg1)))"
    ));
    assert!(result.contains("RuntimeError::method_error"));
}

#[test]
fn test_aot_codegen_static_multidispatch_call_stays_specialized() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();

    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    ));
    codegen.build_method_table(&program);

    let expr = AotExpr::CallStatic {
        function: "add".to_string(),
        args: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "add_i64_i64(1i64, 2i64)");
}

#[test]
fn static_dispatch_picks_most_specific_method_issue_6976() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![("x".to_string(), StaticType::Any)],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    ));
    codegen.build_method_table(&program);

    let expr = AotExpr::CallStatic {
        function: "pick".to_string(),
        args: vec![AotExpr::LitI64(1)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "pick_i64(1i64)");
}

#[test]
fn static_dispatch_rejects_ambiguous_methods_issue_7071() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::Any),
        ],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![
            ("x".to_string(), StaticType::Any),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    ));
    codegen.build_method_table(&program);

    let expr = AotExpr::CallStatic {
        function: "pick".to_string(),
        args: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    };

    let err = codegen.emit_expr_to_string(&expr).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("pick(::Int64, ::Int64) is ambiguous"),
        "unexpected ambiguity diagnostic: {msg}"
    );
}

#[test]
fn static_dispatch_rejects_no_matching_method_issue_7071() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_function(AotFunction::new(
        "only_string".to_string(),
        vec![("x".to_string(), StaticType::Str)],
        StaticType::Str,
    ));
    program.add_function(AotFunction::new(
        "only_string".to_string(),
        vec![("x".to_string(), StaticType::F64)],
        StaticType::F64,
    ));
    codegen.build_method_table(&program);

    let expr = AotExpr::CallStatic {
        function: "only_string".to_string(),
        args: vec![AotExpr::LitI64(1)],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    };

    let err = codegen.emit_expr_to_string(&expr).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("no method matching only_string(::Int64)"),
        "unexpected no-method diagnostic: {msg}"
    );
}

#[test]
fn runtime_dispatcher_checks_ambiguous_overlap_before_fallback_arms_issue_7071() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::Any),
        ],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "pick".to_string(),
        vec![
            ("x".to_string(), StaticType::Any),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    ));

    let generated = codegen.generate_program(&program).unwrap();
    let ambiguous = "(Value::I64(_), Value::I64(_)) => Err(RuntimeError::method_error(\"pick(::Int64, ::Int64) is ambiguous\")),";
    let fallback_left = "(Value::I64(arg0), arg1) => Ok(Value::from(pick_i64_any(arg0, arg1))),";
    let fallback_right = "(arg0, Value::I64(arg1)) => Ok(Value::from(pick_any_i64(arg0, arg1))),";
    let ambiguous_pos = generated
        .find(ambiguous)
        .expect("dispatcher must emit an ambiguity guard");
    assert!(
        ambiguous_pos < generated.find(fallback_left).unwrap()
            && ambiguous_pos < generated.find(fallback_right).unwrap(),
        "ambiguity guard must precede overlapping fallback arms:\n{generated}"
    );
}

#[test]
fn test_aot_codegen_dynamic_call_routes_to_dispatcher_with_value_args() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    ));
    program.add_function(AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::F64,
    ));
    codegen.build_method_table(&program);

    let expr = AotExpr::CallDynamic {
        function: "add".to_string(),
        args: vec![
            AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::Any,
            },
            AotExpr::LitI64(2),
        ],
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(
        result,
        "add(x, Value::from(2i64)).unwrap_or_else(|e| subset_julia_vm_runtime::error::aot_throw(e))"
    );
}

#[test]
fn dynamic_binop_uses_runtime_dispatcher_issue_7074() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::BinOpDynamic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::Any,
        }),
        right: Box::new(AotExpr::LitI64(2)),
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(
        result,
        "subset_julia_vm_runtime::dynamic_binop(subset_julia_vm_runtime::BinOp::Add, &(x), &(Value::from(2i64))).unwrap_or_else(|e| subset_julia_vm_runtime::error::aot_throw(e))"
    );
}

#[test]
fn any_convert_boxes_ternary_branches_issue_7166() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::Convert {
        value: Box::new(AotExpr::Ternary {
            condition: Box::new(AotExpr::Var {
                name: "flag".to_string(),
                ty: StaticType::Bool,
            }),
            then_expr: Box::new(AotExpr::LitI64(1)),
            else_expr: Box::new(AotExpr::LitF64(2.5)),
            result_ty: StaticType::F64,
        }),
        target_ty: StaticType::Any,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(
        result,
        "if flag { Value::from(1i64) } else { Value::from(2.5_f64) }"
    );
    assert!(!result.contains("Value::from(if"));
}

// ========== Arithmetic Operation Tests ==========

#[test]
fn test_aot_codegen_integer_addition() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(20)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(10i64).wrapping_add(20i64)");
}

#[test]
fn test_aot_codegen_float_multiplication() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(AotExpr::LitF64(1.25)),
        right: Box::new(AotExpr::LitF64(2.0)),
        result_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    // Generated format depends on Rust's float formatting
    assert!(result.contains("1.25"));
    assert!(result.contains("*"));
    assert!(result.contains("2"));
}

#[test]
fn complex_arithmetic_supports_parameterized_layouts_issue_7041() {
    let codegen = AotCodeGenerator::default_config();

    for (name, rust_ty, constructor_prefix, op) in [
        (
            "Complex{Float64}",
            "Complex<f64>",
            "Complex::<f64>::new",
            AotBinOp::Add,
        ),
        (
            "Complex{Float32}",
            "Complex<f32>",
            "Complex::<f32>::new",
            AotBinOp::Add,
        ),
        (
            "ComplexF32",
            "Complex<f32>",
            "Complex::<f32>::new",
            AotBinOp::Mul,
        ),
        (
            "Complex{Int64}",
            "Complex<i64>",
            "Complex::<i64>::new",
            AotBinOp::Mul,
        ),
    ] {
        let complex_ty = StaticType::Struct {
            type_id: 0,
            name: name.to_string(),
        };
        assert_eq!(codegen.type_to_rust(&complex_ty), rust_ty);
        let expr = AotExpr::BinOpStatic {
            op,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: complex_ty.clone(),
            }),
            right: Box::new(AotExpr::Var {
                name: "y".to_string(),
                ty: complex_ty.clone(),
            }),
            result_ty: complex_ty.clone(),
        };
        assert_eq!(
            codegen.emit_expr_to_string(&expr).unwrap(),
            format!("(x {} y)", op.to_rust_op())
        );

        let constructor = AotExpr::StructNew {
            name: name.to_string(),
            fields: vec![
                AotExpr::Convert {
                    value: Box::new(AotExpr::LitI64(1)),
                    target_ty: StaticType::complex_param_type_from_name(name).unwrap(),
                },
                AotExpr::Convert {
                    value: Box::new(AotExpr::LitI64(2)),
                    target_ty: StaticType::complex_param_type_from_name(name).unwrap(),
                },
            ],
        };
        assert!(
            codegen
                .emit_expr_to_string(&constructor)
                .unwrap()
                .starts_with(constructor_prefix),
            "{name}"
        );
    }
}

#[test]
fn string_mul_emits_julia_concat_issue_6970() {
    let codegen = AotCodeGenerator::default_config();

    for (left, right) in [
        (
            AotExpr::LitStr("a".to_string()),
            AotExpr::LitStr("b".to_string()),
        ),
        (AotExpr::LitStr("a".to_string()), AotExpr::LitChar('b')),
        (AotExpr::LitChar('a'), AotExpr::LitStr("b".to_string())),
        (AotExpr::LitChar('a'), AotExpr::LitChar('b')),
    ] {
        let expr = AotExpr::BinOpStatic {
            op: AotBinOp::Mul,
            left: Box::new(left),
            right: Box::new(right),
            result_ty: StaticType::Str,
        };
        let result = codegen.emit_expr_to_string(&expr).unwrap();
        assert!(result.starts_with("format!(\"{}{}\", "));
        assert!(!result.contains(" * "));
    }
}

#[test]
fn string_builtin_concat_emits_format_issue_6970() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::StringConcat,
        args: vec![
            AotExpr::LitStr("a".to_string()),
            AotExpr::LitI64(1),
            AotExpr::LitChar('b'),
        ],
        return_ty: StaticType::Str,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "format!(\"{}{}{}\", \"a\".to_string(), 1i64, 'b')");
}

#[test]
fn print_float_uses_julia_display_helper_issue_7013() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Println,
        args: vec![AotExpr::LitF64(3.0), AotExpr::LitF32(3.0)],
        return_ty: StaticType::Nothing,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("__sjulia_format_float64(3_f64)"));
    assert!(result.contains("__sjulia_format_float32(3_f32)"));
    assert!(!result.contains("println!(\"{}{}\", 3_f64, 3_f32)"));
}

#[test]
fn print_array_uses_julia_show_format_issue_7072() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Println,
        args: vec![AotExpr::ArrayLit {
            elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
            elem_ty: StaticType::I64,
            shape: vec![2],
        }],
        return_ty: StaticType::Nothing,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("format!(\"[{}]\""));
    assert!(result.contains(".iter().map"));
    assert!(result.contains(".join(\", \")"));
}

#[test]
fn print_string_array_quotes_elements_issue_7072() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Println,
        args: vec![AotExpr::ArrayLit {
            elements: vec![AotExpr::LitStr("a".to_string())],
            elem_ty: StaticType::Str,
            shape: vec![1],
        }],
        return_ty: StaticType::Nothing,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("format!(\"\\\"{}\\\"\", __sjulia_item)"));
}

#[test]
fn print_matrix_uses_julia_row_separator_issue_7072() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Println,
        args: vec![AotExpr::ArrayLit {
            elements: vec![
                AotExpr::LitI64(1),
                AotExpr::LitI64(2),
                AotExpr::LitI64(3),
                AotExpr::LitI64(4),
            ],
            elem_ty: StaticType::I64,
            shape: vec![2, 2],
        }],
        return_ty: StaticType::Nothing,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("__sjulia_row"));
    assert!(result.contains(".join(\" \")"));
    assert!(result.contains(".join(\"; \")"));
}

#[test]
fn print_tuple_uses_julia_show_format_issue_7072() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Println,
        args: vec![AotExpr::TupleLit {
            elements: vec![AotExpr::LitI64(1), AotExpr::LitStr("x".to_string())],
        }],
        return_ty: StaticType::Nothing,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("let __sjulia_tuple = &"));
    assert!(result.contains("format!(\"({}, {})\""));
    assert!(result.contains("format!(\"\\\"{}\\\"\", &__sjulia_tuple.1)"));
}

#[test]
fn string_concat_float_uses_julia_display_helper_issue_7013() {
    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::StringConcat,
        args: vec![AotExpr::LitStr("x=".to_string()), AotExpr::LitF64(3.0)],
        return_ty: StaticType::Str,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(
        result,
        "format!(\"{}{}\", \"x=\".to_string(), __sjulia_format_float64(3_f64))"
    );
}

#[test]
fn test_aot_codegen_integer_division_to_float() {
    let codegen = AotCodeGenerator::default_config();

    // Julia's / with integers returns Float64
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Div,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(3)),
        result_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    // Should cast both integers to f64
    assert!(result.contains("as f64"));
    assert!(result.contains("/"));
}

#[test]
fn test_aot_codegen_integer_division() {
    let codegen = AotCodeGenerator::default_config();

    // Julia's ÷ (integer division)
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::IntDiv,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(3)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("_sjulia_div_l"));
    assert!(result.contains("_sjulia_div_r"));
    assert!(result.contains("RuntimeError::DivisionByZero"));
    assert!(result.contains("_sjulia_div_l / _sjulia_div_r"));
}

#[test]
fn test_aot_codegen_modulo() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mod,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(3)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("_sjulia_rem_l"));
    assert!(result.contains("_sjulia_rem_r"));
    assert!(result.contains("RuntimeError::DivisionByZero"));
    assert!(result.contains("_sjulia_rem_l % _sjulia_rem_r"));
}

#[test]
fn integer_division_builtins_use_julia_sign_semantics_issue_7067() {
    let codegen = AotCodeGenerator::default_config();

    let div = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Div,
        args: vec![AotExpr::LitI64(-5), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&div).unwrap();
    assert!(result.contains("_sjulia_div_l / _sjulia_div_r"));
    assert!(result.contains("i64::MIN"));
    assert!(result.contains("-1i64"));

    let rem = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Rem,
        args: vec![AotExpr::LitI64(-5), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&rem).unwrap();
    assert!(result.contains("_sjulia_rem_l % _sjulia_rem_r"));
    assert!(!result.contains("_sjulia_rem + _sjulia_rem_r"));

    let modulo = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Mod,
        args: vec![AotExpr::LitI64(-5), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&modulo).unwrap();
    assert!(result.contains("_sjulia_mod_rem + _sjulia_mod_r"));
    assert!(result.contains("(_sjulia_mod_rem > 0i64) != (_sjulia_mod_r > 0i64)"));
}

#[test]
fn fld_cld_builtins_use_floor_and_ceil_integer_division_issue_7067() {
    let codegen = AotCodeGenerator::default_config();

    let fld = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Fld,
        args: vec![AotExpr::LitI64(-5), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&fld).unwrap();
    assert!(result.contains("_sjulia_fld_q - 1"));
    assert!(result.contains("(_sjulia_fld_rem > 0i64) != (_sjulia_fld_r > 0i64)"));

    let cld = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Cld,
        args: vec![AotExpr::LitI64(-5), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&cld).unwrap();
    assert!(result.contains("_sjulia_cld_q + 1"));
    assert!(result.contains("(_sjulia_cld_rem > 0i64) == (_sjulia_cld_r > 0i64)"));
}

#[test]
fn integer_division_builtins_cast_bool_mixed_args_issue_7067() {
    let codegen = AotCodeGenerator::default_config();

    assert_eq!(
        AotBuiltinOp::Mod.return_type(&[StaticType::Bool, StaticType::I64]),
        StaticType::I64
    );
    assert_eq!(
        AotBuiltinOp::Fld.return_type(&[StaticType::Bool, StaticType::I64]),
        StaticType::I64
    );
    assert_eq!(
        AotBuiltinOp::Cld.return_type(&[StaticType::Bool, StaticType::I64]),
        StaticType::I64
    );

    for builtin in [AotBuiltinOp::Mod, AotBuiltinOp::Fld, AotBuiltinOp::Cld] {
        let expr = AotExpr::CallBuiltin {
            builtin,
            args: vec![AotExpr::LitBool(true), AotExpr::LitI64(2)],
            return_ty: StaticType::I64,
        };
        let result = codegen.emit_expr_to_string(&expr).unwrap();
        assert!(result.contains("true as u8 as i64"));
        assert!(!result.contains("true % 2i64"));
        assert!(!result.contains("true / 2i64"));
    }
}

#[test]
fn test_aot_codegen_integer_power() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitI64(2)),
        right: Box::new(AotExpr::LitI64(10)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains(".wrapping_pow("));
    assert!(result.contains("as u32"));
}

#[test]
fn integer_abs_uses_wrapping_parity_issue_7065() {
    let codegen = AotCodeGenerator::default_config();
    assert_eq!(
        AotBuiltinOp::Abs.return_type(&[StaticType::I64]),
        StaticType::I64
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Abs,
        args: vec![AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }],
        return_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "x.wrapping_abs()"
    );

    let expr = AotExpr::CallBuiltin {
        builtin: AotBuiltinOp::Abs,
        args: vec![AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::U64,
        }],
        return_ty: StaticType::U64,
    };
    assert_eq!(codegen.emit_expr_to_string(&expr).unwrap(), "x");
}

#[test]
fn test_aot_codegen_float_power() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitF64(2.0)),
        right: Box::new(AotExpr::LitF64(0.5)),
        result_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains(".powf("));
}

#[test]
fn test_aot_codegen_mixed_type_addition() {
    let codegen = AotCodeGenerator::default_config();

    // i64 + f64 should result in f64
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "y".to_string(),
            ty: StaticType::F64,
        }),
        result_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    // Should cast the integer to float
    assert!(result.contains("as f64"));
}

#[test]
fn float32_codegen_preserves_width_issue_6941() {
    let codegen = AotCodeGenerator::default_config();

    let literal = AotExpr::LitF32(1.25);
    assert_eq!(codegen.emit_expr_to_string(&literal).unwrap(), "1.25_f32");

    let f32_var = || AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::F32,
    };
    let i64_var = || AotExpr::Var {
        name: "i".to_string(),
        ty: StaticType::I64,
    };
    let f64_var = || AotExpr::Var {
        name: "y".to_string(),
        ty: StaticType::F64,
    };
    let bool_var = || AotExpr::Var {
        name: "flag".to_string(),
        ty: StaticType::Bool,
    };

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(f32_var()),
        right: Box::new(i64_var()),
        result_ty: StaticType::F32,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(x + (i as f32))"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Div,
        left: Box::new(i64_var()),
        right: Box::new(f32_var()),
        result_ty: StaticType::F32,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((i as f32) / x)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(f32_var()),
        right: Box::new(f64_var()),
        result_ty: StaticType::F64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((x as f64) * y)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Lt,
        left: Box::new(f32_var()),
        right: Box::new(i64_var()),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(x < (i as f32))"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(bool_var()),
        right: Box::new(f32_var()),
        result_ty: StaticType::F32,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((flag as u8 as f32) + x)"
    );
}

#[test]
fn test_aot_codegen_negation() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::UnaryOp {
        op: AotUnaryOp::Neg,
        operand: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "-5i64");
}

#[test]
fn test_aot_codegen_subtraction() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Sub,
        left: Box::new(AotExpr::LitI64(100)),
        right: Box::new(AotExpr::LitI64(30)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(100i64).wrapping_sub(30i64)");
}

#[test]
fn test_aot_codegen_subtraction_cast_receiver_parenthesized_issue_8146() {
    // Regression (Issue #8146): `@time` lowers `time_ns()` to an expression
    // ending in `... as i64`. Subtracting it via `.wrapping_sub(...)` must
    // parenthesize the receiver, otherwise Rust parses
    // `X as i64.wrapping_sub(t0)` as `X as (i64.wrapping_sub(t0))` and fails
    // with "cast cannot be followed by a method call".
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Sub,
        left: Box::new(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::TimeNs,
            args: vec![],
            return_ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "t0".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(
        !result.contains("as i64.wrapping_sub"),
        "cast receiver must be parenthesized, got: {result}"
    );
    assert!(
        result.contains(").wrapping_sub(t0)"),
        "expected parenthesized wrapping_sub receiver, got: {result}"
    );
}

#[test]
fn test_aot_codegen_wrapping_pow_parenthesizes_cast_receiver_issue_8146() {
    // Same cast-receiver defect class as wrapping_sub, on the integer-power
    // path: `length(s)` emits a trailing `... .len() as i64` cast, so
    // `length(s) ^ 2` must parenthesize the receiver — `(... as i64)
    // .wrapping_pow(2 as u32)` — or rustc rejects it with "cast cannot be
    // followed by a method call" (Issue #8146, general fix over the @time-only
    // wrapping_sub case).
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::StringLength,
            args: vec![AotExpr::Var {
                name: "s".to_string(),
                ty: StaticType::Str,
            }],
            return_ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::LitI64(2)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();

    assert!(
        result.contains("as i64).wrapping_pow("),
        "length(s) ^ 2 must wrap the cast before .wrapping_pow, got: {result}"
    );
    assert!(
        !result.contains("as i64.wrapping_pow"),
        "a cast must not be directly followed by a method call (Issue #8146), got: {result}"
    );
}

#[test]
fn test_aot_codegen_float_modulo() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mod,
        left: Box::new(AotExpr::LitF64(10.5)),
        right: Box::new(AotExpr::LitF64(3.0)),
        result_ty: StaticType::F64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("%"));
}

// ========== Comparison Operation Tests ==========

#[test]
fn test_aot_codegen_less_than() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Lt,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(10)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 < 10i64)");
}

#[test]
fn test_aot_codegen_greater_than() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Gt,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(10i64 > 5i64)");
}

#[test]
fn test_aot_codegen_less_equal() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Le,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 <= 5i64)");
}

#[test]
fn test_aot_codegen_greater_equal() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Ge,
        left: Box::new(AotExpr::LitI64(10)),
        right: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(10i64 >= 5i64)");
}

#[test]
fn test_aot_codegen_equality() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Eq,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 == 5i64)");
}

#[test]
fn test_aot_codegen_inequality() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Ne,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(10)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 != 10i64)");
}

#[test]
fn test_aot_codegen_float_comparison() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Lt,
        left: Box::new(AotExpr::LitF64(1.25)),
        right: Box::new(AotExpr::LitF64(6.78)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("<"));
    assert!(result.contains("1.25"));
}

#[test]
fn test_aot_codegen_mixed_type_comparison() {
    let codegen = AotCodeGenerator::default_config();

    // i64 < f64 should cast i64 to f64
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Lt,
        left: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        right: Box::new(AotExpr::Var {
            name: "y".to_string(),
            ty: StaticType::F64,
        }),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("as f64"));
    assert!(result.contains("<"));
}

#[test]
fn bool_numeric_codegen_preserves_julia_arithmetic_issue_6980() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(true)),
        result_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((true as u8 as i64) + (true as u8 as i64))"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I8,
        }),
        result_ty: StaticType::I8,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((true as u8 as i8) + n)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(false)),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(true && false)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Lt,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I8,
        }),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "((true as u8 as i8) < n)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::IntDiv,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(true)),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ if !true { throw(RuntimeError::DivisionByZero) } else { true } }"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::IntDiv,
        left: Box::new(AotExpr::LitI64(2)),
        right: Box::new(AotExpr::LitBool(false)),
        result_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ if !false { throw(RuntimeError::DivisionByZero) } else { 2i64 / (false as u8 as i64) } }"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mod,
        left: Box::new(AotExpr::LitBool(false)),
        right: Box::new(AotExpr::LitBool(true)),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ if !true { throw(RuntimeError::DivisionByZero) } else { false } }"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Mod,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ if n == 0i64 { throw(RuntimeError::DivisionByZero) } else { (true as u8 as i64) % n } }"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(true)),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(!true || true)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitBool(false)),
        right: Box::new(AotExpr::Var {
            name: "n".to_string(),
            ty: StaticType::I64,
        }),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("DomainError with"));
    assert!(result.contains("else { true }"));
    assert!(result.contains("_sjulia_pow_exp == 0i64 || _sjulia_pow_base"));
    assert!(!result.contains("Value::from"));

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitI64(-1)),
        result_ty: StaticType::Bool,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_pow_base = true; let _sjulia_pow_exp = -1i64; if _sjulia_pow_exp < 0i64 { if !_sjulia_pow_base { throw(RuntimeError::custom(format!(\"DomainError with {}:\\nCannot raise an integer x to a negative power {}.\\nMake x or {} a float by adding a zero decimal (e.g., 2.0^-1 or 2^-1.0 instead of 2^-1) or write 1/x^1, float(x)^-1, x^float(-1) or (x//1)^-1.\", _sjulia_pow_exp, _sjulia_pow_exp, _sjulia_pow_exp))) } else { true } } else { _sjulia_pow_exp == 0i64 || _sjulia_pow_base } }"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitBool(false)),
        right: Box::new(AotExpr::LitF64(-1.0)),
        result_ty: StaticType::F64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "(false as u8 as f64).powf(-1_f64)"
    );

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Pow,
        left: Box::new(AotExpr::LitI64(2)),
        right: Box::new(AotExpr::LitBool(false)),
        result_ty: StaticType::I64,
    };
    assert_eq!(
        codegen.emit_expr_to_string(&expr).unwrap(),
        "{ let _sjulia_pow_base = 2i64; if false { _sjulia_pow_base } else { 1i64 } }"
    );
}

#[test]
fn non_bool_conditions_are_gated_issue_6980() {
    let mut codegen = AotCodeGenerator::default_config();
    let stmt = AotStmt::If {
        condition: AotExpr::LitI64(1),
        then_branch: vec![AotStmt::Expr(AotExpr::LitI64(2))],
        else_branch: None,
    };
    let message = codegen.emit_stmt(&stmt).unwrap_err().to_string();
    assert!(message.contains("condition requires Bool"), "{message}");
    assert!(message.contains("truthy/falsy"), "{message}");
    assert!(message.contains("Issue #6980"), "{message}");

    let codegen = AotCodeGenerator::default_config();
    let expr = AotExpr::Ternary {
        condition: Box::new(AotExpr::LitI64(1)),
        then_expr: Box::new(AotExpr::LitI64(2)),
        else_expr: Box::new(AotExpr::LitI64(3)),
        result_ty: StaticType::I64,
    };
    let message = codegen.emit_expr_to_string(&expr).unwrap_err().to_string();
    assert!(message.contains("condition requires Bool"), "{message}");
    assert!(message.contains("Issue #6980"), "{message}");
}

#[test]
fn type_unstable_bindings_use_value_boundary_issue_6978() {
    let value_ty = StaticType::Union {
        variants: vec![StaticType::I64, StaticType::Str],
    };
    let mut codegen = AotCodeGenerator::default_config();

    codegen
        .emit_stmt(&AotStmt::Let {
            name: "x".to_string(),
            ty: value_ty.clone(),
            value: AotExpr::LitI64(1),
            is_mutable: true,
        })
        .unwrap();
    codegen
        .emit_stmt(&AotStmt::If {
            condition: AotExpr::Var {
                name: "flag".to_string(),
                ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Assign {
                target: AotExpr::Var {
                    name: "x".to_string(),
                    ty: value_ty.clone(),
                },
                value: AotExpr::LitStr("changed".to_string()),
            }],
            else_branch: Some(vec![AotStmt::Assign {
                target: AotExpr::Var {
                    name: "x".to_string(),
                    ty: value_ty,
                },
                value: AotExpr::LitI64(2),
            }]),
        })
        .unwrap();

    let result = &codegen.output;
    assert!(result.contains("let mut x: Value = Value::from(1i64);"));
    assert!(result.contains("x = Value::from(\"changed\".to_string());"));
    assert!(result.contains("x = Value::from(2i64);"));
}

#[test]
fn multi_variant_union_return_uses_runtime_value_enum_issue_6977() {
    let union_ty = StaticType::Union {
        variants: vec![StaticType::I64, StaticType::Str],
    };
    let mut func = AotFunction::new(
        "maybe_union".to_string(),
        vec![("flag".to_string(), StaticType::Bool)],
        union_ty.clone(),
    );
    func.body.push(AotStmt::Expr(AotExpr::Ternary {
        condition: Box::new(AotExpr::Var {
            name: "flag".to_string(),
            ty: StaticType::Bool,
        }),
        then_expr: Box::new(AotExpr::LitI64(1)),
        else_expr: Box::new(AotExpr::LitStr("s".to_string())),
        result_ty: union_ty,
    }));

    let mut codegen = AotCodeGenerator::new(CodegenConfig::release());
    let generated = codegen.generate_function(&func).unwrap();

    assert!(generated.contains("pub fn maybe_union(flag: bool) -> Value"));
    assert!(
        generated
            .contains(r#"if flag { Value::from(1i64) } else { Value::from("s".to_string()) }"#),
        "{generated}"
    );
}

#[test]
fn incompatible_native_assignment_is_gated_issue_6978() {
    let mut codegen = AotCodeGenerator::default_config();
    let err = codegen
        .emit_stmt(&AotStmt::Let {
            name: "x".to_string(),
            ty: StaticType::I64,
            value: AotExpr::LitStr("not an int".to_string()),
            is_mutable: false,
        })
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("let binding cannot store"), "{message}");
    assert!(message.contains("type-unstable variables"), "{message}");
    assert!(message.contains("Issue #6978"), "{message}");
}

#[test]
fn test_aot_codegen_logical_and() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::And,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(false)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(true && false)");
}

#[test]
fn test_aot_codegen_logical_or() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Or,
        left: Box::new(AotExpr::LitBool(true)),
        right: Box::new(AotExpr::LitBool(false)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(true || false)");
}

#[test]
fn test_aot_codegen_logical_not() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::UnaryOp {
        op: AotUnaryOp::Not,
        operand: Box::new(AotExpr::LitBool(true)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "!true");
}

#[test]
fn test_aot_codegen_identity_primitive() {
    let codegen = AotCodeGenerator::default_config();

    // For primitives, === is same as ==
    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Egal,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(5)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 == 5i64)");
}

#[test]
fn test_aot_codegen_not_identity_primitive() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::NotEgal,
        left: Box::new(AotExpr::LitI64(5)),
        right: Box::new(AotExpr::LitI64(10)),
        result_ty: StaticType::Bool,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert_eq!(result, "(5i64 != 10i64)");
}

#[test]
fn test_aot_codegen_bitwise_and() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::BitAnd,
        left: Box::new(AotExpr::LitI64(0b1010)),
        right: Box::new(AotExpr::LitI64(0b1100)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("&"));
}

#[test]
fn test_aot_codegen_bitwise_or() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::BitOr,
        left: Box::new(AotExpr::LitI64(0b1010)),
        right: Box::new(AotExpr::LitI64(0b1100)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("|"));
}

#[test]
fn test_aot_codegen_shift_left() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Shl,
        left: Box::new(AotExpr::LitI64(1)),
        right: Box::new(AotExpr::LitI64(4)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    // `<<` routes through the Julia-faithful shift helper (Issue #7057).
    assert!(result.contains("op_lshift"), "got: {result}");
}

#[test]
fn test_aot_codegen_shift_right() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::BinOpStatic {
        op: AotBinOp::Shr,
        left: Box::new(AotExpr::LitI64(16)),
        right: Box::new(AotExpr::LitI64(2)),
        result_ty: StaticType::I64,
    };
    let result = codegen.emit_expr_to_string(&expr).unwrap();
    // `>>` routes through the Julia-faithful arithmetic shift helper (Issue #7057).
    assert!(result.contains("op_rshift"), "got: {result}");
}

// ========== Control Flow Tests (Issue #1007) ==========

#[test]
fn test_aot_codegen_simple_if() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x > 0 then println("positive") end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Gt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(0)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Expr(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Println,
            args: vec![AotExpr::LitStr("positive".to_string())],
            return_ty: StaticType::Nothing,
        })],
        else_branch: None,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("if (x > 0i64)"));
    assert!(result.contains("positive"));
    assert!(!result.contains("else"));
}

#[test]
fn test_aot_codegen_if_else() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x > 0 then 1 else -1 end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Gt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(0)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Return(Some(AotExpr::LitI64(1)))],
        else_branch: Some(vec![AotStmt::Return(Some(AotExpr::UnaryOp {
            op: AotUnaryOp::Neg,
            operand: Box::new(AotExpr::LitI64(1)),
            result_ty: StaticType::I64,
        }))]),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("if (x > 0i64)"));
    assert!(result.contains("return 1i64"));
    assert!(result.contains("} else {"));
    assert!(result.contains("return -1i64"));
}

#[test]
fn test_aot_codegen_if_elseif_else() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x > 0 then 1 elseif x < 0 then -1 else 0 end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Gt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(0)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Return(Some(AotExpr::LitI64(1)))],
        else_branch: Some(vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Lt,
                left: Box::new(AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(0)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Return(Some(AotExpr::UnaryOp {
                op: AotUnaryOp::Neg,
                operand: Box::new(AotExpr::LitI64(1)),
                result_ty: StaticType::I64,
            }))],
            else_branch: Some(vec![AotStmt::Return(Some(AotExpr::LitI64(0)))]),
        }]),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    // Should generate "} else if" not "} else { if"
    assert!(result.contains("if (x > 0i64)"));
    assert!(result.contains("} else if (x < 0i64)"));
    assert!(result.contains("} else {"));
    assert!(result.contains("return 1i64"));
    assert!(result.contains("return -1i64"));
    assert!(result.contains("return 0i64"));
}

#[test]
fn test_aot_codegen_multiple_elseif() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x == 1 then "one" elseif x == 2 then "two" elseif x == 3 then "three" else "other" end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Eq,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(1)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Return(Some(AotExpr::LitStr("one".to_string())))],
        else_branch: Some(vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Eq,
                left: Box::new(AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(2)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Return(Some(AotExpr::LitStr("two".to_string())))],
            else_branch: Some(vec![AotStmt::If {
                condition: AotExpr::BinOpStatic {
                    op: AotBinOp::Eq,
                    left: Box::new(AotExpr::Var {
                        name: "x".to_string(),
                        ty: StaticType::I64,
                    }),
                    right: Box::new(AotExpr::LitI64(3)),
                    result_ty: StaticType::Bool,
                },
                then_branch: vec![AotStmt::Return(Some(AotExpr::LitStr("three".to_string())))],
                else_branch: Some(vec![AotStmt::Return(Some(AotExpr::LitStr(
                    "other".to_string(),
                )))]),
            }]),
        }]),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    // Verify proper else if chain
    assert!(result.contains("if (x == 1i64)"));
    assert!(result.contains("} else if (x == 2i64)"));
    assert!(result.contains("} else if (x == 3i64)"));
    assert!(result.contains("} else {"));
}

#[test]
fn test_aot_codegen_ternary_operator() {
    let codegen = AotCodeGenerator::default_config();

    // x >= 0 ? x : -x
    let expr = AotExpr::Ternary {
        condition: Box::new(AotExpr::BinOpStatic {
            op: AotBinOp::Ge,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(0)),
            result_ty: StaticType::Bool,
        }),
        then_expr: Box::new(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        }),
        else_expr: Box::new(AotExpr::UnaryOp {
            op: AotUnaryOp::Neg,
            operand: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            result_ty: StaticType::I64,
        }),
        result_ty: StaticType::I64,
    };

    let result = codegen.emit_expr_to_string(&expr).unwrap();
    assert!(result.contains("if (x >= 0i64)"));
    assert!(result.contains("{ x }"));
    assert!(result.contains("else"));
    assert!(result.contains("{ -x }"));
}

#[test]
fn test_aot_codegen_nested_if() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x > 0 then (if y > 0 then 1 else 2 end) else 3 end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Gt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(0)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::If {
            condition: AotExpr::BinOpStatic {
                op: AotBinOp::Gt,
                left: Box::new(AotExpr::Var {
                    name: "y".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(0)),
                result_ty: StaticType::Bool,
            },
            then_branch: vec![AotStmt::Return(Some(AotExpr::LitI64(1)))],
            else_branch: Some(vec![AotStmt::Return(Some(AotExpr::LitI64(2)))]),
        }],
        else_branch: Some(vec![AotStmt::Return(Some(AotExpr::LitI64(3)))]),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    // Nested if should be properly indented
    assert!(result.contains("if (x > 0i64)"));
    assert!(result.contains("if (y > 0i64)"));
    assert!(result.contains("return 1i64"));
    assert!(result.contains("return 2i64"));
    assert!(result.contains("return 3i64"));
}

#[test]
fn test_aot_codegen_if_with_logical_condition() {
    let mut codegen = AotCodeGenerator::default_config();

    // if x > 0 && y > 0 then println("both positive") end
    let stmt = AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::And,
            left: Box::new(AotExpr::BinOpStatic {
                op: AotBinOp::Gt,
                left: Box::new(AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(0)),
                result_ty: StaticType::Bool,
            }),
            right: Box::new(AotExpr::BinOpStatic {
                op: AotBinOp::Gt,
                left: Box::new(AotExpr::Var {
                    name: "y".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(0)),
                result_ty: StaticType::Bool,
            }),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Expr(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Println,
            args: vec![AotExpr::LitStr("both positive".to_string())],
            return_ty: StaticType::Nothing,
        })],
        else_branch: None,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("if ((x > 0i64) && (y > 0i64))"));
}

#[test]
fn test_aot_codegen_if_with_negation() {
    let mut codegen = AotCodeGenerator::default_config();

    // if !done then continue end
    let stmt = AotStmt::If {
        condition: AotExpr::UnaryOp {
            op: AotUnaryOp::Not,
            operand: Box::new(AotExpr::Var {
                name: "done".to_string(),
                ty: StaticType::Bool,
            }),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Continue],
        else_branch: None,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("if !done"));
    assert!(result.contains("continue;"));
}

// ========== Loop Tests (Issue #1008) ==========

#[test]
fn test_aot_codegen_for_range_simple() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 1:10 ... end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(1),
        stop: AotExpr::LitI64(10),
        step: None,
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for i in 1i64..=10i64"));
}

#[test]
fn test_aot_codegen_for_range_with_step() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 1:2:10 ... end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(1),
        stop: AotExpr::LitI64(10),
        step: Some(AotExpr::LitI64(2)),
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("step_by(2 as usize)"));
}

#[test]
fn test_aot_codegen_for_range_reverse() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 10:-1:1 ... end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(10),
        stop: AotExpr::LitI64(1),
        step: Some(AotExpr::LitI64(-1)),
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    // Should generate reverse iteration
    assert!(result.contains(".rev()"));
    assert!(result.contains("1i64..=10i64")); // Swapped range
}

#[test]
fn test_aot_codegen_for_range_reverse_with_step() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 10:-2:1 ... end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(10),
        stop: AotExpr::LitI64(1),
        step: Some(AotExpr::LitI64(-2)),
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "i".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains(".rev()"));
    assert!(result.contains("step_by(2 as usize)"));
}

#[test]
fn test_aot_codegen_for_each_array() {
    let mut codegen = AotCodeGenerator::default_config();

    // for x in arr ... end
    let stmt = AotStmt::ForEach {
        var: "x".to_string(),
        iter: AotExpr::Var {
            name: "arr".to_string(),
            ty: StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            },
        },
        body: vec![AotStmt::Expr(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Println,
            args: vec![AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }],
            return_ty: StaticType::Nothing,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for x in arr.iter().cloned()"));
}

#[test]
fn test_aot_codegen_for_each_array_literal() {
    let mut codegen = AotCodeGenerator::default_config();

    // for x in [1, 2, 3] ... end
    let stmt = AotStmt::ForEach {
        var: "x".to_string(),
        iter: AotExpr::ArrayLit {
            elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2), AotExpr::LitI64(3)],
            elem_ty: StaticType::I64,
            shape: vec![3],
        },
        body: vec![AotStmt::Expr(AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        })],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for x in"));
    assert!(result.contains("vec![1i64, 2i64, 3i64]"));
}

#[test]
fn test_aot_codegen_while_simple() {
    let mut codegen = AotCodeGenerator::default_config();

    // while x < 10 ... end
    let stmt = AotStmt::While {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Lt,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(10)),
            result_ty: StaticType::Bool,
        },
        body: vec![AotStmt::Assign {
            target: AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            },
            value: AotExpr::BinOpStatic {
                op: AotBinOp::Add,
                left: Box::new(AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::LitI64(1)),
                result_ty: StaticType::I64,
            },
        }],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("while (x < 10i64)"));
    assert!(result.contains("x = (x).wrapping_add(1i64)"));
}

#[test]
fn tail_recursion_loop_codegen_uses_mut_params_issue_6987() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut func = AotFunction::new(
        "fact".to_string(),
        vec![
            ("n".to_string(), StaticType::I64),
            ("acc".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    func.body = vec![AotStmt::While {
        condition: AotExpr::LitBool(true),
        body: vec![
            AotStmt::Assign {
                target: AotExpr::Var {
                    name: "n".to_string(),
                    ty: StaticType::I64,
                },
                value: AotExpr::LitI64(1),
            },
            AotStmt::Assign {
                target: AotExpr::Var {
                    name: "acc".to_string(),
                    ty: StaticType::I64,
                },
                value: AotExpr::LitI64(1),
            },
            AotStmt::Continue,
        ],
    }];

    let result = codegen.generate_function(&func).unwrap();

    assert!(result.contains("pub fn fact(mut n: i64, mut acc: i64) -> i64"));
    assert!(result.contains("loop {"));
    assert!(result.contains("continue;"));
    assert!(!result.contains("while true"));
}

#[test]
fn test_aot_codegen_break_statement() {
    let mut codegen = AotCodeGenerator::default_config();

    let stmt = AotStmt::Break;
    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("break;"));
}

#[test]
fn test_aot_codegen_continue_statement() {
    let mut codegen = AotCodeGenerator::default_config();

    let stmt = AotStmt::Continue;
    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("continue;"));
}

#[test]
fn test_aot_codegen_nested_loops() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 1:3
    //     for j in 1:3
    //         sum += i * j
    //     end
    // end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(1),
        stop: AotExpr::LitI64(3),
        step: None,
        body: vec![AotStmt::ForRange {
            var: "j".to_string(),
            start: AotExpr::LitI64(1),
            stop: AotExpr::LitI64(3),
            step: None,
            body: vec![AotStmt::Assign {
                target: AotExpr::Var {
                    name: "sum".to_string(),
                    ty: StaticType::I64,
                },
                value: AotExpr::BinOpStatic {
                    op: AotBinOp::Add,
                    left: Box::new(AotExpr::Var {
                        name: "sum".to_string(),
                        ty: StaticType::I64,
                    }),
                    right: Box::new(AotExpr::BinOpStatic {
                        op: AotBinOp::Mul,
                        left: Box::new(AotExpr::Var {
                            name: "i".to_string(),
                            ty: StaticType::I64,
                        }),
                        right: Box::new(AotExpr::Var {
                            name: "j".to_string(),
                            ty: StaticType::I64,
                        }),
                        result_ty: StaticType::I64,
                    }),
                    result_ty: StaticType::I64,
                },
            }],
        }],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for i in 1i64..=3i64"));
    assert!(result.contains("for j in 1i64..=3i64"));
    assert!(result.contains("sum = (sum).wrapping_add((i).wrapping_mul(j))"));
}

#[test]
fn test_aot_codegen_loop_with_break() {
    let mut codegen = AotCodeGenerator::default_config();

    // while true
    //     if x > 10
    //         break
    //     end
    //     x += 1
    // end
    let stmt = AotStmt::While {
        condition: AotExpr::LitBool(true),
        body: vec![
            AotStmt::If {
                condition: AotExpr::BinOpStatic {
                    op: AotBinOp::Gt,
                    left: Box::new(AotExpr::Var {
                        name: "x".to_string(),
                        ty: StaticType::I64,
                    }),
                    right: Box::new(AotExpr::LitI64(10)),
                    result_ty: StaticType::Bool,
                },
                then_branch: vec![AotStmt::Break],
                else_branch: None,
            },
            AotStmt::Assign {
                target: AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::I64,
                },
                value: AotExpr::BinOpStatic {
                    op: AotBinOp::Add,
                    left: Box::new(AotExpr::Var {
                        name: "x".to_string(),
                        ty: StaticType::I64,
                    }),
                    right: Box::new(AotExpr::LitI64(1)),
                    result_ty: StaticType::I64,
                },
            },
        ],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("loop {"));
    assert!(result.contains("if (x > 10i64)"));
    assert!(result.contains("break;"));
}

#[test]
fn test_aot_codegen_loop_with_continue() {
    let mut codegen = AotCodeGenerator::default_config();

    // for i in 1:10
    //     if i % 2 == 0
    //         continue
    //     end
    //     println(i)
    // end
    let stmt = AotStmt::ForRange {
        var: "i".to_string(),
        start: AotExpr::LitI64(1),
        stop: AotExpr::LitI64(10),
        step: None,
        body: vec![
            AotStmt::If {
                condition: AotExpr::BinOpStatic {
                    op: AotBinOp::Eq,
                    left: Box::new(AotExpr::BinOpStatic {
                        op: AotBinOp::Mod,
                        left: Box::new(AotExpr::Var {
                            name: "i".to_string(),
                            ty: StaticType::I64,
                        }),
                        right: Box::new(AotExpr::LitI64(2)),
                        result_ty: StaticType::I64,
                    }),
                    right: Box::new(AotExpr::LitI64(0)),
                    result_ty: StaticType::Bool,
                },
                then_branch: vec![AotStmt::Continue],
                else_branch: None,
            },
            AotStmt::Expr(AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Println,
                args: vec![AotExpr::Var {
                    name: "i".to_string(),
                    ty: StaticType::I64,
                }],
                return_ty: StaticType::Nothing,
            }),
        ],
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("for i in 1i64..=10i64"));
    assert!(result.contains("continue;"));
}

// ========== Local Variables Tests (Issue #1009) ==========

#[test]
fn test_aot_codegen_let_immutable() {
    let mut codegen = AotCodeGenerator::default_config();

    // let x: i64 = 10
    let stmt = AotStmt::Let {
        name: "x".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(10),
        is_mutable: false,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let x: i64 = 10i64;"));
    assert!(!result.contains("mut"));
}

#[test]
fn test_aot_codegen_let_mutable() {
    let mut codegen = AotCodeGenerator::default_config();

    // let mut x: i64 = 10
    let stmt = AotStmt::Let {
        name: "x".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(10),
        is_mutable: true,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let mut x: i64 = 10i64;"));
}

#[test]
fn test_aot_codegen_let_float() {
    let mut codegen = AotCodeGenerator::default_config();

    // let y: f64 = 1.25
    let stmt = AotStmt::Let {
        name: "y".to_string(),
        ty: StaticType::F64,
        value: AotExpr::LitF64(1.25),
        is_mutable: false,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let y: f64 ="));
    assert!(result.contains("1.25"));
}

#[test]
fn test_aot_codegen_let_bool() {
    let mut codegen = AotCodeGenerator::default_config();

    // let flag: bool = true
    let stmt = AotStmt::Let {
        name: "flag".to_string(),
        ty: StaticType::Bool,
        value: AotExpr::LitBool(true),
        is_mutable: false,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let flag: bool = true;"));
}

#[test]
fn test_aot_codegen_let_string() {
    let mut codegen = AotCodeGenerator::default_config();

    // let s: String = "hello"
    let stmt = AotStmt::Let {
        name: "s".to_string(),
        ty: StaticType::Str,
        value: AotExpr::LitStr("hello".to_string()),
        is_mutable: false,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let s: String ="));
    assert!(result.contains("\"hello\""));
}

#[test]
fn test_aot_codegen_let_with_expression() {
    let mut codegen = AotCodeGenerator::default_config();

    // let y: i64 = x + 5
    let stmt = AotStmt::Let {
        name: "y".to_string(),
        ty: StaticType::I64,
        value: AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::Var {
                name: "x".to_string(),
                ty: StaticType::I64,
            }),
            right: Box::new(AotExpr::LitI64(5)),
            result_ty: StaticType::I64,
        },
        is_mutable: false,
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("let y: i64 = (x).wrapping_add(5i64);"));
}

#[test]
fn test_aot_codegen_simple_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x = 20
    let stmt = AotStmt::Assign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        },
        value: AotExpr::LitI64(20),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("x = 20i64;"));
}

#[test]
fn test_aot_codegen_compound_add_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // sum += 10
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "sum".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::AddAssign,
        value: AotExpr::LitI64(10),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("sum = sum.wrapping_add(10i64);"));
}

#[test]
fn test_aot_codegen_compound_sub_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // count -= 1
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "count".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::SubAssign,
        value: AotExpr::LitI64(1),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("count = count.wrapping_sub(1i64);"));
}

#[test]
fn test_aot_codegen_compound_mul_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // product *= 2
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "product".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::MulAssign,
        value: AotExpr::LitI64(2),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("product = product.wrapping_mul(2i64);"));
}

#[test]
fn test_aot_codegen_compound_div_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // value /= 2
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "value".to_string(),
            ty: StaticType::F64,
        },
        op: CompoundAssignOp::DivAssign,
        value: AotExpr::LitF64(2.0),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("value /="));
}

#[test]
fn test_aot_codegen_compound_mod_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x %= 3
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::ModAssign,
        value: AotExpr::LitI64(3),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("x %= 3i64;"));
}

#[test]
fn test_aot_codegen_compound_pow_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x ^= 2 (power assignment)
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::F64,
        },
        op: CompoundAssignOp::PowAssign,
        value: AotExpr::LitI64(2),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    // Should generate x = x.powi(2) since exponent is integer
    assert!(result.contains("x = x.powi(2i64 as i32);"));
}

#[test]
fn test_aot_codegen_compound_bitand_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x &= 0xFF
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::BitAndAssign,
        value: AotExpr::LitI64(0xFF),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("x &= 255i64;"));
}

#[test]
fn test_aot_codegen_compound_bitor_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // flags |= 0x01
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "flags".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::BitOrAssign,
        value: AotExpr::LitI64(0x01),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("flags |= 1i64;"));
}

#[test]
fn test_aot_codegen_compound_shl_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x <<= 2
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::ShlAssign,
        value: AotExpr::LitI64(2),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("x <<= 2i64;"));
}

#[test]
fn test_aot_codegen_compound_shr_assign() {
    let mut codegen = AotCodeGenerator::default_config();

    // x >>= 1
    let stmt = AotStmt::CompoundAssign {
        target: AotExpr::Var {
            name: "x".to_string(),
            ty: StaticType::I64,
        },
        op: CompoundAssignOp::ShrAssign,
        value: AotExpr::LitI64(1),
    };

    codegen.emit_stmt(&stmt).unwrap();
    let result = &codegen.output;
    assert!(result.contains("x >>= 1i64;"));
}

#[test]
fn test_aot_codegen_variable_in_loop() {
    let mut codegen = AotCodeGenerator::default_config();

    // let mut sum = 0; for i in 1:10 { sum += i }
    codegen
        .emit_stmt(&AotStmt::Let {
            name: "sum".to_string(),
            ty: StaticType::I64,
            value: AotExpr::LitI64(0),
            is_mutable: true,
        })
        .unwrap();

    codegen
        .emit_stmt(&AotStmt::ForRange {
            var: "i".to_string(),
            start: AotExpr::LitI64(1),
            stop: AotExpr::LitI64(10),
            step: None,
            body: vec![AotStmt::CompoundAssign {
                target: AotExpr::Var {
                    name: "sum".to_string(),
                    ty: StaticType::I64,
                },
                op: CompoundAssignOp::AddAssign,
                value: AotExpr::Var {
                    name: "i".to_string(),
                    ty: StaticType::I64,
                },
            }],
        })
        .unwrap();

    let result = &codegen.output;
    assert!(result.contains("let mut sum: i64 = 0i64;"));
    assert!(result.contains("for i in 1i64..=10i64"));
    assert!(result.contains("sum = sum.wrapping_add(i);"));
}

#[test]
fn test_aot_codegen_multiple_variables() {
    let mut codegen = AotCodeGenerator::default_config();

    // Multiple variable declarations
    codegen
        .emit_stmt(&AotStmt::Let {
            name: "a".to_string(),
            ty: StaticType::I64,
            value: AotExpr::LitI64(1),
            is_mutable: false,
        })
        .unwrap();

    codegen
        .emit_stmt(&AotStmt::Let {
            name: "b".to_string(),
            ty: StaticType::I64,
            value: AotExpr::LitI64(2),
            is_mutable: false,
        })
        .unwrap();

    codegen
        .emit_stmt(&AotStmt::Let {
            name: "c".to_string(),
            ty: StaticType::I64,
            value: AotExpr::BinOpStatic {
                op: AotBinOp::Add,
                left: Box::new(AotExpr::Var {
                    name: "a".to_string(),
                    ty: StaticType::I64,
                }),
                right: Box::new(AotExpr::Var {
                    name: "b".to_string(),
                    ty: StaticType::I64,
                }),
                result_ty: StaticType::I64,
            },
            is_mutable: false,
        })
        .unwrap();

    let result = &codegen.output;
    assert!(result.contains("let a: i64 = 1i64;"));
    assert!(result.contains("let b: i64 = 2i64;"));
    assert!(result.contains("let c: i64 = (a).wrapping_add(b);"));
}

// ---------------------------------------------------------------------------
// Issue #7242: top-level scalar globals must not collide with prelude/function
// parameters of the same name (rustc E0530: "function parameters cannot shadow
// statics"). Globals emitted as Rust `static`s are given a `__sjulia_global_`
// prefix, and references to them are rewritten to match.
// ---------------------------------------------------------------------------

#[test]
fn global_static_uses_collision_free_prefix_7242() {
    // A scalar global named `a` would otherwise emit `static a: f64`, which
    // collides with the prelude helper `op_add(a: f64, b: f64)` -> E0530.
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::with_init(
        "a".to_string(),
        StaticType::F64,
        AotExpr::LitF64(3.0),
    ));
    program.add_global(AotGlobal::with_init(
        "b".to_string(),
        StaticType::I64,
        AotExpr::LitI64(2),
    ));

    let result = codegen.generate_program(&program).unwrap();
    // The static carries the prefix.
    assert!(
        result.contains("static __sjulia_global_a: f64 = 3_f64;"),
        "global `a` static must be prefixed, got:\n{}",
        result
    );
    assert!(
        result.contains("static __sjulia_global_b: i64 = 2i64;"),
        "global `b` static must be prefixed, got:\n{}",
        result
    );
    // No bare `static a:`/`static b:` that would shadow `op_add`'s params.
    assert!(
        !result.contains("static a:"),
        "must not emit a bare `static a:` (collides with op_add param), got:\n{}",
        result
    );
    assert!(
        !result.contains("static b:"),
        "must not emit a bare `static b:` (collides with op_add param), got:\n{}",
        result
    );
    // The prelude helper params stay bare (they no longer collide).
    assert!(
        result.contains("fn op_add(a: f64, b: f64) -> f64 { a + b }"),
        "prelude op_add params must stay bare, got:\n{}",
        result
    );
}

#[test]
fn global_reference_in_main_is_prefixed_7242() {
    // `println(a)` at top level references the global static, so the reference
    // must use the prefixed name to match the `static __sjulia_global_a`.
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::with_init(
        "a".to_string(),
        StaticType::I64,
        AotExpr::LitI64(5),
    ));
    program.main.push(AotStmt::Expr(AotExpr::Var {
        name: "a".to_string(),
        ty: StaticType::I64,
    }));

    let result = codegen.generate_program(&program).unwrap();
    assert!(
        result.contains("__sjulia_global_a"),
        "global reference must be rewritten to the prefixed static, got:\n{}",
        result
    );
}

#[test]
fn parameter_shadowing_global_is_not_prefixed_7242() {
    // A function parameter named `a` shadows the global `a`; references to `a`
    // inside the body are the parameter, so they must stay bare (NOT prefixed).
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(AotGlobal::with_init(
        "a".to_string(),
        StaticType::F64,
        AotExpr::LitF64(100.0),
    ));

    let mut func = AotFunction::new(
        "f".to_string(),
        vec![("a".to_string(), StaticType::F64)],
        StaticType::F64,
    );
    func.body.push(AotStmt::Return(Some(AotExpr::Var {
        name: "a".to_string(),
        ty: StaticType::F64,
    })));
    program.add_function(func);

    let result = codegen.generate_program(&program).unwrap();
    // The parameter and its in-body reference stay bare.
    assert!(
        result.contains("pub fn f(a: f64) -> f64"),
        "parameter `a` must stay bare, got:\n{}",
        result
    );
    assert!(
        result.contains("return a;"),
        "in-body reference to the shadowing parameter must stay bare, got:\n{}",
        result
    );
    // The static still carries the prefix.
    assert!(
        result.contains("static __sjulia_global_a: f64"),
        "global static must still be prefixed, got:\n{}",
        result
    );
}

// ---------------------------------------------------------------------------
// Issue #7256: large/small whole-value floats print in Julia scientific
// notation (`1.0e30`), not Rust's decimal expansion. The canonical algorithm
// lives in the runtime crate; the prelude wrappers delegate to it.
// ---------------------------------------------------------------------------

#[test]
fn float_format_prelude_delegates_to_runtime_7256() {
    let mut codegen = AotCodeGenerator::default_config();
    let program = AotProgram::new();
    let result = codegen.generate_program(&program).unwrap();
    // The prelude function still exists with its original signature...
    assert!(
        result.contains("fn __sjulia_format_float64(value: f64) -> String"),
        "float64 formatter must keep its signature, got:\n{}",
        result
    );
    // ...but delegates to the runtime crate's Julia-faithful formatter so AoT
    // output matches the VM and upstream (`1.0e30`, not the decimal expansion).
    assert!(
        result.contains("subset_julia_vm_runtime::intrinsics::format_float64_julia(value)"),
        "float64 formatter must delegate to the runtime helper, got:\n{}",
        result
    );
    assert!(
        result.contains("subset_julia_vm_runtime::intrinsics::format_float32_julia(value)"),
        "float32 formatter must delegate to the runtime helper, got:\n{}",
        result
    );
}
