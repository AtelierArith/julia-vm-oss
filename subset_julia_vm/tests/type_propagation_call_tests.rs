use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::intrinsics::Intrinsic;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::specialize::specialize_function;
use subset_julia_vm::vm::ArrayElementType;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};
use subset_julia_vm::vm::{Value, ValueType, VarTypeTag, Vm};

fn compile_source_with_base(source: &str) -> CompiledProgram {
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("create parser");
    let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let mut user_program = lowering.lower(parsed).expect("lower source");

    merge_programs(prelude_program, &mut user_program);
    compile_core_program(&user_program).expect("compile failed")
}

fn merge_programs(mut prelude: Program, user: &mut Program) {
    prelude.functions.append(&mut user.functions);
    user.functions = prelude.functions;

    prelude.structs.append(&mut user.structs);
    user.structs = prelude.structs;

    prelude.abstract_types.append(&mut user.abstract_types);
    user.abstract_types = prelude.abstract_types;
}

fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
    compiled
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{}' not found", name))
}

fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
    &compiled.code[f.code_start..f.code_end]
}

fn run_source_with_base(source: &str) -> (Value, Vm<StableRng>) {
    let compiled = compile_source_with_base(source);
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().expect("vm run failed");
    (result, vm)
}

fn has_runtime_dispatch(body: &[Instr]) -> bool {
    body.iter().any(|instr| {
        matches!(
            instr,
            Instr::CallDynamic(_, _, _)
                | Instr::CallDynamicBinary(_, _, _)
                | Instr::CallDynamicBinaryBoth(_, _)
                | Instr::CallDynamicOrBuiltin(_, _)
                | Instr::CallFunctionVariable(_)
                | Instr::CallTypedDispatch(_, _, _, _)
        )
    })
}

fn resolve_value(v: &Value, heap: &[subset_julia_vm::vm::value::StructInstance]) -> Value {
    match v {
        Value::StructRef(idx) => heap
            .get(*idx)
            .map(|s| Value::Struct(s.clone()))
            .unwrap_or_else(|| v.clone()),
        _ => v.clone(),
    }
}

#[test]
fn test_typed_xy_propagate_to_static_call_for_f_xy() {
    let src = r#"
function f(x::Int64, y::Int64)
    x + y
end

function g(x::Int64, y::Int64)
    f(x, y)
end

g(1, 2)
"#;

    let compiled = compile_source_with_base(src);
    let g = get_function(&compiled, "g");
    let body = function_body(&compiled, g);

    println!("g bytecode: {:?}", body);

    // A statically-resolved direct call may be emitted as Call / CallInbounds /
    // CallResolved / CallSpecialize — all carry (func_index, arg_count) and bypass
    // dynamic dispatch. `CallResolved` was introduced in PR #5411 (Issue #5418);
    // accept the whole resolved-direct-call family rather than only `Call`.
    let has_direct_call_to_f = body.iter().any(|instr| match instr {
        Instr::Call(func_idx, 2)
        | Instr::CallInbounds(func_idx, 2)
        | Instr::CallResolved(func_idx, 2)
        | Instr::CallSpecialize(func_idx, 2) => compiled
            .functions
            .get(*func_idx)
            .map(|fi| fi.name == "f")
            .unwrap_or(false),
        Instr::CallSpecializeI64Slots(operands) if operands.slots.len() == 2 => compiled
            .specializable_functions
            .get(operands.spec_func_index)
            .map(|fi| fi.name == "f")
            .unwrap_or(false),
        _ => false,
    });
    let fully_inlined = !body.iter().any(|instr| {
        matches!(
            instr,
            Instr::Call(_, _)
                | Instr::CallInbounds(_, _)
                | Instr::CallResolved(_, _)
                | Instr::CallSpecialize(_, _)
                | Instr::CallSpecializeI64Slots(_)
                | Instr::CallSpecializeInboundsI64Slots(_)
        )
    });

    assert!(
        has_direct_call_to_f || fully_inlined,
        "Expected direct Call to f or fully inlined typed g(x::Int64, y::Int64), got {body:?}"
    );
    assert!(
        !has_runtime_dispatch(body),
        "Typed g(x::Int64, y::Int64) should not require dynamic dispatch"
    );
}

