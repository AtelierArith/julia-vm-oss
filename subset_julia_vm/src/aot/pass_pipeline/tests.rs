use super::*;
use crate::aot::ir::{AotExpr, AotFunction, AotInlinePolicy, AotStmt};
use crate::aot::types::StaticType;

#[test]
fn stage_parser_accepts_canonical_and_relaxed_names() {
    assert_eq!(
        "AfterAotIrConversion".parse::<AotPassStage>().unwrap(),
        AotPassStage::AfterAotIrConversion
    );
    assert_eq!(
        "after-aot-ir-conversion".parse::<AotPassStage>().unwrap(),
        AotPassStage::AfterAotIrConversion
    );
}

#[test]
fn dump_selection_filters_stages() {
    let selection = AotDumpSelection::parse(Some("BeforeBackendCodegen")).unwrap();
    assert!(selection.should_dump(AotPassStage::BeforeBackendCodegen));
    assert!(!selection.should_dump(AotPassStage::AfterOptimization));
    assert!(AotDumpSelection::parse(Some("all"))
        .unwrap()
        .should_dump(AotPassStage::AfterOptimization));
}

#[test]
fn diagnostics_records_stats_and_dump() {
    let mut program = AotProgram::new();
    let mut func = AotFunction::new(
        "id".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    func.body.push(AotStmt::Return(Some(AotExpr::Var {
        name: "x".to_string(),
        ty: StaticType::I64,
    })));
    program.add_function(func);
    let mut diagnostics = AotPassDiagnostics::new(AotDumpSelection::All);
    diagnostics
        .verify_and_record(AotPassStage::AfterAotIrConversion, &program)
        .unwrap();
    assert_eq!(diagnostics.stats()[0].functions, 1);
    assert_eq!(diagnostics.dumps().len(), 1);
    assert!(diagnostics.render_dumps().contains("AfterAotIrConversion"));
}

#[test]
fn verifier_rejects_malformed_array_shape() {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Expr(AotExpr::ArrayLit {
        elements: vec![AotExpr::LitI64(1), AotExpr::LitI64(2)],
        elem_ty: StaticType::I64,
        shape: vec![3],
    }));
    let err = verify_aot_program(AotPassStage::BeforeBackendCodegen, &program).unwrap_err();
    assert!(err
        .to_string()
        .contains("BeforeBackendCodegen verifier failed"));
}

#[test]
fn verifier_accepts_only_rank_zero_empty_index_lists() {
    let scalar = StaticType::Array {
        element: Box::new(StaticType::U8),
        ndims: Some(0),
    };
    let vector = StaticType::Array {
        element: Box::new(StaticType::U8),
        ndims: Some(1),
    };
    let index = |ty| AotExpr::Index {
        array: Box::new(AotExpr::Var {
            name: "value".to_string(),
            ty,
        }),
        indices: Vec::new(),
        elem_ty: StaticType::U8,
        is_tuple: false,
    };
    let mut valid = AotProgram::new();
    valid.main.push(AotStmt::Expr(index(scalar)));
    assert!(verify_aot_program(AotPassStage::AfterAotIrConversion, &valid).is_ok());
    let mut invalid = AotProgram::new();
    invalid.main.push(AotStmt::Expr(index(vector)));
    let error = verify_aot_program(AotPassStage::AfterAotIrConversion, &invalid).unwrap_err();
    assert!(error
        .to_string()
        .contains("index expression has no indices"));
}

#[test]
fn verifier_rejects_empty_call_target() {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Expr(AotExpr::CallStatic {
        function: String::new(),
        args: vec![],
        return_ty: StaticType::I64,
        inline_policy: AotInlinePolicy::Auto,
    }));
    let err = verify_aot_program(AotPassStage::AfterOptimization, &program).unwrap_err();
    assert!(err.to_string().contains("call target name is empty"));
}

#[test]
fn verifier_rejects_native_call_boundary_as_ordinary_call() {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Expr(AotExpr::CallStatic {
        function: "llvmcall".to_string(),
        args: vec![],
        return_ty: StaticType::Any,
        inline_policy: AotInlinePolicy::Auto,
    }));
    let err = verify_aot_program(AotPassStage::BeforeBackendCodegen, &program).unwrap_err();
    assert!(err
        .to_string()
        .contains("native call boundary `llvmcall` reached AoT backend"));
}
