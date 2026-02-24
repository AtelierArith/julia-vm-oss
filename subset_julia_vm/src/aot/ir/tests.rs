use super::*;
use crate::aot::types::{JuliaType, StaticType};

#[test]
fn test_basic_block() {
    let mut block = BasicBlock::new("entry".to_string());
    assert_eq!(block.label, "entry");
    assert!(block.instructions.is_empty());
    assert!(block.terminator.is_none());

    block.set_terminator(Terminator::Return(None));
    assert!(block.terminator.is_some());
}

#[test]
fn test_var_ref() {
    let var = VarRef::new("x".to_string(), JuliaType::Int64);
    assert_eq!(var.name, "x");
    assert_eq!(var.version, 0);
    assert_eq!(format!("{}", var), "%x");

    let var2 = var.next_version();
    assert_eq!(var2.version, 1);
    assert_eq!(format!("{}", var2), "%x.1");
}

#[test]
fn test_const_value_type() {
    assert_eq!(ConstValue::Int64(42).get_type(), JuliaType::Int64);
    assert_eq!(ConstValue::Float64(1.25).get_type(), JuliaType::Float64);
    assert_eq!(ConstValue::Bool(true).get_type(), JuliaType::Bool);
}

#[test]
fn test_ir_function() {
    let func = IrFunction::new(
        "test".to_string(),
        vec![("x".to_string(), JuliaType::Int64)],
        JuliaType::Int64,
    );
    assert_eq!(func.name, "test");
    assert_eq!(func.params.len(), 1);
    assert!(func.entry_block().is_some());
}

#[test]
fn test_ir_module() {
    let mut module = IrModule::new("test_module".to_string());
    let func = IrFunction::new("main".to_string(), vec![], JuliaType::Nothing);
    module.add_function(func);
    assert_eq!(module.functions.len(), 1);
}

// ========== AoT IR Tests ==========

#[test]
fn test_aot_program() {
    let mut program = AotProgram::new();
    assert!(program.functions.is_empty());
    assert!(program.globals.is_empty());
    assert!(program.structs.is_empty());
    assert!(program.main.is_empty());

    // Add a function
    let func = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    program.add_function(func);
    assert_eq!(program.functions.len(), 1);

    // Add a global
    let global = AotGlobal::new("PI".to_string(), StaticType::F64);
    program.add_global(global);
    assert_eq!(program.globals.len(), 1);

    // Add a struct
    let mut s = AotStruct::new("Point".to_string(), false);
    s.add_field("x".to_string(), StaticType::F64);
    s.add_field("y".to_string(), StaticType::F64);
    program.add_struct(s);
    assert_eq!(program.structs.len(), 1);
}

#[test]
fn test_aot_function() {
    // Fully static function
    let func = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    assert!(func.is_fully_static());
    assert!(!func.is_generic);

    // Generic function
    let func = AotFunction::new(
        "identity".to_string(),
        vec![("x".to_string(), StaticType::Any)],
        StaticType::Any,
    );
    assert!(!func.is_fully_static());
    assert!(func.is_generic);
}

#[test]
fn test_aot_struct() {
    let mut s = AotStruct::new("Complex".to_string(), false);
    s.add_field("re".to_string(), StaticType::F64);
    s.add_field("im".to_string(), StaticType::F64);

    assert_eq!(s.name, "Complex");
    assert!(!s.is_mutable);
    assert_eq!(s.fields.len(), 2);
}

#[test]
fn test_aot_expr_get_type() {
    // Literals
    assert_eq!(AotExpr::LitI64(42).get_type(), StaticType::I64);
    assert_eq!(AotExpr::LitF64(1.25).get_type(), StaticType::F64);
    assert_eq!(AotExpr::LitBool(true).get_type(), StaticType::Bool);
    assert_eq!(
        AotExpr::LitStr("hello".to_string()).get_type(),
        StaticType::Str
    );
    assert_eq!(AotExpr::LitNothing.get_type(), StaticType::Nothing);

    // Variable
    let var = AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::I64,
    };
    assert_eq!(var.get_type(), StaticType::I64);

    // Binary operation
    let binop = AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(AotExpr::LitI64(1)),
        right: Box::new(AotExpr::LitI64(2)),
        result_ty: StaticType::I64,
    };
    assert_eq!(binop.get_type(), StaticType::I64);
    assert!(binop.is_fully_static());
}

