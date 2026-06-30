use std::collections::HashSet;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::specialize::{specialize_function, SpecializationError};
use subset_julia_vm::vm::{ArrayElementType, CompiledProgram, Instr, ValueType};

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

fn specializable<'a>(
    compiled: &'a CompiledProgram,
    name: &str,
) -> &'a subset_julia_vm::vm::SpecializableFunction {
    compiled
        .specializable_functions
        .iter()
        .find(|func| func.name == name)
        .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
}

#[test]
fn index_assign_i64_loop_specializes_to_typed_store_6346() {
    let compiled = compile_source(
        r#"
function fill_index_assign_i64_6346!(a, n)
    for i in 1:n
        a[i] = i * 3
    end
    return a[n]
end
    "#,
    );
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        &specializable(&compiled, "fill_index_assign_i64_6346!").ir,
        &[
            ValueType::ArrayOf(ArrayElementType::I64, None),
            ValueType::I64,
        ],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize Int64 index assignment loop");

    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::IndexStoreTyped(1))),
        "expected IndexStoreTyped in specialized code: {:?}",
        specialized.code
    );
    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::LoadArray(name) if name == "a")),
        "expected typed array load in specialized code: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::I64);
}

#[test]
fn index_assign_f64_loop_specializes_to_typed_store_6346() {
    let compiled = compile_source(
        r#"
function fill_index_assign_f64_6346!(a, n)
    for i in 1:n
        a[i] = Float64(i) * 0.5
    end
    return a[n]
end
    "#,
    );
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        &specializable(&compiled, "fill_index_assign_f64_6346!").ir,
        &[
            ValueType::ArrayOf(ArrayElementType::F64, None),
            ValueType::I64,
        ],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize Float64 index assignment loop");

    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::IndexStoreTyped(1))),
        "expected IndexStoreTyped in specialized code: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::F64);
}

#[test]
fn index_assign_type_mismatch_stays_on_generic_fallback_6346() {
    let compiled = compile_source(
        r#"
function mismatched_index_assign_6346!(a, n)
    for i in 1:n
        a[i] = 1.5
    end
    return n
end
    "#,
    );
    let type_object_names = HashSet::new();
    let result = specialize_function(
        &specializable(&compiled, "mismatched_index_assign_6346!").ir,
        &[
            ValueType::ArrayOf(ArrayElementType::I64, None),
            ValueType::I64,
        ],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    );

    assert!(
        matches!(result, Err(SpecializationError::Unsupported(_))),
        "type-mismatched IndexAssign should fall back to generic bytecode, got {result:?}"
    );
}
