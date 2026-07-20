use std::collections::HashMap;

use subset_julia_vm_bytecode::ArrayElementType;

use crate::ir::core::Expr;
use crate::span::Span;
use crate::vm::{Instr, ValueType};

use super::FunctionSpecializer;

pub(super) fn compile_index_with_lossy_struct_index_widens_result_issue_10970() -> Result<(), String>
{
    let span = Span::new(0, 0, 1, 1, 1, 1);
    let mut locals = HashMap::new();
    locals.insert(
        "arr".to_string(),
        ValueType::ArrayOf(ArrayElementType::I64, Some(1)),
    );
    // Runtime specialization may receive a Struct id from a value-local
    // namespace that is not authoritative in struct_defs. Such an index
    // may be scalar (CartesianIndex) or cardinal (AbstractRange).
    locals.insert("k".to_string(), ValueType::Struct(0));
    let mut specializer = FunctionSpecializer::new_for_tests(locals, &[]);

    let result = specializer
        .compile_index(
            &Expr::Var("arr".to_string().into(), span),
            &[Expr::Var("k".to_string().into(), span)],
        )
        .map_err(|err| format!("{err:?}"))?;

    assert_eq!(result, ValueType::Any);
    assert!(specializer
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::IndexLoad(1))));
    Ok(())
}

/// Issues #10746/#10835: the specializer emits the same typed-array-literal build as
/// the main compiler (`NewMemory` / per-element `MemorySet` / `FinalizeArray`)
/// instead of bailing the whole specialization on a `T[a, b]` literal, and
/// every element goes through Julia's `convert(T, x)` semantics.
pub(super) fn compile_typed_array_literal_emits_literal_build_issue_10746() -> Result<(), String> {
    let span = Span::new(0, 0, 1, 1, 1, 1);
    let mut locals = HashMap::new();
    locals.insert("x".to_string(), ValueType::Any);
    locals.insert("target".to_string(), ValueType::DataType);
    let mut specializer = FunctionSpecializer::new_for_tests(locals, &[]);

    let result = specializer
        .compile_typed_array_literal(
            &Expr::Var("target".to_string().into(), span),
            ArrayElementType::Any,
            &[Expr::Var("x".to_string().into(), span)],
        )
        .map_err(|err| format!("{err:?}"))?;

    assert_eq!(result, ValueType::ArrayOf(ArrayElementType::Any, None));
    assert!(specializer
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::NewMemory(ArrayElementType::Any, 1))));
    assert!(specializer.code.iter().any(|instr| matches!(
        instr,
        Instr::CallBuiltin(crate::builtins::BuiltinId::Convert, 2)
    )));
    assert_eq!(
        specializer
            .code
            .iter()
            .filter(|instr| matches!(instr, Instr::LoadAny(name) if name.contains("typed_literal_target")))
            .count(),
        1
    );
    assert_eq!(
        specializer
            .code
            .iter()
            .filter(|instr| matches!(instr, Instr::StoreAny(name) if name.contains("typed_literal_target")))
            .count(),
        1
    );
    assert!(specializer
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::MemorySet)));
    assert!(specializer
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::FinalizeArray(shape) if shape == &vec![1])));
    Ok(())
}