#[test]
fn test_monomorphic_call_emits_callresolved_issue_5078() {
    let src = r#"
function f5078(x::Int64, y::Int64)
    x + y
end

function g5078(x::Int64, y::Int64)
    f5078(x, y)
end

g5078(3, 4)
"#;

    let compiled = compile_source_with_base(src);
    let g = get_function(&compiled, "g5078");
    let body = function_body(&compiled, g);

    let has_callresolved_to_f = body.iter().any(|instr| match instr {
        Instr::CallResolved(func_idx, 2) => compiled
            .functions
            .get(*func_idx)
            .map(|fi| fi.name == "f5078")
            .unwrap_or(false),
        _ => false,
    });
    let fully_inlined = !body.iter().any(|instr| {
        matches!(
            instr,
            Instr::Call(_, _)
                | Instr::CallInbounds(_, _)
                | Instr::CallResolved(_, _)
                | Instr::CallSpecialize(_, _)
                | Instr::CallSpecializeI64Slots(_)
                | Instr::CallSpecializeInboundsI64Slots(_)
        )
    });

    assert!(
        has_callresolved_to_f || fully_inlined,
        "monomorphic g5078 call should emit CallResolved to f5078 or inline it, got {body:?}"
    );
    assert!(
        !has_runtime_dispatch(body),
        "monomorphic path should not emit runtime dispatch instructions: {body:?}"
    );

    let (result, _vm) = run_source_with_base(src);
    match result {
        Value::I64(v) => assert_eq!(v, 7),
        other => panic!("Expected I64(7) for g5078(3, 4), got {:?}", other),
    }
}

#[test]
fn test_untyped_xy_uses_dynamic_dispatch_when_f_is_overloaded() {
    let src = r#"
function f(x::Int64, y::Int64)
    x + y
end

function f(x::Float64, y::Float64)
    x + y
end

function h(x, y)
    f(x, y)
end

h(1, 2)
"#;

    let compiled = compile_source_with_base(src);
    let h = get_function(&compiled, "h");
    let body = function_body(&compiled, h);

    println!("h bytecode: {:?}", body);

    let has_overload_runtime_dispatch = body.iter().any(|instr| match instr {
        Instr::CallDynamic(_, 2, candidates) => candidates.len() == 2,
        Instr::CallTypedDispatch(_, 2, _, candidates) => candidates.len() == 2,
        _ => false,
    });
    assert!(
        has_overload_runtime_dispatch,
        "Expected runtime dispatch over both f methods in untyped h(x, y), got {body:?}"
    );
}

#[test]
fn test_any_arg_single_specific_method_uses_no_match_runtime_dispatch_5984() {
    let src = r#"
function h5984(x::String)
    "got string: " * x
end

function g5984(x::Any)
    h5984(x)
end

g5984("ok")
"#;

    let compiled = compile_source_with_base(src);
    let g = get_function(&compiled, "g5984");
    let body = function_body(&compiled, g);

    let has_no_match_dynamic_string_candidate = body.iter().any(|instr| {
        matches!(
            instr,
            Instr::CallDynamic(fallback, 1, candidates)
                if *fallback == usize::MAX
                    && candidates.iter().any(|c| matches!(c,
                        subset_julia_vm::vm::DynamicCallCandidate::Method(idx)
                            if compiled.functions.get(*idx)
                                .and_then(|f| f.param_julia_types.first())
                                .is_some_and(|ty| ty.to_string() == "String")))
        )
    });
    assert!(
        has_no_match_dynamic_string_candidate,
        "Any-typed forwarder to a lone ::String method should emit no-match runtime dispatch, got {body:?}"
    );
}

