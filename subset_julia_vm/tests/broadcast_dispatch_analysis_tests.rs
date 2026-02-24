use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};
use subset_julia_vm::vm::{profiler, Vm};

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

fn is_call_like(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Call(_, _)
            | Instr::CallWithKwargs(_, _, _)
            | Instr::CallWithKwargsSplat(_, _, _, _)
            | Instr::CallWithSplat(_, _, _)
            | Instr::CallIntrinsic(_)
            | Instr::CallBuiltin(_, _)
            | Instr::CallDynamic(_, _, _)
            | Instr::CallDynamicBinary(_, _, _)
            | Instr::CallDynamicBinaryBoth(_, _)
            | Instr::CallDynamicBinaryNoFallback(_)
            | Instr::CallDynamicOrBuiltin(_, _)
            | Instr::IterateDynamic(_, _)
            | Instr::CallTypedDispatch(_, _, _, _)
            | Instr::CallTypeConstructor
            | Instr::CallGlobalRef(_)
            | Instr::CallFunctionVariable(_)
            | Instr::CallFunctionVariableWithSplat(_, _)
            | Instr::CallSpecialize(_, _)
    )
}

fn count_call_like(compiled: &CompiledProgram, f: &FunctionInfo) -> usize {
    compiled.code[f.code_start..f.code_end]
        .iter()
        .filter(|instr| is_call_like(instr))
        .count()
}

fn direct_call_target_names(compiled: &CompiledProgram, f: &FunctionInfo) -> Vec<String> {
    compiled.code[f.code_start..f.code_end]
        .iter()
        .filter_map(|instr| match instr {
            Instr::Call(func_index, _) => compiled.functions.get(*func_index).map(|fi| fi.name.clone()),
            _ => None,
        })
        .collect()
}

fn typed_dispatch_target_names(compiled: &CompiledProgram, f: &FunctionInfo) -> Vec<String> {
    compiled.code[f.code_start..f.code_end]
        .iter()
        .filter_map(|instr| match instr {
            Instr::CallTypedDispatch(name, _, _, _) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn single_candidate_typed_dispatches(
    compiled: &CompiledProgram,
    f: &FunctionInfo,
) -> Vec<(String, usize)> {
    compiled.code[f.code_start..f.code_end]
        .iter()
        .filter_map(|instr| match instr {
            Instr::CallTypedDispatch(name, _, fallback, candidates)
                if candidates.len() == 1 && candidates[0].0 == *fallback =>
            {
                Some((name.clone(), *fallback))
            }
            _ => None,
        })
        .collect()
}

fn find_largest_function_by_name<'a>(
    compiled: &'a CompiledProgram,
    name: &str,
) -> Option<&'a FunctionInfo> {
    compiled
        .functions
        .iter()
        .filter(|f| f.name == name)
        .max_by_key(|f| f.code_end - f.code_start)
}

fn run_with_profile(source: &str) -> std::collections::HashMap<String, u64> {
    let compiled = compile_source_with_base(source);
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);

    profiler::clear();
    profiler::enable();
    let _ = vm.run().expect("vm run failed");
    profiler::disable();

    profiler::get_results().into_iter().collect()
}

fn total_call_like_exec_counts(counts: &std::collections::HashMap<String, u64>) -> u64 {
    let call_like_names = [
        "Call",
        "CallWithKwargs",
        "CallWithKwargsSplat",
        "CallWithSplat",
        "CallIntrinsic",
        "CallBuiltin",
        "CallDynamic",
        "CallDynamicBinary",
        "CallDynamicBinaryBoth",
        "IterateDynamic",
        "CallSpecialize",
    ];
    call_like_names
        .iter()
        .map(|name| counts.get(*name).copied().unwrap_or(0))
        .sum()
}

