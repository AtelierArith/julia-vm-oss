//! Bytecode guards for demoting native Dict carriers to boundary/cache roles.

use subset_julia_vm::base;
use subset_julia_vm::builtins::BuiltinId;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};

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

fn is_public_dict_builtin(id: BuiltinId) -> bool {
    // DictNew/DictLen/DictMerge were removed with `Value::Dict` (Issue #6731);
    // the remaining Dict BuiltinIds are pure struct-dispatch trampolines, which
    // a struct-backed Dict program must still not emit.
    matches!(
        id,
        BuiltinId::DictGet
            | BuiltinId::DictGetkey
            | BuiltinId::DictSet
            | BuiltinId::DictDelete
            | BuiltinId::DictHasKey
            | BuiltinId::DictKeys
            | BuiltinId::DictValues
            | BuiltinId::DictPairs
            | BuiltinId::DictGetBang
            | BuiltinId::DictMergeBang
            | BuiltinId::DictEmpty
            | BuiltinId::DictPop
    )
}

fn is_legacy_dict_boundary_instr(instr: &Instr) -> bool {
    // NewDict*/Value::Dict removed (Issue #6731); LoadDict/StoreDict/DictSet/
    // DictLen/ReturnDict remain only as Set-shared instructions and must not
    // appear in struct-backed Dict bytecode.
    match instr {
        Instr::LoadDict(_)
        | Instr::StoreDict(_)
        | Instr::DictSet
        | Instr::DictLen
        | Instr::ReturnDict => true,
        Instr::CallBuiltin(id, _) => is_public_dict_builtin(*id),
        Instr::CallTypedDispatchOrBuiltin(id, _, _, _) => is_public_dict_builtin(*id),
        Instr::CallTypedDispatchOrBuiltinResult(id, _, _, _) => is_public_dict_builtin(*id),
        Instr::CallTypedDispatchOrBuiltinStoreDict(operands)
        | Instr::CallTypedDispatchOrBuiltinStoreDictResult(operands) => {
            is_public_dict_builtin(operands.builtin)
        }
        _ => false,
    }
}

#[test]
fn public_struct_dict_ops_do_not_emit_legacy_dict_boundary_issue_6621() {
    let compiled = compile_source_with_base(
        r#"
function public_struct_dict_ops_6621()
    d = Dict("a" => 1, "b" => 2)
    d["c"] = 3

    x = d["a"]
    y = get(d, "missing", 4)
    k = getkey(d, "a", "fallback")
    ok = haskey(d, "b")

    ks = keys(d)
    vs = values(d)
    ps = pairs(d)
    pair_ok = ("a" => 1) in d

    filtered = filter(p -> p.second > 1, d)
    filter!(p -> p.second > 1, d)
    merge!(d, filtered)
    get!(d, "q", 9)
    pop!(d, "q", 0)
    delete!(d, "c")
    empty!(filtered)

    return ok && pair_ok && x == 1 && y == 4 && k == "a" &&
        length(ks) >= 1 && length(vs) >= 1 && length(ps) >= 1
end
"#,
    );
    let function = get_function(&compiled, "public_struct_dict_ops_6621");
    let body = function_body(&compiled, function);

    let offenders: Vec<_> = body
        .iter()
        .filter(|instr| is_legacy_dict_boundary_instr(instr))
        .collect();

    assert!(
        offenders.is_empty(),
        "public struct-backed Dict bytecode must not use legacy Dict boundary instructions: {offenders:#?}"
    );
}