#[test]
fn test_direct_static_no_method_emits_runtime_methoderror_6007() {
    let src = r#"
function h6007(x::String)
    "got string: " * x
end

function trigger6007()
    h6007(42)
end
"#;

    let compiled = compile_source_with_base(src);
    let trigger = get_function(&compiled, "trigger6007");
    let body = function_body(&compiled, trigger);

    let has_runtime_methoderror = body.iter().any(|instr| {
        matches!(
            instr,
            Instr::ThrowMethodError(msg)
                if msg.contains("no method matching h6007(::Int64)")
        )
    });
    assert!(
        has_runtime_methoderror,
        "direct static no-method call should emit catchable runtime MethodError, got {body:?}"
    );
}

#[test]
fn test_untyped_f_xy_uses_runtime_specialization_for_int_and_complex_calls() {
    let src_for_bytecode = r#"
function f(x, y)
    x + 2y
end

function g1()
    f(1, 2)
end

function g2()
    f(1, 2im)
end

g1()
g2()
"#;

    let compiled = compile_source_with_base(src_for_bytecode);
    let g1 = get_function(&compiled, "g1");
    let g2 = get_function(&compiled, "g2");
    let g1_body = function_body(&compiled, g1);
    let g2_body = function_body(&compiled, g2);

    println!("g1 bytecode: {:?}", g1_body);
    println!("g2 bytecode: {:?}", g2_body);

    let g1_specialized_or_inlined = g1_body.iter().any(|instr| {
        matches!(instr, Instr::CallSpecialize(_, 2))
            || matches!(instr, Instr::CallSpecializeI64Slots(operands) if operands.slots.len() == 2)
    }) || !has_runtime_dispatch(g1_body);
    let g2_specialized_or_inlined = g2_body.iter().any(|instr| {
        matches!(instr, Instr::CallSpecialize(_, 2))
            || matches!(instr, Instr::CallSpecializeI64Slots(operands) if operands.slots.len() == 2)
    }) || !has_runtime_dispatch(g2_body);
    assert!(
        g1_specialized_or_inlined,
        "g1() should specialize or inline f for untyped parameters, got {g1_body:?}"
    );
    assert!(
        g2_specialized_or_inlined,
        "g2() should specialize or inline f for untyped parameters, got {g2_body:?}"
    );

    let (result_int, _vm1) = run_source_with_base(
        r#"
function f(x, y)
    x + 2y
end
f(1, 2)
"#,
    );
    match result_int {
        Value::I64(v) => assert_eq!(v, 5),
        other => panic!("Expected I64(5) for f(1,2), got {:?}", other),
    }

    let (result_complex, vm2) = run_source_with_base(
        r#"
function f(x, y)
    x + 2y
end
f(1, 2im)
"#,
    );
    let resolved_complex = resolve_value(&result_complex, vm2.get_struct_heap());
    let (re, im) = resolved_complex
        .as_complex_parts()
        .unwrap_or_else(|| panic!("Expected Complex for f(1, 2im), got {:?}", result_complex));
    assert!((re - 1.0).abs() < 1e-10, "real part mismatch: {}", re);
    assert!((im - 4.0).abs() < 1e-10, "imag part mismatch: {}", im);
}

#[test]
fn test_runtime_specialization_keeps_nothing_while_condition_sound_issue_5618() {
    let src = r#"
function f(x)
    while x !== nothing
        return x + 1
    end
    return 0
end

a = f(5)
b = f(0)
c = f(nothing)
a + b + c
"#;

    let (result, _vm) = run_source_with_base(src);
    match result {
        Value::I64(v) => assert_eq!(v, 7),
        other => panic!("Expected I64(7) for mixed f(x) calls, got {:?}", other),
    }
}