#[test]
fn test_aot_binop() {
    // Arithmetic
    assert_eq!(AotBinOp::Add.to_rust_op(), "+");
    assert_eq!(AotBinOp::Sub.to_rust_op(), "-");
    assert_eq!(AotBinOp::Mul.to_rust_op(), "*");
    assert_eq!(AotBinOp::Div.to_rust_op(), "/");

    // Comparison
    assert!(AotBinOp::Lt.is_comparison());
    assert!(AotBinOp::Eq.is_comparison());
    assert!(!AotBinOp::Add.is_comparison());

    // Logical
    assert!(AotBinOp::And.is_logical());
    assert!(AotBinOp::Or.is_logical());
    assert!(!AotBinOp::Add.is_logical());

    // Special handling
    assert!(AotBinOp::Pow.needs_special_handling());
    assert!(AotBinOp::IntDiv.needs_special_handling());
    assert!(!AotBinOp::Add.needs_special_handling());
}

#[test]
fn test_aot_unaryop() {
    assert_eq!(AotUnaryOp::Neg.to_rust_op(), "-");
    assert_eq!(AotUnaryOp::Not.to_rust_op(), "!");
    assert_eq!(AotUnaryOp::Pos.to_rust_op(), "+");
}

#[test]
fn test_aot_builtinop() {
    // from_name
    assert_eq!(AotBuiltinOp::from_name("sqrt"), Some(AotBuiltinOp::Sqrt));
    assert_eq!(
        AotBuiltinOp::from_name("println"),
        Some(AotBuiltinOp::Println)
    );
    assert_eq!(AotBuiltinOp::from_name("unknown"), None);

    // return_type
    assert_eq!(AotBuiltinOp::Sqrt.return_type(&[]), StaticType::F64);
    assert_eq!(AotBuiltinOp::Length.return_type(&[]), StaticType::I64);
    assert_eq!(AotBuiltinOp::Println.return_type(&[]), StaticType::Nothing);
}

#[test]
fn test_aot_binop_from_core_ir() {
    use crate::ir::core::BinaryOp;

    assert_eq!(AotBinOp::from(&BinaryOp::Add), AotBinOp::Add);
    assert_eq!(AotBinOp::from(&BinaryOp::Sub), AotBinOp::Sub);
    assert_eq!(AotBinOp::from(&BinaryOp::Mul), AotBinOp::Mul);
    assert_eq!(AotBinOp::from(&BinaryOp::Lt), AotBinOp::Lt);
    assert_eq!(AotBinOp::from(&BinaryOp::And), AotBinOp::And);
}

#[test]
fn test_aot_unaryop_from_core_ir() {
    use crate::ir::core::UnaryOp;

    assert_eq!(AotUnaryOp::from(&UnaryOp::Neg), AotUnaryOp::Neg);
    assert_eq!(AotUnaryOp::from(&UnaryOp::Not), AotUnaryOp::Not);
    assert_eq!(AotUnaryOp::from(&UnaryOp::Pos), AotUnaryOp::Pos);
}

// ========== Issue #1191: Mangled Name Sanitization Tests ==========

