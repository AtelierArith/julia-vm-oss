use super::*;
use crate::aot::ir::{
    AotBinOp, AotBuiltinOp, AotEnum, AotExpr, AotFunction, AotProgram, AotStmt,
    AotStruct, AotUnaryOp, CompoundAssignOp,
};
use crate::aot::types::StaticType;

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
    assert!(result.contains("+"));
    assert!(result.contains("2i64"));
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
fn test_aot_codegen_function_call() {
    let codegen = AotCodeGenerator::default_config();

    let expr = AotExpr::CallStatic {
        function: "add".to_string(),
        args: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        return_ty: StaticType::I64,
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
    assert!(result.contains(".sqrt()"));
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
    assert!(result.contains("+"));
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
    assert!(result.contains("while true"));
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
fn test_aot_codegen_enum() {
    let mut codegen = AotCodeGenerator::default_config();

    let mut e = AotEnum::new("Color".to_string());
    e.add_member("red".to_string(), 0);
    e.add_member("green".to_string(), 1);
    e.add_member("blue".to_string(), 2);

    codegen.emit_enum(&e).unwrap();
    let result = &codegen.output;
    assert!(result.contains("pub type Color = i32;"));
    assert!(result.contains("pub const RED: Color = 0;"));
    assert!(result.contains("pub const GREEN: Color = 1;"));
    assert!(result.contains("pub const BLUE: Color = 2;"));
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
    assert!(result.contains("pub const NORTH: Direction = 0;"));
    assert!(result.contains("pub const SOUTH: Direction = 1;"));
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
    }));

    let result = codegen.generate_program(&program).unwrap();
    assert!(result.contains("Auto-generated"));
    assert!(result.contains("pub fn square"));
    assert!(result.contains("pub fn main()"));
}

#[test]
fn test_aot_codegen_global_name_preserves_original_case() {
    let mut codegen = AotCodeGenerator::default_config();
    let mut program = AotProgram::new();
    program.add_global(crate::aot::ir::AotGlobal::with_init(
        "x".to_string(),
        StaticType::I64,
        AotExpr::LitI64(1),
    ));
    program.main.push(AotStmt::Expr(AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::I64,
    }));

    let result = codegen.generate_program(&program).unwrap();
    assert!(result.contains("static x: i64 = 1i64;"));
    assert!(result.contains("x;"));
    assert!(!result.contains("static X: i64"));
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
    assert_eq!(result, "(10i64 + 20i64)");
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
    // Should be simple integer division
    assert_eq!(result, "(10i64 / 3i64)");
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
    assert_eq!(result, "(10i64 % 3i64)");
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
    assert!(result.contains(".pow("));
    assert!(result.contains("as u32"));
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
    assert_eq!(result, "(100i64 - 30i64)");
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
    assert!(result.contains("<<"));
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
    assert!(result.contains(">>"));
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
    // Should iterate by reference for arrays
    assert!(result.contains("for x in &arr"));
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
    assert!(result.contains("x = (x + 1i64)"));
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
    assert!(result.contains("sum = (sum + (i * j))"));
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
    assert!(result.contains("while true"));
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
    assert!(result.contains("let y: i64 = (x + 5i64);"));
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
    assert!(result.contains("sum += 10i64;"));
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
    assert!(result.contains("count -= 1i64;"));
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
    assert!(result.contains("product *= 2i64;"));
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
    assert!(result.contains("sum += i;"));
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
    assert!(result.contains("let c: i64 = (a + b);"));
}