#[test]
fn test_specialized_f_xy_instruction_selection_int_vs_complex() {
    let src = r#"
function f(x, y)
    x + 2y
end

f(1, 2)
f(1, 2im)
"#;

    let compiled = compile_source_with_base(src);
    let f = compiled
        .specializable_functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap_or_else(|| panic!("specializable function 'f' not found"));

    let type_object_names = std::collections::HashSet::new();
    let int_spec = specialize_function(
        &f.ir,
        &[ValueType::I64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("int specialize");
    assert!(
        int_spec.code.iter().any(|i| matches!(i, Instr::MulI64)),
        "Int specialization should emit MulI64"
    );
    assert!(
        int_spec.code.iter().any(|i| matches!(i, Instr::AddI64)),
        "Int specialization should emit AddI64"
    );

    let complex_type_id = compiled
        .struct_defs
        .iter()
        .enumerate()
        .find(|(_, d)| d.name == "Complex" || d.name.starts_with("Complex{"))
        .map(|(idx, _)| idx)
        .expect("Complex type not found");
    let complex_spec = specialize_function(
        &f.ir,
        &[ValueType::I64, ValueType::Struct(complex_type_id)],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("complex specialize");

    assert!(
        complex_spec
            .code
            .iter()
            .any(|i| matches!(i, Instr::DynamicMul)),
        "Complex specialization currently uses DynamicMul"
    );
    assert!(
        complex_spec
            .code
            .iter()
            .any(|i| matches!(i, Instr::DynamicAdd)),
        "Complex specialization currently uses DynamicAdd"
    );
}

#[test]
fn test_mixed_narrow_concrete_arithmetic_uses_typed_opcodes_issue_5080() {
    let compiled = compile_source_with_base(
        r#"
function mixed_narrow_add()
    a = Int8(1)
    b = Int16(2)
    a + b
end

mixed_narrow_add()
"#,
    );
    let func = get_function(&compiled, "mixed_narrow_add");
    let body = function_body(&compiled, func);

    assert!(
        body.iter().any(|instr| matches!(instr, Instr::AddI64)),
        "mixed concrete integer arithmetic should emit AddI64"
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::CallIntrinsic(Intrinsic::AddInt))),
        "mixed concrete integer arithmetic should not call AddInt dynamically"
    );
}

#[test]
fn test_call_return_type_stores_concrete_slot_issue_5084() {
    let compiled = compile_source_with_base(
        r#"
function inc(x)
    x + 1
end

function use_inc(x::Int64)
    y = inc(x)
    y + 2
end

use_inc(3)
"#,
    );
    let func = get_function(&compiled, "use_inc");
    let body = function_body(&compiled, func);

    assert!(
        body.iter().any(|instr| {
            matches!(instr, Instr::StoreSlotI64(_))
                || matches!(instr, Instr::StoreI64(name) if name == "y")
        }),
        "call result with inferred Int64 return type should be stored in an Int64 local: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::StoreAny(name) if name == "y")),
        "call result with inferred Int64 return type should not use an Any store: {:?}",
        body
    );
}

#[test]
fn test_nary_float_operator_call_preserves_slot_type() {
    let compiled = compile_source_with_base(
        r#"
function nary_mul_slot(cr::Float64, ci::Float64)
    zr = 0.0
    zi = 0.0
    zi = 2.0 * zr * zi + ci
    zi
end

nary_mul_slot(0.0, 1.0)
"#,
    );
    let func = get_function(&compiled, "nary_mul_slot");
    let body = function_body(&compiled, func);
    let zi_slot = func
        .slot_names
        .iter()
        .position(|name| name == "zi")
        .expect("zi slot");

    assert_eq!(func.slot_types[zi_slot], Some(VarTypeTag::F64));
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::StoreSlotF64(slot) if *slot == zi_slot)),
        "n-ary Float64 operator result should store through F64 slot: {:?}",
        body
    );
    assert!(
        body.iter().all(|instr| {
            !matches!(
                instr,
                Instr::StoreSlot(slot) | Instr::LoadSlot(slot) if *slot == zi_slot
            )
        }),
        "zi should not fall back to generic slot bytecode: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::CallDynamicBinaryBoth(_, _))),
        "n-ary Float64 operator chain should not force dynamic binary dispatch: {:?}",
        body
    );
}