#[test]
fn test_broadcast_call_path_contains_dynamic_call_sites() {
    let src = r#"
function for_add!(out, a, b)
    for i in 1:length(a)
        out[i] = a[i] + b[i]
    end
    out
end

function bcast_add!(out, a, b)
    out .= a .+ b
    out
end

n = 8
a = [i for i in 1:n]
b = [2 * i for i in 1:n]
out = [0 for _ in 1:n]

for_add!(out, a, b)
bcast_add!(out, a, b)
"#;

    let compiled = compile_source_with_base(src);

    let for_add = get_function(&compiled, "for_add!");
    let bcast_add = get_function(&compiled, "bcast_add!");

    let for_calls = count_call_like(&compiled, for_add);
    let bcast_calls = count_call_like(&compiled, bcast_add);
    println!("for_add! call-like count: {}", for_calls);
    println!("bcast_add! call-like count: {}", bcast_calls);
    println!(
        "for_add! bytecode: {:?}",
        &compiled.code[for_add.code_start..for_add.code_end]
    );
    println!(
        "bcast_add! bytecode: {:?}",
        &compiled.code[bcast_add.code_start..bcast_add.code_end]
    );
    assert!(
        compiled.code[for_add.code_start..for_add.code_end]
            .iter()
            .any(|i| matches!(i, Instr::CallDynamicBinaryBoth(_, _))),
        "for_add! should contain dynamic binary operator dispatch in current pipeline"
    );
    assert!(
        compiled.code[bcast_add.code_start..bcast_add.code_end]
            .iter()
            .any(|i| matches!(i, Instr::CallSpecialize(_, _))),
        "bcast_add! should enter specialized broadcast path via CallSpecialize"
    );

    let copyto = find_largest_function_by_name(&compiled, "copyto!")
        .expect("expected at least one copyto! method");
    let copyto_call_count = count_call_like(&compiled, copyto);
    let copyto_targets = direct_call_target_names(&compiled, copyto);
    let copyto_typed_targets = typed_dispatch_target_names(&compiled, copyto);
    println!(
        "copyto! (largest method) call-like count: {}",
        copyto_call_count
    );
    println!("copyto! direct call targets: {:?}", copyto_targets);
    println!("copyto! typed dispatch targets: {:?}", copyto_typed_targets);
    println!(
        "copyto! bytecode head: {:?}",
        &compiled.code[copyto.code_start..std::cmp::min(copyto.code_start + 80, copyto.code_end)]
    );

    assert!(
        copyto_call_count > 0,
        "copyto! still contains direct call sites (not fully inlined/static)"
    );
    assert!(
        copyto_typed_targets
            .iter()
            .any(|name| name == "_broadcast_getindex" || name == "_broadcast_getindex_2d"),
        "expected copyto! to call broadcast helper(s) through typed dispatch, got {:?}",
        copyto_typed_targets
    );
}

#[test]
fn test_runtime_profile_broadcast_executes_more_call_like_instructions() {
    let src_for = r#"
function for_add!(out, a, b, iters)
    for _ in 1:iters
        for i in 1:length(a)
            out[i] = a[i] + b[i]
        end
    end
    out
end

n = 200
iters = 5
a = [Float64(i) for i in 1:n]
b = [Float64(2 * i) for i in 1:n]
out = [0.0 for _ in 1:n]
for_add!(out, a, b, iters)
"#;

    let src_bcast = r#"
function bcast_add!(out, a, b, iters)
    for _ in 1:iters
        out .= a .+ b
    end
    out
end

n = 200
iters = 5
a = [Float64(i) for i in 1:n]
b = [Float64(2 * i) for i in 1:n]
out = [0.0 for _ in 1:n]
bcast_add!(out, a, b, iters)
"#;

    let for_counts = run_with_profile(src_for);
    let bcast_counts = run_with_profile(src_bcast);
    let for_call_like = total_call_like_exec_counts(&for_counts);
    let bcast_call_like = total_call_like_exec_counts(&bcast_counts);
    println!("for call-like exec count: {}", for_call_like);
    println!("bcast call-like exec count: {}", bcast_call_like);
    println!(
        "for profile (subset): Call={}, CallBuiltin={}, CallDynamicBinaryBoth={}, CallSpecialize={}",
        for_counts.get("Call").copied().unwrap_or(0),
        for_counts.get("CallBuiltin").copied().unwrap_or(0),
        for_counts.get("CallDynamicBinaryBoth").copied().unwrap_or(0),
        for_counts.get("CallSpecialize").copied().unwrap_or(0)
    );
    println!(
        "bcast profile (subset): Call={}, CallBuiltin={}, CallDynamicBinaryBoth={}, CallSpecialize={}",
        bcast_counts.get("Call").copied().unwrap_or(0),
        bcast_counts.get("CallBuiltin").copied().unwrap_or(0),
        bcast_counts.get("CallDynamicBinaryBoth").copied().unwrap_or(0),
        bcast_counts.get("CallSpecialize").copied().unwrap_or(0)
    );

    assert!(
        bcast_call_like > for_call_like,
        "expected broadcast to execute more call-like instructions than plain for-loop (for={}, bcast={})",
        for_call_like,
        bcast_call_like
    );
}

#[test]
fn test_copyto_devirtualizes_single_candidate_typed_dispatch() {
    let src = r#"
function bcast_add!(out, a, b)
    out .= a .+ b
    out
end

n = 8
a = [i for i in 1:n]
b = [2 * i for i in 1:n]
out = [0 for _ in 1:n]
bcast_add!(out, a, b)
"#;

    let compiled = compile_source_with_base(src);
    let copyto = find_largest_function_by_name(&compiled, "copyto!")
        .expect("expected at least one copyto! method");

    let single_candidate_dispatches = single_candidate_typed_dispatches(&compiled, copyto);
    println!(
        "single-candidate typed dispatches in copyto!: {:?}",
        single_candidate_dispatches
    );
    assert!(
        single_candidate_dispatches.is_empty(),
        "single-candidate typed dispatch should be devirtualized to direct/specialized call; found {:?}",
        single_candidate_dispatches
    );
}