#[test]
fn test_mangled_name_regular_function() {
    // Regular function name should remain unchanged
    let func = AotFunction::new(
        "add".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    assert_eq!(func.mangled_name(), "add_i64_i64");
}

#[test]
fn test_mangled_name_multiplication_operator() {
    // "*" operator should be sanitized to "op_mul"
    let func = AotFunction::new(
        "*".to_string(),
        vec![
            ("x".to_string(), StaticType::Bool),
            ("y".to_string(), StaticType::Bool),
        ],
        StaticType::Bool,
    );
    // This should NOT be "*_bool_bool" which is invalid Rust
    assert_eq!(func.mangled_name(), "op_mul_bool_bool");
}

#[test]
fn test_mangled_name_addition_operator() {
    // "+" operator should be sanitized to "op_add"
    let func = AotFunction::new(
        "+".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    assert_eq!(func.mangled_name(), "op_add_i64_i64");
}

#[test]
fn test_mangled_name_comparison_operators() {
    // "==" operator should be sanitized to "op_eq"
    let func_eq = AotFunction::new(
        "==".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::Bool,
    );
    assert_eq!(func_eq.mangled_name(), "op_eq_i64_i64");

    // "<" operator should be sanitized to "op_lt"
    let func_lt = AotFunction::new(
        "<".to_string(),
        vec![
            ("x".to_string(), StaticType::F64),
            ("y".to_string(), StaticType::F64),
        ],
        StaticType::Bool,
    );
    assert_eq!(func_lt.mangled_name(), "op_lt_f64_f64");
}

#[test]
fn test_mangled_name_bitwise_operators() {
    // "&" operator should be sanitized to "op_band"
    let func_band = AotFunction::new(
        "&".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    assert_eq!(func_band.mangled_name(), "op_band_i64_i64");

    // "|" operator should be sanitized to "op_bor"
    let func_bor = AotFunction::new(
        "|".to_string(),
        vec![
            ("x".to_string(), StaticType::I64),
            ("y".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    assert_eq!(func_bor.mangled_name(), "op_bor_i64_i64");
}

#[test]
fn test_mangled_name_no_params() {
    // Function with no params should just use sanitized name
    let func = AotFunction::new("+".to_string(), vec![], StaticType::I64);
    assert_eq!(func.mangled_name(), "op_add");
}

#[test]
fn test_sanitize_function_name_all_operators() {
    // Test all supported operator conversions
    assert_eq!(AotFunction::sanitize_function_name("+"), "op_add");
    assert_eq!(AotFunction::sanitize_function_name("-"), "op_sub");
    assert_eq!(AotFunction::sanitize_function_name("*"), "op_mul");
    assert_eq!(AotFunction::sanitize_function_name("/"), "op_div");
    assert_eq!(AotFunction::sanitize_function_name("÷"), "op_intdiv");
    assert_eq!(AotFunction::sanitize_function_name("%"), "op_mod");
    assert_eq!(AotFunction::sanitize_function_name("^"), "op_pow");
    assert_eq!(AotFunction::sanitize_function_name("=="), "op_eq");
    assert_eq!(AotFunction::sanitize_function_name("!="), "op_ne");
    assert_eq!(AotFunction::sanitize_function_name("<"), "op_lt");
    assert_eq!(AotFunction::sanitize_function_name("<="), "op_le");
    assert_eq!(AotFunction::sanitize_function_name(">"), "op_gt");
    assert_eq!(AotFunction::sanitize_function_name(">="), "op_ge");
    assert_eq!(AotFunction::sanitize_function_name("==="), "op_egal");
    assert_eq!(AotFunction::sanitize_function_name("!=="), "op_notegal");
    assert_eq!(AotFunction::sanitize_function_name("!"), "op_not");
    assert_eq!(AotFunction::sanitize_function_name("&"), "op_band");
    assert_eq!(AotFunction::sanitize_function_name("|"), "op_bor");
    assert_eq!(AotFunction::sanitize_function_name("⊻"), "op_xor");
    assert_eq!(AotFunction::sanitize_function_name("xor"), "op_xor");
    assert_eq!(AotFunction::sanitize_function_name("<<"), "op_lshift");
    assert_eq!(AotFunction::sanitize_function_name(">>"), "op_rshift");
    assert_eq!(AotFunction::sanitize_function_name(">>>"), "op_urshift");
    assert_eq!(AotFunction::sanitize_function_name("~"), "op_bnot");
    assert_eq!(AotFunction::sanitize_function_name("&&"), "op_and");
    assert_eq!(AotFunction::sanitize_function_name("||"), "op_or");
}

#[test]
fn test_sanitize_function_name_regular_names() {
    // Regular function names should remain unchanged
    assert_eq!(AotFunction::sanitize_function_name("add"), "add");
    assert_eq!(
        AotFunction::sanitize_function_name("my_function"),
        "my_function"
    );
    assert_eq!(AotFunction::sanitize_function_name("convert"), "convert");
}

#[test]
fn test_sanitize_function_name_invalid_chars() {
    // Invalid characters should be replaced with underscores
    assert_eq!(AotFunction::sanitize_function_name("my-func"), "my_func");
    assert_eq!(AotFunction::sanitize_function_name("foo.bar"), "foo_bar");
}