#[test]
fn test_map_inline_lambda_return_type_inference_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function map_inline_lambda_5094()
    ys = map(x -> x * 2.0, [1, 2, 3])
    ys
end

map_inline_lambda_5094()
"#,
    );
    let func = get_function(&compiled, "map_inline_lambda_5094");

    assert_eq!(
        func.return_type,
        ValueType::ArrayOf(ArrayElementType::F64, None),
        "inline lambda map should infer Vector{{Float64}} return type"
    );
    let body = function_body(&compiled, func);
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::StoreAny(name) if name == "ys")),
        "inline lambda map result should not be stored as Any: {:?}",
        body
    );
}

#[test]
fn test_reduce_inline_lambda_return_type_inference_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function reduce_inline_lambda_5094()
    y = reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    y
end

reduce_inline_lambda_5094()
"#,
    );
    let func = get_function(&compiled, "reduce_inline_lambda_5094");

    let body = function_body(&compiled, func);
    assert!(
        body.iter().any(|instr| {
            matches!(instr, Instr::StoreSlotF64(_))
                || matches!(instr, Instr::StoreF64(name) if name == "y")
        }),
        "inline lambda reduce result should be stored as Float64: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::StoreAny(name) if name == "y")),
        "inline lambda reduce result should not be stored as Any: {:?}",
        body
    );
    assert!(
        body.iter().any(|instr| matches!(instr, Instr::ReturnF64)),
        "inline lambda reduce should return through ReturnF64 bytecode: {:?}",
        body
    );
}

#[test]
fn test_qualified_reduction_hof_return_type_inference_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function base_reduce_inline_5094()
    y = Base.reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    y
end

function base_mapreduce_inline_5094()
    y = Base.mapreduce(x -> x * 0.5, +, [1, 2, 3])
    y
end

base_reduce_inline_5094()
base_mapreduce_inline_5094()
"#,
    );

    for function_name in ["base_reduce_inline_5094", "base_mapreduce_inline_5094"] {
        let func = get_function(&compiled, function_name);
        assert_eq!(
            func.return_type,
            ValueType::F64,
            "{function_name} should infer a Float64 return type"
        );

        let body = function_body(&compiled, func);
        assert!(
            body.iter().any(|instr| {
                matches!(instr, Instr::StoreSlotF64(_))
                    || matches!(instr, Instr::StoreF64(name) if name == "y")
            }),
            "{function_name} result should be stored as Float64: {body:?}"
        );
        assert!(
            body.iter()
                .all(|instr| !matches!(instr, Instr::StoreAny(name) if name == "y")),
            "{function_name} result should not be stored as Any: {body:?}"
        );
    }
}

#[test]
fn test_qualified_reduction_init_keyword_rewrite_issue_5541() {
    let compiled = compile_source_with_base(
        r#"
function base_reduce_init_5541()
    y = Base.reduce(min, [1, 2, 3]; init = 10)
    y
end

function base_mapreduce_init_5541()
    y = Base.mapreduce(identity, min, [1, 2, 3]; init = 10)
    y
end

base_reduce_init_5541()
base_mapreduce_init_5541()
"#,
    );

    for function_name in ["base_reduce_init_5541", "base_mapreduce_init_5541"] {
        let func = get_function(&compiled, function_name);
        assert_eq!(
            func.return_type,
            ValueType::I64,
            "{function_name} should infer the Int64 reduction result"
        );

        let body = function_body(&compiled, func);
        assert!(
            body.iter().any(|instr| {
                matches!(instr, Instr::StoreSlotI64(_))
                    || matches!(instr, Instr::StoreI64(name) if name == "y")
            }),
            "{function_name} result should be stored as Int64: {body:?}"
        );
        assert!(
            body.iter()
                .all(|instr| !matches!(instr, Instr::StoreAny(name) if name == "y")),
            "{function_name} result should not be stored as Any: {body:?}"
        );
    }
}
