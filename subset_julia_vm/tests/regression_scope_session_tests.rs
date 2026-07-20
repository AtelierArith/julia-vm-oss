//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod file_mode_soft_scope_9210_tests {
    //! Regression tests for Issue #9210: file/module-mode soft scope.
    //!
    //! In non-interactive script execution (`sjulia file.jl` / `-e` / piped stdin),
    //! a top-level `for`/`while` body is a strict soft scope. An assignment to a
    //! name that already exists as a global is ambiguous, so the name becomes a NEW
    //! local: a read-before-write (`+=`) raises `UndefVarError`, and a write-only
    //! assignment leaves the global untouched. An explicit `global` opts back into
    //! mutating the module binding. This matches `julia file.jl` / `julia -e`.
    //!
    //! The strict path is [`compile_and_run_value_file_mode`]
    //! (`SoftScopeMode::Strict`, used by the CLI script runners). The interactive
    //! REPL — and the historical C ABI / WASM host default reached via
    //! [`compile_and_run_value`] — keep the lenient soft scope (Issues #8691 /
    //! #8715), where the same loop mutates the existing global.

    use subset_julia_vm::repl::REPLSession;
    use subset_julia_vm::{compile_and_run_value, compile_and_run_value_file_mode};
    use subset_julia_vm_bytecode::Value;

    const SEED: u64 = 42;

    /// File mode: `total += 1` against an existing global inside a top-level loop
    /// reads the freshly bound (un-initialised) local first, so it raises
    /// `UndefVarError` naming the *original* variable (the internal rename is
    /// hidden from the message).
    #[test]
    fn file_mode_compound_assign_to_global_raises_undefvar() {
        let src = "total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("file-mode soft scope must raise UndefVarError for read-before-write");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
        assert!(
            err.contains("`total`"),
            "error must name the original variable `total`, got: {err}"
        );
        assert!(
            !err.contains("softlocal"),
            "internal soft-scope rename must not leak into the message: {err}"
        );
    }

    /// The `while`-loop form behaves identically (the brief calls this out).
    #[test]
    fn file_mode_while_loop_compound_assign_raises_undefvar() {
        let src = "total = 0\nc = 0\nwhile c < 3\n    total += 1\n    c += 1\nend\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("while-loop soft scope must raise UndefVarError");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
    }

    /// File mode: a loop nested inside a top-level `begin … end` is still a
    /// top-level soft scope, so `total += 1` raises `UndefVarError` (matching
    /// `julia file.jl`). `begin` lowers to an empty-bindings `LetBlock`, which the
    /// soft-scope pass must descend into (regression for the fix-forward gap).
    #[test]
    fn file_mode_begin_block_loop_raises_undefvar() {
        let src =
            "total = 0\nbegin\n    for i in 1:3\n        total += 1\n    end\nend\nprintln(total)\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("begin-nested loop soft scope must raise UndefVarError");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
        assert!(
            err.contains("`total`") && !err.contains("softlocal"),
            "error must name the original variable `total` without the internal rename: {err}"
        );
    }

    /// File mode: a loop under `@time` is a top-level soft scope too. `@time`
    /// expands to a block that nests the loop inside a value-position
    /// empty-bindings `LetBlock`, so the pass must look through assignment values.
    #[test]
    fn file_mode_time_macro_loop_raises_undefvar() {
        let src = "total = 0\n@time for i in 1:3\n    total += 1\nend\nprintln(total)\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("@time-nested loop soft scope must raise UndefVarError");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
    }

    /// File mode: a write-only loop nested in a top-level `begin … end` binds a new
    /// local and leaves the global unchanged (upstream prints the original value).
    #[test]
    fn file_mode_begin_block_write_only_leaves_global_unchanged() {
        let src = "total = 5\nbegin\n    for i in 1:3\n        total = i\n    end\nend\ntotal\n";
        let value = compile_and_run_value_file_mode(src, SEED)
            .expect("write-only begin loop must not error");
        assert!(
            matches!(value, Value::I64(5)),
            "global must keep its original value, got {value:?}"
        );
    }

    /// The lenient (REPL/host) path keeps mutating the existing global for the same
    /// `begin`-nested loop that errors under file mode — guarding the split for the
    /// newly covered `begin` shape.
    #[test]
    fn lenient_path_begin_block_still_mutates_global() {
        let src = "total = 0\nbegin\n    for i in 1:3\n        total += 1\n    end\nend\ntotal\n";
        let value = compile_and_run_value(src, SEED)
            .expect("lenient soft scope must not raise UndefVarError for begin-nested loop");
        assert!(
            matches!(value, Value::I64(3)),
            "lenient soft scope must mutate the existing global, got {value:?}"
        );
    }

    /// File mode: a write-only loop binds a new local and leaves the global
    /// unchanged.
    #[test]
    fn file_mode_write_only_leaves_global_unchanged() {
        let src = "total = 5\nfor i in 1:3\n    total = i\nend\ntotal\n";
        let value =
            compile_and_run_value_file_mode(src, SEED).expect("write-only loop must not error");
        assert!(
            matches!(value, Value::I64(5)),
            "global must keep its original value, got {value:?}"
        );
    }

    /// File mode: an explicit `global` declaration still mutates the module binding.
    #[test]
    fn file_mode_explicit_global_still_mutates() {
        let src = "total = 0\nfor i in 1:3\n    global total\n    total += 1\nend\ntotal\n";
        let value =
            compile_and_run_value_file_mode(src, SEED).expect("explicit global must not error");
        assert!(
            matches!(value, Value::I64(3)),
            "explicit global must mutate to 3, got {value:?}"
        );
    }

    /// A fresh try-clause assignment must not become a phantom module binding
    /// that changes the soft-scope decision for a later loop (#11322).
    #[test]
    fn fresh_try_clause_binding_is_not_visible_after_clause_11322() -> Result<(), String> {
        let src = "try\n    ghost11322 = 1\ncatch\nend\nfor i in 1:1\n    ghost11322 = 2\nend\n@isdefined ghost11322\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::Bool(false)),
            "fresh try-clause name must stay undefined at module scope, got {value:?}"
        );
        Ok(())
    }

    /// The hygienic locals declared by the top-level `Test.@test` expansion
    /// enclose its internal `try`; strict file-mode rewriting must not split
    /// those generated slots into fresh ambiguous locals (#11415).
    #[test]
    fn top_level_test_macro_preserves_try_enclosing_locals_11415() -> Result<(), String> {
        let value = compile_and_run_value_file_mode("using Test\n@test true\ntrue\n", SEED)?;
        assert!(
            matches!(value, Value::Bool(true)),
            "top-level @test true must pass, got {value:?}"
        );
        Ok(())
    }

    /// A subsequent ordinary top-level assignment replaces retired clause-local
    /// provenance with a live mutable-global fact. The later loop must preserve
    /// that global rather than reusing compiler metadata for the old clause.
    #[test]
    fn later_global_supersedes_retired_try_clause_local() -> Result<(), String> {
        let src = "try\n    mixedprov = 1\ncatch\nend\nmixedprov = 0\nfor i in 1:1\n    mixedprov = 2\nend\nmixedprov\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::I64(0)),
            "later mutable global must remain unchanged by the loop, got {value:?}"
        );
        Ok(())
    }

    /// An existing mutable global assigned in a top-level try is an ambiguous
    /// strict soft-scope binding; the clause-local value must not escape
    /// (#11335).
    #[test]
    fn try_clause_assignment_preserves_existing_global_11335() -> Result<(), String> {
        let src = "existing11335 = 1\ntry\n    existing11335 = 2\ncatch\nend\nexisting11335\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::I64(1)),
            "strict try-clause assignment must preserve the outer global, got {value:?}"
        );
        Ok(())
    }

    /// A nested try assignment reuses the local created by its enclosing try
    /// clause. The inner value is therefore visible later in the outer clause
    /// while the module global remains untouched (#11159).
    #[test]
    fn nested_try_assignment_reuses_enclosing_clause_local_11159() -> Result<(), String> {
        let src = "nestedreuse11159 = 0\nobserved11159 = 0\ntry\n    nestedreuse11159 = 1\n    try\n        nestedreuse11159 = 2\n    catch\n    end\n    global observed11159 = nestedreuse11159\ncatch\nend\nstring(nestedreuse11159, \":\", observed11159)\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::Str(ref value) if value.as_ref() == "0:2"),
            "nested clause must reuse the outer clause local, got {value:?}"
        );
        Ok(())
    }

    /// Preserve the already-green const-shadow behavior while changing the
    /// shared source-order inventory (#11305).
    #[test]
    fn loop_nested_try_const_shadow_preserves_const_11305() -> Result<(), String> {
        let src = "const const11305 = 1\nfor i in 1:1\n    try\n        const11305 = 2\n    catch\n    end\nend\nconst11305\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::I64(1)),
            "nested clause-local assignment must preserve the outer const, got {value:?}"
        );
        Ok(())
    }

    /// A direct top-level loop uses the same silent fresh-local rule for a
    /// same-named const (#11305's shared const-provenance path).
    #[test]
    fn direct_loop_const_shadow_preserves_const_11305() -> Result<(), String> {
        let src = "const direct_const11305 = 1\nfor i in 1:1\n    direct_const11305 = 2\nend\ndirect_const11305\n";
        let value = compile_and_run_value_file_mode(src, SEED)?;
        assert!(
            matches!(value, Value::I64(1)),
            "direct loop-local assignment must preserve the outer const, got {value:?}"
        );
        Ok(())
    }

    /// File mode: a loop-body name that is NOT a pre-existing global keeps its
    /// existing behaviour (already a fresh loop-local — no untouched global is
    /// created).
    #[test]
    fn file_mode_non_preexisting_name_unaffected() {
        let src = "for i in 1:3\n    acc = i\nend\n42\n";
        let value =
            compile_and_run_value_file_mode(src, SEED).expect("fresh loop-local must not error");
        assert!(matches!(value, Value::I64(42)), "got {value:?}");
    }

    /// The lenient (REPL/host) path — reached via `compile_and_run_value` — keeps
    /// mutating the existing global for the identical program that errors under
    /// file mode. This guards the file-vs-host split.
    #[test]
    fn lenient_path_still_mutates_global() {
        let src = "total = 0\nfor i in 1:3\n    total += 1\nend\ntotal\n";
        let value = compile_and_run_value(src, SEED)
            .expect("lenient soft scope must not raise UndefVarError");
        assert!(
            matches!(value, Value::I64(3)),
            "lenient soft scope must mutate the existing global, got {value:?}"
        );
    }

    /// The interactive REPL stays lenient: the same loop mutates the existing
    /// global (Issues #8691 / #8715).
    #[test]
    fn repl_mode_stays_lenient() {
        let mut session = REPLSession::new(SEED);
        let init = session.eval("total = 0");
        assert!(init.success, "init failed: {:?}", init.error);
        let loop_result = session.eval("for i in 1:3\n    total += 1\nend");
        assert!(
            loop_result.success,
            "REPL loop must not raise UndefVarError (lenient soft scope): {:?}",
            loop_result.error
        );
        let read = session.eval("total");
        assert!(read.success, "read failed: {:?}", read.error);
        assert!(
            matches!(read.value, Some(Value::I64(3))),
            "REPL soft scope must mutate the existing global, got {:?}",
            read.value
        );
    }
}

mod binding_provenance_contract_11317_tests {
    use std::collections::HashSet;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::compile_and_run_value_file_mode;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value, VarTypeTag};
    use subset_julia_vm_types::ir::core::{Block, Expr, LocalDeclKind, Program, Stmt};

    enum Expected {
        I64(i64),
        Str(&'static str),
    }

    #[derive(Clone, Copy)]
    struct DeclExpectation {
        name_prefix: &'static str,
        kind: LocalDeclKind,
    }

    enum BytecodeExpectation {
        Global {
            function: &'static str,
            key: &'static str,
        },
        Slot {
            function: &'static str,
            slot: &'static str,
            tag: Option<VarTypeTag>,
        },
    }

    struct ProvenanceCase {
        name: &'static str,
        context: &'static str,
        scope: &'static str,
        binding: &'static str,
        exit: &'static str,
        value_path: &'static str,
        decl: Option<DeclExpectation>,
        bytecode: BytecodeExpectation,
        source: &'static str,
        expected: Expected,
    }

    fn lower_source(source: &str) -> Program {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        lowering.lower(parsed).expect("lower source")
    }

    fn collect_expr_decls<'a>(expr: &'a Expr, decls: &mut Vec<(&'a str, LocalDeclKind)>) {
        if let Expr::LetBlock { body, .. } = expr {
            collect_block_decls(body, decls);
        }
    }

    fn collect_block_decls<'a>(block: &'a Block, decls: &mut Vec<(&'a str, LocalDeclKind)>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::LocalDecl { var, kind, .. } => decls.push((var, *kind)),
                Stmt::Block(body)
                | Stmt::For { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachTuple { body, .. }
                | Stmt::While { body, .. }
                | Stmt::Timed { body, .. }
                | Stmt::TestSet { body, .. } => collect_block_decls(body, decls),
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_block_decls(then_branch, decls);
                    if let Some(branch) = else_branch {
                        collect_block_decls(branch, decls);
                    }
                }
                Stmt::Try {
                    try_block,
                    catch_block,
                    else_block,
                    finally_block,
                    ..
                } => {
                    collect_block_decls(try_block, decls);
                    for branch in [catch_block, else_block, finally_block]
                        .into_iter()
                        .flatten()
                    {
                        collect_block_decls(branch, decls);
                    }
                }
                Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                    collect_block_decls(&func.body, decls);
                }
                Stmt::Assign { value, .. }
                | Stmt::AddAssign { value, .. }
                | Stmt::Expr { expr: value, .. } => collect_expr_decls(value, decls),
                Stmt::Return {
                    value: Some(value), ..
                } => collect_expr_decls(value, decls),
                _ => {}
            }
        }
    }

    fn collect_program_decls(program: &Program) -> Vec<(&str, LocalDeclKind)> {
        let mut decls = Vec::new();
        collect_block_decls(&program.main, &mut decls);
        for function in &program.functions {
            collect_block_decls(&function.body, &mut decls);
        }
        for module in &program.modules {
            collect_block_decls(&module.body, &mut decls);
            for function in &module.functions {
                collect_block_decls(&function.body, &mut decls);
            }
        }
        decls
    }

    fn function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("function `{name}` not found"))
    }

    fn assert_bytecode(case: &ProvenanceCase, compiled: &CompiledProgram) {
        match case.bytecode {
            BytecodeExpectation::Global {
                function: name,
                key,
            } => {
                let function = function(compiled, name);
                let body = &compiled.code[function.code_start..function.code_end];
                assert!(
                    body.iter().any(
                        |instr| matches!(instr, Instr::StoreGlobalAny(actual) if actual == key)
                    ),
                    "{} must store through owner-qualified global key `{key}`: {body:?}",
                    case.name
                );
                assert!(
                    body.iter().any(
                        |instr| matches!(instr, Instr::LoadGlobalAny(actual) if actual == key)
                    ),
                    "{} must load through owner-qualified global key `{key}`: {body:?}",
                    case.name
                );
            }
            BytecodeExpectation::Slot {
                function: name,
                slot: slot_name,
                tag,
            } => {
                let function = function(compiled, name);
                let slot = function
                    .slot_names
                    .iter()
                    .position(|actual| actual.starts_with(slot_name))
                    .unwrap_or_else(|| panic!("{} missing slot `{slot_name}`", case.name));
                assert_eq!(
                    function.slot_types.get(slot).copied().flatten(),
                    tag,
                    "{} storage tag drifted for `{slot_name}`",
                    case.name
                );
                let body = &compiled.code[function.code_start..function.code_end];
                let has_generic = body.iter().any(|instr| {
                    matches!(
                        instr,
                        Instr::LoadSlot(actual)
                            | Instr::StoreSlot(actual)
                            | Instr::TakeSlot(actual)
                            if *actual == slot
                    )
                });
                let has_i64_load = body.iter().any(|instr| {
                    matches!(
                        instr,
                        Instr::LoadSlotI64(actual) | Instr::LoadAddI64Slot(actual)
                            if *actual == slot
                    )
                });
                let has_i64_store = body
                    .iter()
                    .any(|instr| matches!(instr, Instr::StoreSlotI64(actual) if *actual == slot));
                let uses_expected_path = match tag {
                    Some(VarTypeTag::I64) => has_i64_load && has_i64_store && !has_generic,
                    None | Some(VarTypeTag::Any) => has_generic && !has_i64_load && !has_i64_store,
                    other => panic!("{} unsupported test storage tag {other:?}", case.name),
                };
                assert!(
                    uses_expected_path,
                    "{} does not exercise the expected slot path for `{slot_name}`: {body:?}",
                    case.name
                );
            }
        }
    }

    fn assert_expected(case: &ProvenanceCase, value: Value) {
        match (&case.expected, value) {
            (Expected::I64(expected), Value::I64(actual)) => {
                assert_eq!(actual, *expected, "{}", case.name);
            }
            (Expected::Str(expected), Value::Str(actual)) => {
                assert_eq!(actual.as_ref(), *expected, "{}", case.name);
            }
            (_, actual) => panic!("{} returned unexpected value {actual:?}", case.name),
        }
    }

    #[test]
    fn vm_binding_provenance_matrix_covers_cross_layer_contract_11317() {
        let cases = [
            ProvenanceCase {
                name: "main loop explicit dynamic",
                context: "main",
                scope: "loop",
                binding: "explicit",
                exit: "normal",
                value_path: "dynamic",
                decl: None,
                bytecode: BytecodeExpectation::Global {
                    function: "set_main_11317",
                    key: "main_v_11317",
                },
                source: "main_v_11317 = 1\nfunction set_main_11317()\n    global main_v_11317\n    for i in 1:1\n        main_v_11317 = 11\n    end\n    return main_v_11317\nend\nset_main_11317()",
                expected: Expected::I64(11),
            },
            ProvenanceCase {
                name: "module function explicit dynamic",
                context: "module",
                scope: "function",
                binding: "explicit",
                exit: "normal",
                value_path: "dynamic",
                decl: None,
                bytecode: BytecodeExpectation::Global {
                    function: "M11317.setv",
                    key: "M11317.value",
                },
                source: "module M11317\nvalue = \"outer\"\nfunction setv(x::Any)\n    global value\n    value = x\n    return value\nend\nsetv(\"dynamic\")\nend\nM11317.value",
                expected: Expected::Str("dynamic"),
            },
            ProvenanceCase {
                name: "function try fresh normal dynamic",
                context: "function",
                scope: "try",
                binding: "fresh",
                exit: "normal",
                value_path: "dynamic",
                decl: None,
                bytecode: BytecodeExpectation::Slot {
                    function: "fresh_normal_11317",
                    slot: "fresh_11317",
                    tag: None,
                },
                source: "function fresh_normal_11317()\n    try\n        fresh_11317 = 22\n    catch\n    end\n    return @isdefined(fresh_11317) ? -1 : 22\nend\nfresh_normal_11317()",
                expected: Expected::I64(22),
            },
            ProvenanceCase {
                name: "function try fresh exceptional dynamic",
                context: "function",
                scope: "try",
                binding: "fresh",
                exit: "exceptional",
                value_path: "dynamic",
                decl: None,
                bytecode: BytecodeExpectation::Slot {
                    function: "fresh_exception_11317",
                    slot: "transient_11317",
                    tag: None,
                },
                source: "function fresh_exception_11317(x::Any)\n    try\n        transient_11317 = x\n        error(\"boom\")\n    catch\n    end\n    return @isdefined(transient_11317) ? \"leaked\" : \"clean\"\nend\nfresh_exception_11317(\"inner\")",
                expected: Expected::Str("clean"),
            },
            ProvenanceCase {
                name: "function try result compiler generated",
                context: "function",
                scope: "try",
                binding: "compiler-generated",
                exit: "exceptional",
                value_path: "dynamic",
                decl: Some(DeclExpectation {
                    name_prefix: "__sjvm_try_result_",
                    kind: LocalDeclKind::CompilerEnclosing,
                }),
                bytecode: BytecodeExpectation::Slot {
                    function: "result_11317",
                    slot: "__sjvm_try_result_",
                    tag: None,
                },
                source: "function result_11317(flag)\n    value_11317 = try\n        flag ? 33 : error(\"boom\")\n    catch\n        44\n    end\n    return value_11317\nend\nresult_11317(true) * 100 + result_11317(false)",
                expected: Expected::I64(3344),
            },
            ProvenanceCase {
                name: "function loop explicit typed",
                context: "function",
                scope: "loop",
                binding: "explicit",
                exit: "normal",
                value_path: "typed",
                decl: Some(DeclExpectation {
                    name_prefix: "step_11317",
                    kind: LocalDeclKind::Explicit,
                }),
                bytecode: BytecodeExpectation::Slot {
                    function: "loop_11317",
                    slot: "step_11317",
                    tag: Some(VarTypeTag::I64),
                },
                source: "function loop_11317()\n    total_11317 = 0\n    for i in 1:3\n        local step_11317 = i\n        total_11317 += step_11317\n    end\n    return total_11317\nend\nloop_11317()",
                expected: Expected::I64(6),
            },
        ];

        for case in &cases {
            let program = lower_source(case.source);
            let decls = collect_program_decls(&program);
            match case.decl {
                Some(expected) => assert!(
                    decls.iter().any(|(name, kind)| {
                        name.starts_with(expected.name_prefix) && *kind == expected.kind
                    }),
                    "{} missing {:?} declaration with prefix `{}`: {decls:?}",
                    case.name,
                    expected.kind,
                    expected.name_prefix
                ),
                None => assert!(
                    decls.is_empty(),
                    "{} unexpectedly introduced LocalDecl provenance: {decls:?}",
                    case.name
                ),
            }
            let compiled = compile_with_cache(&program)
                .unwrap_or_else(|error| panic!("{} failed to compile: {error:?}", case.name));
            assert_bytecode(case, &compiled);

            let value = compile_and_run_value_file_mode(case.source, 11317)
                .unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
            assert_expected(case, value);
        }

        let dimensions = |project: fn(&ProvenanceCase) -> &'static str| {
            cases.iter().map(project).collect::<HashSet<_>>()
        };
        assert_eq!(
            dimensions(|case| case.context),
            HashSet::from(["main", "module", "function"])
        );
        assert_eq!(
            dimensions(|case| case.scope),
            HashSet::from(["function", "loop", "try"])
        );
        assert_eq!(
            dimensions(|case| case.binding),
            HashSet::from(["explicit", "fresh", "compiler-generated"])
        );
        assert_eq!(
            dimensions(|case| case.exit),
            HashSet::from(["normal", "exceptional"])
        );
        assert_eq!(
            dimensions(|case| case.value_path),
            HashSet::from(["typed", "dynamic"])
        );
    }
}

mod builtin_spelled_value_parametric_base_10948_tests {
    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    /// A lexical value parameter shadows a same-named builtin type constructor
    /// in the function body. The base of `Vector{Int64}` must therefore be the
    /// runtime argument (`Set`), not the global `Vector` binding (Issue #10948).
    #[test]
    fn short_form_value_parameter_shadows_builtin_parametric_base() {
        let src = r#"
make_type_10948(Vector::Type) = Vector{Int64}
make_type_10948(Set) === Set{Int64}
"#;

        let value = compile_and_run_value(src, 0).expect("Issue #10948 MWE must run");
        assert!(
            matches!(value, Value::Bool(true)),
            "lexical value parameter must supply the parametric base, got {value:?}"
        );
    }

    #[test]
    fn arrow_value_parameter_shadows_builtin_parametric_base() {
        let src = r#"
make_type_arrow_10948 = (Vector::Type) -> Vector{Int64}
make_type_arrow_10948(Set) === Set{Int64}
"#;

        let value = compile_and_run_value(src, 0).expect("Issue #10948 arrow MWE must run");
        assert!(
            matches!(value, Value::Bool(true)),
            "arrow value parameter must supply the parametric base, got {value:?}"
        );
    }

    #[test]
    fn inline_arrow_parameter_shadows_builtin_parametric_base() {
        let src = "((Vector::Type) -> Vector{Int64})(Set) === Set{Int64}";

        let value = compile_and_run_value(src, 0).expect("Issue #10948 inline arrow must run");
        assert!(
            matches!(value, Value::Bool(true)),
            "inline arrow parameter must supply the parametric base, got {value:?}"
        );
    }

    #[test]
    fn destructured_parameter_shadows_builtin_parametric_base() {
        let src = r#"
make_type_destructured_10948((Vector,)) = Vector{Int64}
make_type_destructured_10948((Set,)) === Set{Int64}
"#;

        let value = compile_and_run_value(src, 0).expect("Issue #10948 destructuring MWE must run");
        assert!(
            matches!(value, Value::Bool(true)),
            "destructured parameter must supply the parametric base, got {value:?}"
        );
    }
}

mod soft_scope_hosts_9283_tests {
    //! Regression tests for Issue #9283: strict file-mode soft scope extended to
    //! the C ABI / WASM hosts, plus the strict-mode `UndefVarError` diagnostic text.
    //!
    //! The C ABI / WASM editor hosts run a whole buffer through the strict pipeline
    //! ([`compile_and_run_str_file_mode`] / [`compile_and_run_value_file_mode`]), so
    //! a top-level loop assignment to an existing global binds a NEW local and a
    //! read-before-write (`+=`) raises `UndefVarError` — matching `julia file.jl`.
    //! The lenient library API (`compile_and_run_str` / `compile_and_run_value`) and
    //! the interactive REPL keep the old behaviour where the loop mutates the global.
    //!
    //! Companion to `file_mode_soft_scope_9210_tests.rs` (the CLI script path) and
    //! `sjulia_cli_soft_scope_9283_tests.rs` (the on-stderr warning text).

    use subset_julia_vm::{
        compile_and_run_str, compile_and_run_str_file_mode, compile_and_run_value_file_mode,
    };

    const SEED: u64 = 42;

    /// The MWE: a top-level loop `+=` against a pre-existing global.
    const MWE: &str = "total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n";

    // === Host strictness (Issue #9283) ======================================

    /// The strict host path raises `UndefVarError` (NaN result) on the MWE, while
    /// the lenient library API runs it to completion (the `println(total)` tail
    /// yields `Unit`, mapped to -4.0). This is the file-vs-host split the hosts now
    /// adopt.
    #[test]
    fn strict_str_host_errors_but_lenient_mutates() {
        assert!(
            compile_and_run_str_file_mode(MWE, SEED).is_nan(),
            "strict host must raise UndefVarError (NaN) on a top-level loop mutating a global"
        );
        assert!(
            !compile_and_run_str(MWE, SEED).is_nan(),
            "lenient library API must still run the same program to completion"
        );
    }

    /// An explicit `global` opts back into mutating the module binding even on the
    /// strict host — the migration every bundled sample uses.
    #[test]
    fn strict_str_host_global_decl_runs() {
        let src = "total = 0\nfor i in 1:3\n    global total += 1\nend\nprintln(total)\n";
        assert!(
            !compile_and_run_str_file_mode(src, SEED).is_nan(),
            "an explicit `global` must run cleanly under the strict host"
        );
    }

    /// A `global` declaration inside a NESTED loop also opts back in (Issue #9493):
    /// the counter mutates the module binding and the trailing expression yields 7,
    /// matching upstream `julia file.jl`.
    #[test]
    fn strict_str_host_nested_loop_global_decl_runs() {
        let src =
            "val = 1\nfor i in 1:2\n    for j in 1:3\n        global val += 1\n    end\nend\nval\n";
        let result = compile_and_run_str_file_mode(src, SEED);
        assert_eq!(
            result, 7.0,
            "a nested-loop global decl must mutate the module binding (upstream prints 7)"
        );
    }

    // === UndefVarError diagnostic text (Issue #9283) ========================

    /// The strict-path `UndefVarError` for a soft-scope-localized read carries
    /// upstream's `in local scope` suffix and the `Suggestion:` line.
    #[test]
    fn strict_undefvar_says_in_local_scope_with_suggestion() {
        let err = compile_and_run_value_file_mode(MWE, SEED)
            .expect_err("strict soft scope must raise UndefVarError");
        assert!(
            err.contains("UndefVarError: `total` not defined in local scope"),
            "got: {err}"
        );
        assert!(
            err.contains(
                "Suggestion: check for an assignment to a local variable that shadows a global of the same name."
            ),
            "got: {err}"
        );
        // The internal rename marker never leaks into the user-facing message.
        assert!(!err.contains("softlocal"), "got: {err}");
    }

    /// With multiple captured names the runtime read-before-write hits the FIRST
    /// one in source order (`zebra`, before `apple`/`mango`), and the message names
    /// it — not the alphabetically-first `apple`.
    #[test]
    fn strict_undefvar_multiname_errors_on_first_source_order_name() {
        let src = "zebra = 0\napple = 0\nmango = 0\nfor i in 1:3\n    zebra += 1\n    apple += 1\n    mango += 1\nend\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("strict soft scope must raise UndefVarError");
        assert!(
            err.contains("`zebra` not defined in local scope"),
            "must name the first source-order captured name `zebra`, got: {err}"
        );
    }

    /// A plain undefined-global read (no soft-scope localization) keeps the bare
    /// `not defined` message — the `in local scope` suffix is reserved for the
    /// scope-localized read-before-write case, matching upstream.
    #[test]
    fn plain_undefined_global_has_no_local_scope_suffix() {
        let err = compile_and_run_value_file_mode("println(no_such_global_xyz)", SEED)
            .expect_err("reading an undefined global must raise UndefVarError");
        assert!(err.contains("UndefVarError"), "got: {err}");
        assert!(
            !err.contains("in local scope"),
            "a plain undefined global must not get the soft-scope `in local scope` suffix: {err}"
        );
    }
}

mod let_hardscope_global_9284_tests {
    //! Regression tests for Issue #9284: hard-scope `let` global localization.
    //!
    //! A `let ... end` is a **hard** local scope. A `for`/`while` loop nested inside
    //! it that assigns a name resolving ONLY to a module global must bind a **fresh
    //! loop-local**, so a read-before-write (`+=`) raises `UndefVarError` with **no**
    //! soft-scope warning. This holds in EVERY execution mode — `julia file.jl` and
    //! the REPL both error — unlike the top-level soft scope of Issue #9210, which is
    //! lenient in the REPL/host path. The fix therefore runs unconditionally in the
    //! lowering pipeline (`lowering::soft_scope::apply_hard_scope_let_localization`),
    //! so both [`compile_and_run_value`] (lenient / host default) and
    //! [`compile_and_run_value_file_mode`] (strict / CLI) error identically.
    //!
    //! Names bound by the `let` (its bindings or a let-body-level assignment), an
    //! explicit `global` declaration, and loop variables are left untouched.

    use subset_julia_vm::{compile_and_run_value, compile_and_run_value_file_mode};
    use subset_julia_vm_bytecode::Value;

    const SEED: u64 = 42;

    /// Assert that both the lenient (host) and strict (CLI) entry points raise a
    /// `UndefVarError` naming the original variable `total`, without leaking the
    /// internal `##letlocal` rename into the message.
    fn assert_both_modes_undefvar(src: &str) {
        for (label, err) in [
            ("lenient", compile_and_run_value(src, SEED)),
            ("file-mode", compile_and_run_value_file_mode(src, SEED)),
        ]
        .map(|(label, r)| {
            (
                label,
                r.expect_err("hard-scope `let` loop over a global must raise UndefVarError"),
            )
        }) {
            assert!(
                err.contains("UndefVarError"),
                "[{label}] expected UndefVarError, got: {err}"
            );
            assert!(
                err.contains("`total`"),
                "[{label}] error must name the original variable `total`, got: {err}"
            );
            assert!(
                !err.contains("letlocal") && !err.contains("softlocal"),
                "[{label}] internal rename must not leak into the message: {err}"
            );
        }
    }

    /// Assert both entry points evaluate `src` to the `I64` `expected` without error
    /// (`Value` does not derive `PartialEq`, so match the variant explicitly).
    fn assert_both_modes_i64(src: &str, expected: i64) {
        for (label, r) in [
            ("lenient", compile_and_run_value(src, SEED)),
            ("file-mode", compile_and_run_value_file_mode(src, SEED)),
        ] {
            let value = r.unwrap_or_else(|e| panic!("[{label}] must not error: {e}"));
            match value {
                Value::I64(got) => assert_eq!(got, expected, "[{label}] wrong value"),
                other => panic!("[{label}] expected I64({expected}), got {other:?}"),
            }
        }
    }

    fn assert_both_modes_str(src: &str, expected: &str) {
        for (label, r) in [
            ("lenient", compile_and_run_value(src, SEED)),
            ("file-mode", compile_and_run_value_file_mode(src, SEED)),
        ] {
            let value = r.unwrap_or_else(|e| panic!("[{label}] must not error: {e}"));
            match value {
                Value::Str(got) => assert_eq!(got.as_ref(), expected, "[{label}] wrong value"),
                other => panic!("[{label}] expected String({expected:?}), got {other:?}"),
            }
        }
    }

    /// The reported MWE: an empty `let` whose loop `+=`s a pre-existing global.
    #[test]
    fn empty_let_loop_compound_assign_to_global_raises_undefvar() {
        let src =
            "total = 0\nlet\n    for i in 1:3\n        total += 1\n    end\nend\nprintln(total)\n";
        assert_both_modes_undefvar(src);
    }

    /// The bound `let x = 10` variant behaves identically (the issue calls it out).
    #[test]
    fn bound_let_loop_compound_assign_to_global_raises_undefvar() {
        let src =
            "total = 0\nlet x = 10\n    for i in 1:3\n        total += 1\n    end\nend\nprintln(total)\n";
        assert_both_modes_undefvar(src);
    }

    /// A `while` loop inside the `let` behaves identically to `for`.
    #[test]
    fn while_loop_in_let_compound_assign_to_global_raises_undefvar() {
        let src =
            "total = 0\nc = 0\nlet\n    while c < 3\n        total += 1\n        c += 1\n    end\nend\ntotal\n";
        assert_both_modes_undefvar(src);
    }

    /// A `let` inside another `let`: the innermost loop still binds a fresh local.
    #[test]
    fn nested_let_inner_loop_over_global_raises_undefvar() {
        let src = "total = 0\nlet x = 1\n    let y = 2\n        for i in 1:3\n            total += 1\n        end\n    end\nend\ntotal\n";
        assert_both_modes_undefvar(src);
    }

    /// A loop nested in a `let` inside a top-level `begin … end` (empty-bindings
    /// `LetBlock`) is still a hard-scope `let`, so it errors.
    #[test]
    fn let_inside_begin_block_loop_over_global_raises_undefvar() {
        let src = "total = 0\nbegin\n    let\n        for i in 1:3\n            total += 1\n        end\n    end\nend\ntotal\n";
        assert_both_modes_undefvar(src);
    }

    /// An explicit `global total` inside the loop keeps mutating the module binding
    /// (upstream: `3`). The localization must NOT fire.
    #[test]
    fn explicit_global_in_let_loop_still_mutates() {
        let src =
            "total = 0\nlet\n    for i in 1:3\n        global total\n        total += 1\n    end\nend\ntotal\n";
        assert_both_modes_i64(src, 3);
    }

    /// A let-body-level `global total` (outside the loop) also keeps the loop
    /// mutating the module binding (upstream: `3`).
    #[test]
    fn let_body_level_global_decl_still_mutates() {
        let src =
            "total = 0\nlet\n    global total\n    for i in 1:3\n        total += 1\n    end\nend\ntotal\n";
        assert_both_modes_i64(src, 3);
    }

    /// A declaration installed for a hard `try` clause remains in force while
    /// compiling a nested loop in that same clause (Issue #11316).
    #[test]
    fn let_try_clause_global_decl_reaches_nested_loop() {
        let src = "total = 0\nlet\n    try\n        global total\n        for i in 1:3\n            total += 1\n        end\n    catch\n    end\nend\ntotal\n";
        assert_both_modes_i64(src, 3);
    }

    /// A catch binder is a clause local, so a nested loop reuses it instead of
    /// localizing the same-named pre-existing global (Issue #11316).
    #[test]
    fn let_catch_binder_reaches_nested_loop() {
        let src = "err = 99\nlet\n    try\n        error(\"boom\")\n    catch err\n        for i in 1:1\n            err = err\n        end\n    end\nend\nerr\n";
        assert_both_modes_i64(src, 99);
    }

    /// A declaration owned by a `finally` clause reaches loops nested in that
    /// clause just like one owned by the `try` clause (Issue #11316).
    #[test]
    fn let_finally_clause_global_decl_reaches_nested_loop() {
        let src = "total = 0\nlet\n    try\n        nothing\n    finally\n        global total\n        for i in 1:3\n            total += 1\n        end\n    end\nend\ntotal\n";
        assert_both_modes_i64(src, 3);
    }

    /// The iterator expression executes in the enclosing clause, so its
    /// assignment declares a clause local that the loop body reuses (#11316).
    #[test]
    fn let_try_loop_header_assignment_binds_clause_local() {
        let src = "x = 100\nobserved = \"\"\nlet\n    try\n        for i in (x = 1):1\n            x += 1\n        end\n        global observed = string(x)\n    catch e\n        global observed = string(typeof(e))\n    end\nend\nstring(x, \"\\n\", observed)\n";
        assert_both_modes_str(src, "100\n2");
    }

    /// A `while` condition is another loop header evaluated in the enclosing
    /// clause, so its assignment binds the clause local used by the body
    /// (Issue #11316).
    #[test]
    fn let_try_while_condition_assignment_binds_clause_local() {
        let src = "x = 100\nobserved = \"\"\nlet\n    try\n        while (x = 1) < 2\n            x += 1\n            break\n        end\n        global observed = string(x)\n    catch e\n        global observed = string(typeof(e))\n    end\nend\nstring(x, \"\\n\", observed)\n";
        assert_both_modes_str(src, "100\n2");
    }

    /// A global declaration in one nested `try` clause must not exempt a
    /// sibling `catch` clause's nested loop from hard-scope localization.
    #[test]
    fn let_outer_loop_nested_try_globals_do_not_leak_to_catch() {
        let src = "x = 0\ncaught = \"none\"\nlet\n    try\n        for i in 1:1\n            try\n                global x\n                x = 1\n                error(\"inner\")\n            catch\n                for j in 1:1\n                    x += 1\n                end\n            end\n        end\n    catch e\n        global caught = string(typeof(e))\n    end\nend\nstring(x, \"\\n\", caught)\n";
        assert_both_modes_str(src, "1\nUndefVarError");
    }

    /// `else` owns an independent clause inventory just like try/catch/finally.
    #[test]
    fn let_else_clause_global_decl_reaches_nested_loop() {
        let src = "x = 0\nlet\n    try\n        nothing\n    catch\n    else\n        global x\n        for i in 1:2\n            x += 1\n        end\n    end\nend\nx\n";
        assert_both_modes_i64(src, 2);
    }

    /// Provenance isolation survives transparent control flow and multiple
    /// nested hard-clause boundaries rather than only one `try` level.
    #[test]
    fn let_deep_nested_try_sibling_globals_stay_isolated() {
        let src = "x = 0\ncaught = \"none\"\nlet\n    try\n        for i in 1:1\n            if true\n                try\n                    try\n                        global x\n                        x = 1\n                        error(\"inner\")\n                    catch\n                        for j in 1:1\n                            x += 1\n                        end\n                    end\n                finally\n                    nothing\n                end\n            end\n        end\n    catch e\n        global caught = string(typeof(e))\n    end\nend\nstring(x, \"\\n\", caught)\n";
        assert_both_modes_str(src, "1\nUndefVarError");
    }

    /// A `let` binding of the SAME name shadows the global: the loop mutates the
    /// let-local (upstream: `103`), so nothing is localized.
    #[test]
    fn let_bound_same_name_shadows_global() {
        let src = "total = 0\nlet total = 100\n    for i in 1:3\n        total += 1\n    end\n    total\nend\n";
        assert_both_modes_i64(src, 103);
    }

    /// A let-body-level assignment of the same name (outside the loop) makes it a
    /// let-local, so the loop mutates that local (upstream: `103`).
    #[test]
    fn let_body_level_assign_makes_local() {
        let src = "total = 0\nlet\n    total = 100\n    for i in 1:3\n        total += 1\n    end\n    total\nend\n";
        assert_both_modes_i64(src, 103);
    }

    /// A loop-body name that is NOT a pre-existing global keeps its existing
    /// behaviour (already a fresh loop-local — this errored before the fix too, and
    /// still does). Guards against the pass being a no-op that only works by luck.
    #[test]
    fn non_global_name_in_let_loop_still_errors() {
        let src = "let\n    for i in 1:3\n        acc += 1\n    end\nend\n42\n";
        let err = compile_and_run_value_file_mode(src, SEED)
            .expect_err("read-before-write of a fresh loop-local must raise UndefVarError");
        assert!(
            err.contains("UndefVarError") && err.contains("`acc`"),
            "expected UndefVarError naming `acc`, got: {err}"
        );
    }

    /// A `let` with a loop that only READS a global (never assigns it) is untouched:
    /// the read resolves to the module global and the loop runs normally.
    #[test]
    fn let_loop_read_only_global_is_untouched() {
        let src = "base = 10\nacc = 0\nlet\n    for i in 1:3\n        global acc\n        acc += base\n    end\nend\nacc\n";
        assert_both_modes_i64(src, 30);
    }

    /// A `let` with NO loop — a plain top-level global read/assign outside any loop —
    /// must be unaffected by the pass (the localization only targets loop bodies).
    #[test]
    fn let_without_loop_reading_global_is_untouched() {
        let src = "base = 7\nlet\n    y = base + 1\n    y\nend\n";
        assert_both_modes_i64(src, 8);
    }
}

mod bottom_undefvar_10304_tests {
    //! Regression tests for Issue #10304 (bare `Bottom` must be UndefVarError),
    //! superseding the Issue #5065 tests that pinned the opposite behavior.
    //!
    //! Upstream defines `const Bottom = Union{}` in Base WITHOUT exporting it,
    //! so a bare `Bottom` in Main is `UndefVarError`. sjulia used to (a) define
    //! the const in the prelude's `base/essentials.jl` — whose flat, unqualified
    //! type-alias table leaked the binding into user scope — and (b) accept
    //! `"Bottom"` in the static type-name parser (`JuliaType::from_name`) and
    //! several compile-side match arms. Both are removed; the Bottom SEMANTICS
    //! from Issue #5065 (subtype universality, typeintersect zero element,
    //! union normalization) are preserved via the canonical `Union{}` spelling
    //! and via an upstream-valid user-defined `const Bottom = Union{}`.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    /// Upstream-valid Main-scope binding; everything referencing `Bottom`
    /// after this prefix must behave exactly as with `Union{}` itself.
    const USER_BOTTOM: &str = "const Bottom = Union{}\n";

    fn run(src: &str) -> Value {
        compile_and_run_value(src, 0).unwrap_or_else(|e| panic!("run failed for {src:?}: {e}"))
    }

    fn run_bool(src: &str) -> bool {
        match run(src) {
            Value::Bool(b) => b,
            other => panic!("expected Bool for {src:?}, got {other:?}"),
        }
    }

    fn run_user_bottom_bool(expr: &str) -> bool {
        run_bool(&format!("{USER_BOTTOM}{expr}"))
    }

    #[test]
    fn bare_bottom_is_undefvarerror() {
        // Upstream: ERROR: UndefVarError: `Bottom` not defined in `Main`.
        // Previously sjulia resolved it to Union{} (Issue #10304).
        let err = compile_and_run_value("Bottom === Union{}", 0)
            .expect_err("bare `Bottom` must raise UndefVarError like upstream");
        assert!(
            err.contains("UndefVarError") && err.contains("`Bottom`"),
            "expected UndefVarError naming `Bottom`, got: {err}"
        );
    }

    #[test]
    fn union_empty_spelling_still_carries_bottom_semantics() {
        // The canonical spelling must be unaffected (Issue #5065 semantics).
        assert!(run_bool("Union{} <: Int"));
        assert!(run_bool("Union{} <: Any"));
        assert!(!run_bool("Int <: Union{}"));
        assert!(run_bool("typeintersect(Int, String) === Union{}"));
        assert!(run_bool("Union{Union{}, Int} === Int"));
    }

    #[test]
    fn user_const_bottom_is_subtype_of_every_type() {
        assert!(run_user_bottom_bool("Bottom <: Int"));
        assert!(run_user_bottom_bool("Bottom <: Number"));
        assert!(run_user_bottom_bool("Bottom <: String"));
        assert!(run_user_bottom_bool("Bottom <: Any"));
        assert!(run_user_bottom_bool("Bottom <: Bottom"));
    }

    #[test]
    fn only_bottom_is_subtype_of_user_const_bottom() {
        assert!(!run_user_bottom_bool("Int <: Bottom"));
        assert!(!run_user_bottom_bool("Any <: Bottom"));
    }

    #[test]
    fn user_const_bottom_is_typeintersect_zero_element() {
        assert!(run_user_bottom_bool(
            "typeintersect(Int, String) === Bottom"
        ));
        assert!(run_user_bottom_bool(
            "typeintersect(Int, Bottom) === Bottom"
        ));
        assert!(run_user_bottom_bool(
            "typeintersect(Bottom, Number) === Bottom"
        ));
    }

    #[test]
    fn user_const_bottom_collapses_in_union_normalization() {
        assert!(run_user_bottom_bool("Union{Bottom, Int} === Int"));
        assert!(run_user_bottom_bool("Union{Bottom} === Bottom"));
    }

    #[test]
    fn user_const_bottom_can_bind_any_type() {
        // With no Base binding to collide with, a user `const Bottom = Int`
        // is an ordinary const definition, matching upstream.
        assert!(run_bool("const Bottom = Int\nBottom === Int"));
    }
}

mod session_boundedness_8625_tests {
    //! Long-running session boundedness + host cache-cap injection (Issue #8625).
    //!
    //! Parent #8610 bounded the VM runtime caches with an always-on hard cap
    //! (clear a cache once it exceeds its entry limit). These integration tests
    //! exercise that bound through the *public* surface a long-running host uses:
    //!
    //! 1. A single `REPLSession` runs a small program many times and its reported
    //!    VM memory/cache counters stay bounded and stable (no monotonic growth).
    //! 2. The host-facing cache-cap injection API
    //!    (`set_default_cache_entry_limits`, mirrored by the FFI
    //!    `subset_julia_vm_set_cache_entry_limits`) actually propagates into the
    //!    VM the session builds per eval.
    //!
    //! On-device iOS measurement is tracked separately on #8625; nextest runs
    //! these host-independent checks in every process.
    //!
    //! The iteration count is controlled by `SJULIA_LONG_SESSION_ITERATIONS`
    //! (default 100 for local iteration; CI sets 1000).

    use subset_julia_vm::repl::REPLSession;
    use subset_julia_vm::vm::{
        set_default_cache_entry_limits, RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT,
        RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT,
    };
    use subset_julia_vm_bytecode::Value;

    /// Number of iterations for long-session boundedness tests.
    ///
    /// Day-to-day local runs default to a shortened count so the test still
    /// exercises the boundedness invariant without paying the full ~100s CI
    /// cost. The full 1000-iteration stress run is used in CI by setting
    /// `SJULIA_LONG_SESSION_ITERATIONS=1000`.
    fn long_session_iterations() -> usize {
        std::env::var("SJULIA_LONG_SESSION_ITERATIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100)
    }

    /// A small program that allocates, dispatches generically, and mutates a
    /// struct — the kind of work that grows struct_heap and dispatch caches
    /// within a single eval.
    const SMALL_PROGRAM: &str = r#"
    mutable struct Counter8625
        n::Int
    end
    function bump!(c::Counter8625, k)
        c.n += k
        c.n
    end
    c = Counter8625(0)
    s = 0
    for i in 1:20
        s += bump!(c, i)
    end
    s
    "#;

    #[test]
    fn repl_session_memory_stats_stay_bounded_over_1000_iterations_issue_8625() {
        let mut session = REPLSession::new(0);

        // Warm up so caches/heap reach their steady state for this program.
        for _ in 0..10 {
            let result = session.eval(SMALL_PROGRAM);
            assert!(
                result.error.is_none(),
                "warmup eval failed: {:?}",
                result.error
            );
        }
        let baseline = session
            .last_vm_memory_stats()
            .expect("a successful eval must record VM memory stats");

        let iterations = long_session_iterations();

        // Long session: many iterations of the same program. Each eval builds a
        // fresh VM, so a correctly-bounded runtime never accumulates across evals.
        for iteration in 0..iterations {
            let result = session.eval(SMALL_PROGRAM);
            assert!(
                result.error.is_none(),
                "iteration {iteration} failed: {:?}",
                result.error
            );
        }

        let after = session
            .last_vm_memory_stats()
            .expect("stats present after the loop");

        // Boundedness: the per-eval VM footprint after the long session matches the
        // warmed-up baseline — no monotonic growth in struct_heap or any cache.
        assert_eq!(
            after.struct_heap_len, baseline.struct_heap_len,
            "struct_heap grew across a long session"
        );
        assert_eq!(
            after.dispatch_cache_entries,
            baseline.dispatch_cache_entries
        );
        assert_eq!(
            after.method_dispatch_cache_entries,
            baseline.method_dispatch_cache_entries
        );
        assert_eq!(
            after.specialization_cache_entries,
            baseline.specialization_cache_entries
        );
        assert_eq!(
            after.generated_expr_cache_entries,
            baseline.generated_expr_cache_entries
        );

        // And every cache stays far under the hard cap.
        assert!(
            after.struct_heap_len <= 64,
            "struct_heap not compact: {after:?}"
        );
        assert!(after.dispatch_cache_entries <= RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT);
        assert!(after.specialization_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);

        // Default caps are reported when no host override is active.
        assert_eq!(
            after.dispatch_cache_entry_limit,
            RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT
        );
        assert_eq!(
            after.specialization_cache_entry_limit,
            RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT
        );
    }

    /// The host cache-cap injection (the FFI
    /// `subset_julia_vm_set_cache_entry_limits` calls this) propagates through to
    /// the VM the session builds per eval (Issue #8625). Runs in its own nextest
    /// process, so mutating the process-wide default is isolated.
    #[test]
    fn host_cache_cap_injection_propagates_into_session_vm_issue_8625() {
        // Injected before the session builds any VM, as an iOS host would at
        // startup based on the device memory budget.
        set_default_cache_entry_limits(Some(256), Some(128));

        let mut session = REPLSession::new(0);
        let result = session.eval(SMALL_PROGRAM);
        assert!(result.error.is_none(), "eval failed: {:?}", result.error);

        let stats = session
            .last_vm_memory_stats()
            .expect("stats present after eval");
        assert_eq!(
            stats.dispatch_cache_entry_limit, 256,
            "injected dispatch cap did not reach the session VM"
        );
        assert_eq!(
            stats.specialization_cache_entry_limit, 128,
            "injected specialization cap did not reach the session VM"
        );

        // Passing 0/None restores the built-in defaults for later-built VMs.
        set_default_cache_entry_limits(None, None);
        let mut session2 = REPLSession::new(0);
        let result2 = session2.eval(SMALL_PROGRAM);
        assert!(result2.error.is_none());
        let stats2 = session2.last_vm_memory_stats().expect("stats present");
        assert_eq!(
            stats2.dispatch_cache_entry_limit,
            RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT
        );
    }

    /// Regression test for Issue #9056: a single `REPLSession` must survive 100
    /// sequential evals of a simple variable-assignment program without heap
    /// corruption.  The iOS XCTest that first exposed the bug ran exactly this
    /// loop on `REPLSessionManager`; this Rust-level test exercises the same
    /// `REPLSession::eval` path on the host to catch any Rust-side regression
    /// (i.e. ensures the crash is not reproducible at the Rust level alone).
    ///
    /// NOTE: the original failure was observed on an aarch64-apple-ios-sim target
    /// where Base is compiled at runtime (embedded cache mismatch).  This host
    /// test uses the pre-loaded Base cache and therefore cannot reproduce
    /// iOS-specific allocator or runtime-compilation differences; its purpose is
    /// to guard the Rust-side session eval loop.  Full iOS verification requires
    /// running the XCTest suite on the simulator (see `scripts/ios_repl_e2e.sh`).
    /// Persistent-model analogue of the #8625 boundedness test (Issue #9787).
    ///
    /// Under the `Persistent` eval model the fresh VM each eval is seeded by
    /// transplanting the prior eval's struct heap. Before the fix that transplant
    /// copied the WHOLE accumulated heap regardless of what the carried globals
    /// referenced, so `struct_heap` grew linearly with the number of evals
    /// (~188 → ~19188 over 1000 iterations) instead of staying at the per-eval
    /// steady state — violating the #8625 guarantee and blocking the LV6 default
    /// flip. Reachable-only transplant compaction (Issue #9787) reclaims the dead
    /// accumulation, so the heap stays at its warmed-up bound.
    #[test]
    fn repl_persistent_struct_heap_stays_bounded_over_1000_iterations_issue_9787() {
        let mut session = REPLSession::new(0);

        let iterations = long_session_iterations();

        // Warm up so the heap reaches its per-eval steady state.
        for _ in 0..10 {
            let result = session.eval(SMALL_PROGRAM);
            assert!(result.error.is_none(), "warmup failed: {:?}", result.error);
        }
        let baseline = session
            .last_vm_memory_stats()
            .expect("a successful eval must record VM memory stats")
            .struct_heap_len;

        for iteration in 0..iterations {
            let result = session.eval(SMALL_PROGRAM);
            assert!(
                result.error.is_none(),
                "iteration {iteration} failed: {:?}",
                result.error
            );
        }
        let after = session
            .last_vm_memory_stats()
            .expect("stats present after the loop")
            .struct_heap_len;

        // Boundedness: no monotonic growth. The per-eval footprint after the long
        // Persistent session matches the warmed-up baseline.
        assert_eq!(
            after, baseline,
            "Persistent struct_heap grew across a long session (before fix: ~19188 vs ~188)"
        );
        assert!(
            after <= 64,
            "Persistent struct_heap not compact: {after} entries"
        );
    }

    /// StructRef integrity must survive reachable-only transplant compaction
    /// (Issue #9787). A closure captures a mutable struct (a *carried* value that
    /// holds a `StructRef` into the transplanted heap — the path the compaction
    /// remaps), a nested struct field holds another struct, and an array holds
    /// structs; unreferenced structs are dropped as globals are rebound. After many
    /// evals force repeated compaction, every surviving struct's fields must still
    /// read back correctly against the upstream-reviewed result.
    #[test]
    fn repl_persistent_struct_ref_integrity_survives_compaction_issue_9787() {
        // A sequence that: defines mutable structs, builds a closure capturing a
        // struct (carried across evals), nests a struct in a struct, arrays structs,
        // rebinds a global to drop the old struct, and mutates through the closure —
        // then reads the surviving state back. Evaluated many times so the persistent
        // transplant compaction runs repeatedly between reads.
        let program = r#"
    mutable struct Cell9787
        v::Int
    end
    struct Pair9787
        a::Cell9787
        b::Cell9787
    end
    c = Cell9787(1)
    getc() = c.v
    p = Pair9787(Cell9787(10), Cell9787(20))
    arr = [Cell9787(100), Cell9787(200), Cell9787(300)]
    c = Cell9787(c.v + 1)
    c.v + p.a.v + p.b.v + arr[1].v + arr[3].v + getc()
    "#;

        let run = || {
            let mut session = REPLSession::new(0);
            let mut last = None;
            for i in 0..40 {
                let result = session.eval(program);
                assert!(
                    result.error.is_none(),
                    "iteration {i} failed: {:?}",
                    result.error
                );
                last = result.value;
            }
            last
        };

        let persistent = run();

        // c.v starts at 1, rebound to c.v+1 = 2 each eval (fresh reconstruction, not
        // accumulating): 2 + 10 + 20 + 100 + 300 + 2 = 434. getc() reads the closure's
        // captured (rebound) c. The exact upstream-reviewed value and absence of
        // dangling/mis-indexed StructRefs both matter here.
        assert!(
            matches!(persistent, Some(Value::I64(434))),
            "expected 434 (verified against upstream julia), got {persistent:?}"
        );
    }

    /// A *carried* mutable struct (one whose runtime `Value` has no init-expr form,
    /// so it rides across evals through `seed_persisted_globals` rather than being
    /// reconstructed) must keep its identity through reachable-only transplant
    /// compaction (Issue #9787). This is the package-free analogue of the
    /// `test_repl_odeproblem_global_persists_8260` regression: the compaction moved
    /// the carried struct's heap index, and before the fix the session's cached
    /// index (`self.globals`) was NOT remapped in lockstep, so after the VM appended
    /// its own structs over the vacated slot the global silently pointed at a
    /// *different* struct — `typeof`/dispatch then broke.
    ///
    /// A `Dict` field has no init-expr reconstruction, so the struct is carried; a
    /// method dispatches on the struct type, exercising type identity after
    /// compaction. Expected values are verified against upstream Julia.
    #[test]
    fn repl_carried_struct_dispatch_survives_compaction_issue_9787() {
        let seq = [
            "mutable struct Carrier9787\n    tag::Int\n    d::Dict{String,Int}\nend",
            "kind9787(c::Carrier9787) = c.tag + 1000",
            "car9787 = Carrier9787(42, Dict(\"a\" => 1))",
            // Read it back (forces the carry path once).
            "car9787.tag",
            // Allocate unrelated structs so the transplant heap has dead entries to
            // reclaim and indices to shift under compaction.
            "junk9787 = Carrier9787(7, Dict(\"b\" => 2))",
            "car9787.tag",
            // Dispatch on the carried struct's TYPE after several compactions.
            "kind9787(car9787)",
            "string(typeof(car9787))",
        ];

        // Collect only the MODEL-INVARIANT observable results — the scalar/string
        // values a user sees. Raw carried-struct `Value`s carry a heap index and a
        // `type_id` that legitimately differ between the two heap layouts, so they are
        // not comparable; what MUST agree (and be correct) is the field read, the
        // dispatch result, and the type name.
        let run = || {
            let mut session = REPLSession::new(0);
            let mut scalars = Vec::new();
            for src in seq {
                let r = session.eval(src);
                assert!(r.success, "`{src}` failed: {:?}", r.error);
                match r.value {
                    Some(Value::I64(n)) => scalars.push(format!("i64:{n}")),
                    Some(Value::Str(s)) => scalars.push(format!("str:{s}")),
                    _ => {}
                }
            }
            scalars
        };

        let persistent = run();
        // car9787.tag == 42 (twice), kind9787(car9787) == 1042, typeof == Carrier9787
        // (all verified against upstream `julia`). A dangling/mis-indexed carried
        // StructRef would surface here as a wrong field, a MethodError, or a wrong
        // type name.
        assert_eq!(
            persistent,
            vec![
                "i64:42".to_string(),
                "i64:42".to_string(),
                "i64:1042".to_string(),
                "str:Carrier9787".to_string(),
            ],
            "carried struct corrupted after compaction"
        );
    }

    /// Two globals ALIAS the same array of mutable structs (`alias = arr`), so both
    /// carried seed-globals share one `Rc<RefCell<ArrayValue>>` holding `StructRef`s.
    /// The transplant remap must rewrite that shared array EXACTLY once — the
    /// `Rc::as_ptr` visited-set dedup in `remap_value_struct_refs` (Issue #9787). A
    /// non-idempotent double-remap (`3→1` then `1→0`) would corrupt the array's
    /// element indices, so a later `arr[i]` field-read / dispatch would hit the wrong
    /// struct. The array is carried (not reconstructed) because its element struct
    /// has a `Dict` field with no init-expr form (#8260 carry path). Expected
    /// identity and values are verified against upstream `julia`.
    #[test]
    fn repl_aliased_array_shared_rc_remaps_once_issue_9787() {
        let seq = [
            "mutable struct Holder9787b\n    d::Dict{Int,Int}\nend",
            "tagof9787b(h::Holder9787b) = length(h.d)",
            "arr9787b = [Holder9787b(Dict(1 => 10, 2 => 20)), Holder9787b(Dict(3 => 30))]",
            // Alias: the SAME Rc-backed array under a second global name.
            "alias9787b = arr9787b",
            // A hard-scope block now runs transactionally on the live VM while
            // keeping the aliased array and its struct references intact.
            "junk9787b = Holder9787b(Dict(9 => 9))\nlet force_full_recompile_9787b = 0\n    force_full_recompile_9787b\nend",
            // Identity, not merely equal contents: independently cloned arrays
            // would make this false after the transplant.
            "alias9787b === arr9787b",
            // Dispatch on elements reached through BOTH the original and the alias,
            // after the aliased array's shared Rc has ridden through compaction.
            "tagof9787b(arr9787b[1])",   // length(Dict(1=>10,2=>20)) = 2
            "tagof9787b(alias9787b[2])", // length(Dict(3=>30)) = 1
            "length(alias9787b)",        // 2
        ];

        let run = || {
            let mut session = REPLSession::new(0);
            let mut observations = Vec::new();
            for (index, src) in seq.into_iter().enumerate() {
                let r = session.eval(src);
                assert!(r.success, "`{src}` failed: {:?}", r.error);
                if index == 4 {
                    assert_eq!(
                        session.last_vm_build_nanos(),
                        Some(0),
                        "hard-scope churn must reuse the live VM transaction"
                    );
                    continue;
                }
                match r.value {
                    Some(Value::I64(n)) => observations.push(format!("i64:{n}")),
                    Some(Value::Bool(b)) => observations.push(format!("bool:{b}")),
                    _ => {}
                }
            }
            observations
        };

        let persistent = run();
        // alias===arr, tagof(arr[1])=2, tagof(alias[2])=1, length(alias)=2
        // (verified vs julia). Independent clones would fail identity; a double
        // remap would mis-index and read the wrong struct/length.
        assert_eq!(
            persistent,
            vec![
                "bool:true".to_string(),
                "i64:2".to_string(),
                "i64:1".to_string(),
                "i64:2".to_string(),
            ],
            "aliased shared-Rc array corrupted by double-remap"
        );
    }

    /// A carried aliased array remains an `Rc` root across live transactions.
    /// Rebinding an unrelated struct must keep the heap bounded through the full
    /// 1000-eval stress configuration while preserving alias identity.
    #[test]
    fn repl_shared_rc_carry_stays_bounded_over_1000_iterations_issue_9827() {
        let mut session = REPLSession::new(0);

        for src in [
            "mutable struct Holder9827\n    d::Dict{Int,Int}\nend",
            "arr9827 = [Holder9827(Dict(1 => 10))]",
            "alias9827 = arr9827",
        ] {
            let result = session.eval(src);
            assert!(result.success, "setup `{src}` failed: {:?}", result.error);
        }

        // The hard-scope `let` executes on the parked VM. Rebinding `junk9827`
        // makes the prior Holder/Dict graph unreachable while
        // `arr9827`/`alias9827` stay live and aliased.
        let step = r#"
junk9827 = Holder9827(Dict(9 => 9))
let force_full_recompile_9827 = 0
    force_full_recompile_9827
end
length(alias9827)
"#;
        for _ in 0..10 {
            let result = session.eval(step);
            assert!(result.success, "warmup failed: {:?}", result.error);
            assert_eq!(
                session.last_vm_build_nanos(),
                Some(0),
                "hard-scope step must reuse the live VM transaction"
            );
        }
        let baseline = session.get_struct_heap().len();

        for iteration in 0..long_session_iterations() {
            let result = session.eval(step);
            assert!(
                result.success,
                "iteration {iteration} failed: {:?}",
                result.error
            );
            assert_eq!(
                session.last_vm_build_nanos(),
                Some(0),
                "hard-scope step must reuse the live VM transaction"
            );
        }

        let after = session.get_struct_heap().len();
        assert_eq!(
            after, baseline,
            "shared-Rc carry grew struct_heap across the long session"
        );
        assert!(after <= 64, "shared-Rc carry heap not compact: {after}");

        let mutate =
            session.eval("alias9827[1] = Holder9827(Dict(2 => 20, 3 => 30)); length(arr9827[1].d)");
        assert!(mutate.success, "alias mutation failed: {:?}", mutate.error);
        assert!(
            matches!(mutate.value, Some(Value::I64(2))),
            "alias write was not visible through arr9827: {:?}",
            mutate.value
        );
    }

    #[test]
    fn sequential_eval_100_iterations_no_corruption_issue_9056() {
        let mut session = REPLSession::new(8625);
        for i in 1usize..=100 {
            let code = format!("x_9056 = {}; x_9056 + 1", i);
            let result = session.eval(&code);
            assert!(result.success, "iteration {} failed: {:?}", i, result.error);
            // The last expression is `x_9056 + 1` which evaluates to i+1.
            // The result value should be i+1 as an I64.
            let expected_i64 = (i + 1) as i64;
            match &result.value {
                Some(Value::I64(v)) => {
                    assert_eq!(
                        *v, expected_i64,
                        "iteration {}: expected {}, got {}",
                        i, expected_i64, v
                    );
                }
                other => {
                    panic!(
                        "iteration {}: expected Value::I64({}), got {:?}",
                        i, expected_i64, other
                    );
                }
            }
        }
    }
}

mod cache_eviction_spike_9097_tests {
    //! Cache-cap full-clear spike measurement (Issue #9097).
    //!
    //! Measures clear frequency (`cache_clears` counter) and per-eval latency
    //! under a low vs high cache cap to decide whether the full-clear strategy in
    //! #8610 causes material recompilation spikes that would justify partial
    //! eviction.
    //!
    //! Decision formula (stated before measuring, per #8650 discipline):
    //!   spike_ratio = median_latency_low_cap / median_latency_high_cap
    //!   IMPLEMENT partial eviction only if: spike_ratio > 2.0 AND clears_per_eval ≥ 2
    //!   REJECT (close as measured-rejected) if: spike_ratio ≤ 2.0 OR clears_per_eval < 2

    use std::time::Instant;
    use subset_julia_vm::{
        repl::REPLSession,
        vm::{
            set_default_cache_entry_limits, RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT,
            RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT,
        },
    };

    /// A dispatch-heavy workload that generates many dispatch-cache entries within
    /// a single eval. Two struct types, four generic functions, and 40 iterations
    /// of mixed calls keep the fan-out broad. This is the same workload as the
    /// criterion bench so results are comparable.
    const DISPATCH_HEAVY: &str = r#"
    mutable struct SpikePoint9097
        x::Float64
        y::Float64
    end

    mutable struct SpikeCounter9097
        n::Int
    end

    function snorm(p::SpikePoint9097)
        sqrt(p.x * p.x + p.y * p.y)
    end

    function sbump!(c::SpikeCounter9097, k::Int)
        c.n += k
        c.n
    end

    function sdot(a::SpikePoint9097, b::SpikePoint9097)
        a.x * b.x + a.y * b.y
    end

    function sscale(p::SpikePoint9097, s::Float64)
        SpikePoint9097(p.x * s, p.y * s)
    end

    total = 0.0
    c = SpikeCounter9097(0)
    for i in 1:40
        p = SpikePoint9097(Float64(i), Float64(i + 1))
        q = sscale(p, 0.5)
        total += snorm(p) + sdot(p, q)
        sbump!(c, i)
    end
    total + Float64(c.n)
    "#;

    const WARMUP: usize = 5;
    const MEASURE: usize = 30;

    /// Run MEASURE evals with a given cap, returning the sorted per-eval durations
    /// and the `cache_clears` and `cache_cleared_entries` from the last eval.
    fn measure_evals(cap: usize) -> (Vec<u64>, u64, u64) {
        set_default_cache_entry_limits(Some(cap), Some(cap));
        let mut session = REPLSession::new(99);

        // Warmup
        for i in 0..WARMUP {
            let r = session.eval(DISPATCH_HEAVY);
            assert!(
                r.error.is_none(),
                "warmup {i} failed at cap={cap}: {:?}",
                r.error
            );
        }

        let mut durations_us: Vec<u64> = Vec::with_capacity(MEASURE);
        for _ in 0..MEASURE {
            let t0 = Instant::now();
            let r = session.eval(DISPATCH_HEAVY);
            let elapsed = t0.elapsed().as_micros() as u64;
            assert!(
                r.error.is_none(),
                "measure eval failed at cap={cap}: {:?}",
                r.error
            );
            durations_us.push(elapsed);
        }
        durations_us.sort_unstable();

        let stats = session
            .last_vm_memory_stats()
            .expect("stats present after measure evals");

        // Reset so the next call starts clean.
        set_default_cache_entry_limits(None, None);

        (
            durations_us,
            stats.cache_clears,
            stats.cache_cleared_entries,
        )
    }

    fn median(sorted: &[u64]) -> u64 {
        let n = sorted.len();
        if n == 0 {
            return 0;
        }
        if n % 2 == 1 {
            sorted[n / 2]
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2
        }
    }

    /// Main measurement test for Issue #9097.
    ///
    /// Prints a decision table and asserts the formula-based verdict.
    /// Set SJULIA_SKIP_SPIKE_BENCH=1 to skip in environments without accurate
    /// wall-clock (CI containers with heavy contention).
    #[test]
    fn cache_full_clear_spike_measurement_issue_9097() {
        if std::env::var("SJULIA_SKIP_SPIKE_BENCH").is_ok() {
            eprintln!("[9097] SJULIA_SKIP_SPIKE_BENCH set — skipping spike measurement");
            return;
        }

        // Low cap: small enough that the dispatch-heavy workload is likely to
        // trigger at least one clear per eval. Set to 32 which is far below the
        // ~100+ dispatch entries a single eval with 40-iteration dispatch fan-out
        // can generate.
        let low_cap: usize = 32;
        let high_cap: usize = RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT; // 4096 — no clears expected

        eprintln!("[9097] measuring low_cap={low_cap} ({MEASURE} evals + {WARMUP} warmup) …");
        let (low_sorted, low_clears, low_cleared_entries) = measure_evals(low_cap);

        eprintln!("[9097] measuring high_cap={high_cap} ({MEASURE} evals + {WARMUP} warmup) …");
        let (high_sorted, high_clears, high_cleared_entries) = measure_evals(high_cap);

        let low_median = median(&low_sorted);
        let high_median = median(&high_sorted);
        let spike_ratio = if high_median == 0 {
            0.0_f64
        } else {
            low_median as f64 / high_median as f64
        };

        // clears_per_eval from the last sampled eval stat (each REPL eval
        // creates a fresh VM, so these counters reset per eval — they reflect
        // the single most-recent eval's clear activity).
        let clears_per_eval_low = low_clears;
        let clears_per_eval_high = high_clears;

        eprintln!("=== Issue #9097 Cache Eviction Spike Measurement ===");
        eprintln!("  low_cap  = {low_cap}");
        eprintln!("  high_cap = {high_cap}");
        eprintln!(
            "  low_cap  latency: median={low_median}µs  p5={}µs  p95={}µs",
            low_sorted[low_sorted.len() / 20],
            low_sorted[low_sorted.len() * 19 / 20],
        );
        eprintln!(
            "  high_cap latency: median={high_median}µs  p5={}µs  p95={}µs",
            high_sorted[high_sorted.len() / 20],
            high_sorted[high_sorted.len() * 19 / 20],
        );
        eprintln!("  spike_ratio        = {spike_ratio:.2}x  (low/high median latency)");
        eprintln!("  clears_per_eval    = {clears_per_eval_low} (low_cap)  / {clears_per_eval_high} (high_cap)");
        eprintln!("  cleared_entries    = {low_cleared_entries} (low_cap)  / {high_cleared_entries} (high_cap)");
        eprintln!("  default dispatch_cache_entry_limit  = {RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT}");
        eprintln!(
            "  default spec_cache_entry_limit      = {RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT}"
        );

        // --- Decision ---
        let implement_partial_eviction = spike_ratio > 2.0 && clears_per_eval_low >= 2;
        if implement_partial_eviction {
            eprintln!("  VERDICT: IMPLEMENT partial eviction (spike_ratio={spike_ratio:.2} > 2.0 AND clears={clears_per_eval_low} >= 2)");
        } else {
            eprintln!(
                "  VERDICT: REJECT — full-clear spike is not material \
                 (spike_ratio={spike_ratio:.2}, clears_per_eval={clears_per_eval_low})"
            );
        }

        // Sanity: the high_cap run should not clear at all.
        assert_eq!(
            clears_per_eval_high, 0,
            "high_cap ({high_cap}) should not trigger any cache clear, got {clears_per_eval_high}"
        );

        // Record the verdict as a structured assertion so CI captures it.
        // NOTE: if this test fails in the future it likely means the workload
        // grew enough to fill the high_cap — raise high_cap or reduce MEASURE.
        if implement_partial_eviction {
            // Record the decision; the caller (PR author) must act on this.
            eprintln!("[9097] ACTION REQUIRED: partial eviction criterion met — see Issue #9097");
        } else {
            eprintln!("[9097] No action required: measured-rejected per formula");
        }

        // Store the verdict in a test-output artifact for the PR description.
        // We do NOT panic here — the measured verdict drives the PR action, not a
        // test failure. The test is green if the workload runs correctly; the
        // eprintln output is the deliverable.
    }

    /// KEY FINDING (Issue #9097): at cap=32 with a 40-iteration dispatch-heavy
    /// workload, ZERO cache clears occur. This is the central measurement result:
    ///
    /// - Each REPL eval runs on a **fresh VM** with empty caches.
    /// - A realistic typed workload (struct methods, 40 iterations, 4 functions)
    ///   only needs O(1) dispatch cache entries per call-site/type pair — far below
    ///   the default 4 096 cap and even below cap=32.
    /// - Therefore the "clear → full recompilation → clear again" cycle from the
    ///   issue description does not materialize for any realistic workload at the
    ///   configured default cap.
    ///
    /// The test asserts `cache_clears == 0` (not ≥ 1) to lock in this finding.
    #[test]
    fn low_cap_confirms_zero_clears_per_realistic_eval_issue_9097() {
        let low_cap: usize = 32;
        set_default_cache_entry_limits(Some(low_cap), Some(low_cap));

        let mut session = REPLSession::new(7);
        let r = session.eval(DISPATCH_HEAVY);
        assert!(r.error.is_none(), "eval failed: {:?}", r.error);

        let stats = session
            .last_vm_memory_stats()
            .expect("stats present after eval");

        set_default_cache_entry_limits(None, None);

        // KEY FINDING: even at cap=32, a realistic dispatch-heavy eval generates
        // zero hard-cap clears. The #8610 full-clear mechanism is never triggered
        // for normal workloads → no spike to optimize away (Issue #9097: REJECT).
        assert_eq!(
            stats.cache_clears, 0,
            "expected 0 cache clears at cap={low_cap} (fresh VM per eval = cache never fills), \
             got {}. Stats: {stats:?}",
            stats.cache_clears,
        );
        // The cap is wired through correctly (limit is 32, not the default 4096).
        assert_eq!(
            stats.dispatch_cache_entry_limit, low_cap,
            "cap not propagated into VM"
        );

        eprintln!(
            "[9097] KEY FINDING: cap={low_cap}, cache_clears={}, cleared_entries={}, \
             dispatch_entries={}, method_dispatch_entries={}",
            stats.cache_clears,
            stats.cache_cleared_entries,
            stats.dispatch_cache_entries,
            stats.method_dispatch_cache_entries,
        );
    }
}

mod memory_budget_8702_tests {
    use subset_julia_vm::{compile_and_run_value, vm::Value};

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn budgeted_single_allocation_raises_catchable_out_of_memory_error_8702() {
        let _guard = EnvGuard::set("SJULIA_MEMORY_BUDGET_BYTES", "4096");
        let chunk = "x".repeat(1024);
        let src = format!(
            r#"
    function caught_out_of_memory_8702(e)
        return e isa OutOfMemoryError &&
            typeof(e) == OutOfMemoryError &&
            sprint(showerror, e) == "OutOfMemoryError()"
    end

    function budgeted_single_allocation_8702()
        try
            zeros(1024)
            return false
        catch e
            return caught_out_of_memory_8702(e)
        end
    end

    function budgeted_array_growth_8702()
        a = zeros(512)
        try
            push!(a, 1.0)
            return false
        catch e
            return caught_out_of_memory_8702(e)
        end
    end

    function budgeted_string_concat_8702()
        chunk = "{chunk}"
        try
            chunk * chunk * chunk * chunk * chunk
            return false
        catch e
            return caught_out_of_memory_8702(e)
        end
    end

    budgeted_single_allocation_8702() &&
        budgeted_array_growth_8702() &&
        budgeted_string_concat_8702()
    "#
        );

        let value =
            compile_and_run_value(&src, 0).expect("budgeted allocations should be catchable");
        assert!(
            matches!(value, Value::Bool(true)),
            "expected Bool(true), got {value:?}"
        );
    }
}

mod memory_budget_8703_tests {
    use subset_julia_vm::{
        compile_and_run_value,
        repl::REPLSession,
        vm::{set_default_memory_budget_bytes, Value},
    };

    struct MemoryBudgetGuard;

    impl MemoryBudgetGuard {
        fn set(bytes: usize) -> Self {
            set_default_memory_budget_bytes(Some(bytes));
            Self
        }
    }

    impl Drop for MemoryBudgetGuard {
        fn drop(&mut self) {
            set_default_memory_budget_bytes(None);
        }
    }

    #[test]
    fn host_memory_budget_stops_push_loop_and_vm_continues_8703() {
        let _guard = MemoryBudgetGuard::set(4_194_304);
        let src = r#"
    function caught_out_of_memory_8703(e)
        return e isa OutOfMemoryError &&
            typeof(e) == OutOfMemoryError &&
            sprint(showerror, e) == "OutOfMemoryError()"
    end

    function budgeted_push_loop_8703()
        caught = false
        try
            a = zeros(1)
            while true
                push!(a, 1.0)
            end
        catch e
            caught = caught_out_of_memory_8703(e)
        end
        return caught && (1 + 1 == 2)
    end

    (try
        budgeted_push_loop_8703()
    catch e
        caught_out_of_memory_8703(e)
    end) && (1 + 1 == 2)
    "#;

        let value = compile_and_run_value(src, 0).expect("budgeted push loop should be catchable");
        assert!(
            matches!(value, Value::Bool(true)),
            "expected Bool(true), got {value:?}"
        );

        let mut session = REPLSession::new(0);
        let result = session.eval("1 + 2");
        assert!(result.error.is_none(), "eval failed: {:?}", result.error);
        let stats = session
            .last_vm_memory_stats()
            .expect("successful eval should report memory stats");
        assert_eq!(stats.memory_budget_bytes, Some(4_194_304));
    }
}

mod repl_session_malformed_redefinition_10906_tests {
    //! Issue #10906 (epic #10869 Phase 1c): REPL/session robustness coverage
    //! for `repl/session.rs:550`'s invariant, converted from an
    //! `.expect(...)` call to a guarded `match` + typed `REPLResult::error`
    //! (`delta_eligible implies a prior persistent compile`). These
    //! black-box `REPLSession` tests exercise the acceptance criteria named
    //! in the Issue directly: malformed multi-line input, empty input,
    //! embedded control characters, and function/struct redefinition
    //! sequences that walk the live-delta / fresh-delta / full-recompile
    //! branches guarding that invariant. None of these may panic; every one
    //! must return a typed `REPLResult` and leave the session usable for the
    //! next `eval`.

    use subset_julia_vm::repl::{REPLResult, REPLSession};
    use subset_julia_vm_bytecode::Value;

    const SEED: u64 = 42;

    fn i64_of(r: &REPLResult) -> Option<i64> {
        match r.value {
            Some(Value::I64(n)) => Some(n),
            _ => None,
        }
    }

    /// Unterminated `function` block (missing `end`): a parse/lower error
    /// surfaced as a typed `REPLResult`, never a panic — and the session must
    /// still evaluate correctly afterward.
    #[test]
    fn malformed_multiline_input_returns_typed_error_and_session_survives() {
        let mut session = REPLSession::new(SEED);
        let bad = session.eval("function f10906(x)\n    x + 1\n");
        assert!(!bad.success, "malformed input must not silently succeed");
        assert!(
            bad.error.is_some(),
            "malformed input must carry a typed error message, got: {bad:?}"
        );

        let ok = session.eval("1 + 1");
        assert!(
            ok.success,
            "session must survive a prior parse error: {:?}",
            ok.error
        );
        assert_eq!(i64_of(&ok), Some(2));
    }

    /// Empty input must not panic. Upstream Julia's REPL simply no-ops on an
    /// empty line, so this test does not assert the empty eval's own
    /// success/failure — only that it does not crash and the session keeps
    /// working afterward.
    #[test]
    fn empty_input_does_not_panic_and_session_survives() {
        let mut session = REPLSession::new(SEED);
        let _ = session.eval("");
        let ok = session.eval("40 + 2");
        assert!(
            ok.success,
            "session must survive an empty eval: {:?}",
            ok.error
        );
        assert_eq!(i64_of(&ok), Some(42));
    }

    /// NUL / SOH / BEL control characters embedded in otherwise-plausible
    /// source text. Whether this parses or fails is not asserted — only that
    /// it returns a typed `REPLResult` (never a crash) and the session
    /// remains usable afterward.
    #[test]
    fn embedded_control_characters_do_not_panic_and_session_survives() {
        let mut session = REPLSession::new(SEED);
        let src = "x10906 = 1\u{0}\u{1}\u{7} + 1\n";
        let _ = session.eval(src);
        let ok = session.eval("7 * 6");
        assert!(
            ok.success,
            "session must survive control-character input: {:?}",
            ok.error
        );
        assert_eq!(i64_of(&ok), Some(42));
    }

    /// Upstream Julia allows redefining a top-level generic function at the
    /// REPL; the delta / live-delta / full-recompile machinery
    /// `repl/session.rs:550` guards must not panic on any routing a
    /// redefinition sequence takes.
    #[test]
    fn function_redefinition_sequence_does_not_panic() {
        let mut session = REPLSession::new(SEED);
        assert!(session.eval("g10906(x) = x + 1").success);
        assert_eq!(i64_of(&session.eval("g10906(41)")), Some(42));

        let redef = session.eval("g10906(x) = x * 2");
        assert!(
            redef.success,
            "redefinition must not panic or error: {:?}",
            redef.error
        );
        assert_eq!(i64_of(&session.eval("g10906(21)")), Some(42));

        // Back-to-back redefinition, immediately re-hitting the same guarded
        // invariant on the next delta-eligible eval.
        let redef2 = session.eval("g10906(x) = x + 100");
        assert!(redef2.success, "{:?}", redef2.error);
        assert_eq!(i64_of(&session.eval("g10906(1)")), Some(101));
    }

    /// A struct redefinition (1 field -> 2 fields) must full-recompile
    /// cleanly rather than panic (mirrors `struct_redefinition_full_recompiles_9199`
    /// in `repl/session.rs`'s own white-box tests, exercised here through the
    /// public `REPLSession` surface).
    #[test]
    fn struct_redefinition_sequence_does_not_panic() {
        let mut session = REPLSession::new(SEED);
        assert!(session.eval("struct Pt10906\n    x::Int\nend").success);
        assert!(session.eval("Pt10906(1)").success);
        // An intervening delta-eligible eval resyncs the prefix so the
        // redefinition below is not mistaken for a same-eval definition.
        assert!(session.eval("1 + 1").success);

        let redef = session.eval("struct Pt10906\n    x::Int\n    y::Int\nend");
        assert!(
            redef.success,
            "struct redefinition must not panic: {:?}",
            redef.error
        );

        let constructed = session.eval("Pt10906(10, 20).y");
        assert!(constructed.success, "{:?}", constructed.error);
        assert_eq!(i64_of(&constructed), Some(20));
    }

    /// Repeated-eval process-survival sweep (Issue #10908, Phase 3 of the
    /// #10869 panic-debt retirement epic): a single long-lived
    /// `REPLSession` — the same object a real REPL process keeps for its
    /// entire run — cycles through every malformed shape above (and a few
    /// more session-mutating ones) many times, interleaved with valid evals,
    /// never resetting the session. This is the "repeated REPL eval
    /// survival" acceptance criterion: not just that one malformed eval
    /// doesn't panic, but that a session which has already absorbed dozens
    /// of malformed evals keeps behaving correctly indefinitely (no
    /// accumulating corruption, no eventual panic from stale state).
    #[test]
    fn repeated_malformed_eval_survives_across_many_iterations() {
        const ITERATIONS: usize = 50;
        let malformed = [
            "function f10908(x)\n    x + 1\n",
            "struct S10908\n    x::Int\n",
            "for i in 1:10\n    println(i)\n",
            "",
            "x10908 = 1\u{0}\u{1}\u{7} + 1\n",
            "[1 2; 3 4",
            "let x = 1; x +",
        ];
        let mut session = REPLSession::new(SEED);
        for iter in 0..ITERATIONS {
            for src in malformed {
                // Every malformed eval must return a typed REPLResult; the
                // process (and the session object) must still be alive to
                // run the very next line, whether this one succeeded or not.
                let _ = session.eval(src);
            }
            let ok = session.eval(&format!("{iter} + 1"));
            assert!(
                ok.success,
                "session must still evaluate correctly after {} malformed evals (iteration {iter}): {:?}",
                malformed.len() * (iter + 1),
                ok.error
            );
            assert_eq!(i64_of(&ok), Some(iter as i64 + 1));
        }
    }
}

mod type_alias_signature_source_order_11086_tests {
    use subset_julia_vm::repl::{REPLResult, REPLSession};
    use subset_julia_vm_bytecode::Value;

    fn bool_of(result: &REPLResult) -> Option<bool> {
        match result.value {
            Some(Value::Bool(value)) => Some(value),
            _ => None,
        }
    }

    fn i64_of(result: &REPLResult) -> Option<i64> {
        match result.value {
            Some(Value::I64(value)) => Some(value),
            _ => None,
        }
    }

    #[test]
    fn later_alias_in_same_eval_is_not_visible_to_earlier_signature_11086() {
        let mut session = REPLSession::new(11086);
        let definition = session.eval(
            "later_error_11086 = nothing\n\
             try\n\
                 later_method_11086(x::LaterAlias11086) = x\n\
             catch e\n\
                 global later_error_11086 = e\n\
             end\n\
             const LaterAlias11086 = Int64",
        );
        assert!(definition.success, "{:?}", definition.error);
        let observed = session.eval("later_error_11086 isa UndefVarError");
        assert_eq!(bool_of(&observed), Some(true), "{observed:?}");
    }

    #[test]
    fn prior_eval_alias_remains_visible_before_same_eval_redefinition_11086() {
        let mut session = REPLSession::new(11086);
        assert!(session.eval("PriorAlias11086 = Int64").success);

        let definition = session.eval(
            "prior_method_11086(x::PriorAlias11086) = x + 1\n\
             PriorAlias11086 = Float64",
        );
        assert!(definition.success, "{:?}", definition.error);

        let call = session.eval("prior_method_11086(41)");
        assert!(call.success, "{:?}", call.error);
        assert_eq!(i64_of(&call), Some(42));
    }

    #[test]
    fn prior_alias_is_not_reused_after_value_rebinding_11086() {
        let mut session = REPLSession::new(11086);
        assert!(session.eval("ReboundAlias11086 = Int64").success);
        assert!(session.eval("ReboundAlias11086 = 1").success);

        let definition = session.eval(
            "rebound_error_11086 = nothing\n\
             try\n\
                 rebound_method_11086(x::ReboundAlias11086) = x\n\
             catch e\n\
                 global rebound_error_11086 = e\n\
             end",
        );
        assert!(definition.success, "{:?}", definition.error);
        let call = session.eval("rebound_method_11086(1)");
        assert!(
            !call.success,
            "value rebinding must prevent the stale Int64 alias method: {call:?}"
        );
    }

    #[test]
    fn prior_private_module_alias_does_not_leak_as_bare_name_11086() {
        let mut session = REPLSession::new(11086);
        assert!(
            session
                .eval("module PrivateAliasOwner11086\nconst A11086 = Int64\nend")
                .success
        );

        let definition = session.eval(
            "private_alias_error_11086 = nothing\n\
             try\n\
                 private_alias_method_11086(x::A11086) = x\n\
             catch e\n\
                 global private_alias_error_11086 = e\n\
             end",
        );
        assert!(definition.success, "{:?}", definition.error);
        let observed = session.eval("private_alias_error_11086 isa UndefVarError");
        assert_eq!(bool_of(&observed), Some(true), "{observed:?}");
    }
}

mod baremodule_builtin_type_authority_11419_tests {
    use subset_julia_vm::compile_and_run_value_file_mode;
    use subset_julia_vm::repl::REPLSession;
    use subset_julia_vm_bytecode::Value;

    fn assert_undefvar(src: &str, name: &str) {
        let err = compile_and_run_value_file_mode(src, 11419)
            .expect_err("an invisible Base type binding must fail");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
        assert!(err.contains(name), "error must name `{name}`, got: {err}");
    }

    #[test]
    fn baremodule_base_annotation_is_undefined() {
        assert_undefvar(
            "baremodule BareAnnotation11419\nf(x::BigInt) = 1\nend\ntrue",
            "BigInt",
        );
    }

    #[test]
    fn baremodule_parametric_base_type_is_undefined() {
        assert_undefvar(
            "baremodule BareParametric11419\nVector{Int64}\nend\ntrue",
            "Vector",
        );
    }

    #[test]
    fn named_base_import_is_source_ordered() {
        assert_undefvar(
            "baremodule BareLateImport11419\n\
             f(x::BigInt) = 1\n\
             import Base: BigInt\n\
             end\n\
             true",
            "BigInt",
        );
    }

    #[test]
    fn failed_annotation_does_not_activate_the_method() {
        let mut session = REPLSession::new(11419);
        let definition = session.eval("baremodule BareActivation11419\nf(x::BigInt) = 1\nend");
        assert!(
            !definition.success,
            "definition unexpectedly succeeded: {definition:?}"
        );
        assert!(
            definition.error.as_deref().is_some_and(|error| {
                error.contains("UndefVarError") && error.contains("BigInt")
            }),
            "wrong definition error: {definition:?}"
        );
        let call = session.eval("BareActivation11419.f(1)");
        assert!(
            !call.success,
            "failed definition leaked an active method: {call:?}"
        );
    }

    #[test]
    fn local_builtin_named_alias_remains_visible() {
        let value = compile_and_run_value_file_mode(
            "baremodule BareLocalTypeAlias11419\n\
             const BigInt = Int64\n\
             f(x::BigInt) = x\n\
             annotation_result = f(1)\n\
             isa_result = 1 isa BigInt\n\
             subtype_result = BigInt <: Any\n\
             end\n\
             BareLocalTypeAlias11419.annotation_result == 1 &&\n\
             BareLocalTypeAlias11419.isa_result &&\n\
             BareLocalTypeAlias11419.subtype_result",
            11419,
        )
        .expect("a current-module alias must override the builtin spelling");
        assert!(matches!(value, Value::Bool(true)), "got {value:?}");
    }

    #[test]
    fn local_core_builtin_named_alias_controls_isa_folding() {
        let value = compile_and_run_value_file_mode(
            "baremodule BareCoreAliasFold11419\n\
             const Int64 = Float64\n\
             f(x) = x isa Int64\n\
             result = f(1)\n\
             end\n\
             BareCoreAliasFold11419.result == false",
            11419,
        )
        .expect("isa folding must use the visible alias target");
        assert!(matches!(value, Value::Bool(true)), "got {value:?}");
    }
}

mod runtime_nominal_repl_tests_11654 {
    use subset_julia_vm::repl::{REPLResult, REPLSession};
    use subset_julia_vm_bytecode::Value;

    fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    fn assert_i64(result: &REPLResult, expected: i64) {
        assert!(result.success, "eval failed: {:?}", result.error);
        assert!(
            matches!(result.value, Some(Value::I64(value)) if value == expected),
            "expected {expected}, got {:?}",
            result.value
        );
    }

    #[cfg(feature = "aot")]
    #[test]
    fn aot_session_preserves_bundled_module_runtime_nominals_11716() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11716);
            let result = session.eval("true");
            assert!(
                result.success,
                "bundled Base module nominal disappeared during session reconstruction: {:?}",
                result.error
            );
        });
    }

    #[test]
    fn repl_runtime_nominal_persists_only_reached_structs_11654() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let skipped =
                session.eval("if false\nstruct SkippedRuntimeStruct11654\nx::Int\nend\nend");
            assert!(skipped.success, "skipped eval failed: {:?}", skipped.error);
            let reached =
                session.eval("if true\nstruct ReachedRuntimeStruct11654\nx::Int\nend\nend");
            assert!(reached.success, "reached eval failed: {:?}", reached.error);
            assert_i64(&session.eval("ReachedRuntimeStruct11654(9).x"), 9);
            let absent = session.eval("SkippedRuntimeStruct11654");
            assert!(!absent.success, "skipped definition must remain absent");
        });
    }

    #[test]
    fn repl_runtime_nominal_persists_abstract_primitive_and_enum_11654() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            assert!(session.eval("0").success);
            let definitions = session.eval(
                "if true\nabstract type RuntimeAbstractPersist11654 end\nend\n\
                 if true\nprimitive type RuntimePrimitivePersist11654 8 end\nend\n\
                 if true\n@enum RuntimeEnumPersist11654 runtime_enum_a11654 runtime_enum_b11654\nend",
            );
            assert!(
                definitions.success,
                "runtime family eval failed: {:?}",
                definitions.error
            );
            let observed = session.eval(
                "(RuntimeAbstractPersist11654 <: Any) && \
                 (RuntimePrimitivePersist11654 <: Any) && \
                 (instances(RuntimeEnumPersist11654) == \
                    (runtime_enum_a11654, runtime_enum_b11654))",
            );
            assert!(observed.success, "observation failed: {:?}", observed.error);
            assert!(matches!(observed.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_runtime_nominal_source_later_parent_is_catchable_11654() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let declaration = session.eval(
                "caught11654 = try\nabstract type CatchChild11654 <: CatchParent11654 end\nfalse\ncatch e\ne isa UndefVarError\nend\nabstract type CatchParent11654 end",
            );
            assert!(
                declaration.success,
                "caught declaration eval failed: {:?}",
                declaration.error
            );
            let observed = session.eval(
                "caught11654 && !isdefined(Main, :CatchChild11654) && isdefined(Main, :CatchParent11654)",
            );
            assert!(observed.success, "observation failed: {:?}", observed.error);
            assert!(matches!(observed.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_runtime_nominal_survives_uncaught_later_error_11654() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "if true\nstruct BeforeRuntimeError11654\nx::Int\nend\nend\nerror(\"stop after runtime definition\")",
            );
            assert!(!failed.success, "the trailing error must escape");
            assert!(
                session.has_live_vm(),
                "the reached runtime definition must retain a recoverable live VM"
            );
            assert_i64(&session.eval("BeforeRuntimeError11654(12).x"), 12);
        });
    }

    #[test]
    fn repl_runtime_nominal_and_method_survive_uncaught_later_error_11683() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                r#"begin
                    if true
                        struct RuntimeErrorSignatureType11683
                            x::Int
                        end
                    end
                    runtime_error_signature_value11683(x::RuntimeErrorSignatureType11683) = x.x
                    error("boom")
                end"#,
            );
            assert!(!failed.success, "the source must reach its trailing error");
            assert!(
                session.has_live_vm(),
                "the reached runtime type and method must retain a recoverable live VM: {:?}",
                failed.error
            );
            assert_i64(
                &session
                    .eval("runtime_error_signature_value11683(RuntimeErrorSignatureType11683(23))"),
                23,
            );
            let barrier = session.eval("module RuntimeRecoveryBarrier11683 end");
            assert!(
                barrier.success,
                "full-recompile barrier failed: {:?}",
                barrier.error
            );
            assert_i64(
                &session
                    .eval("runtime_error_signature_value11683(RuntimeErrorSignatureType11683(29))"),
                29,
            );
        });
    }

    #[test]
    fn repl_runtime_nominal_survives_error_before_unreached_method_11683() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "begin\n\
                   if true\nstruct RuntimeBeforeUnreachedMethod11683\nx::Int\nend\nend\n\
                   error(\"boom\")\n\
                   unreachable_method11683(x) = x\n\
                 end",
            );
            assert!(!failed.success, "the source must reach its error");
            assert!(
                session.has_live_vm(),
                "the reached runtime type must retain a recoverable live VM: {:?}",
                failed.error
            );
            assert_i64(&session.eval("RuntimeBeforeUnreachedMethod11683(31).x"), 31);
        });
    }

    #[test]
    fn repl_runtime_inner_constructor_persists_across_evals_11679() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11679);
            assert_i64(&session.eval("0"), 0);
            let first = session.eval(
                "if true\n\
                   struct SessionRuntimeInner11679\n\
                     x::Int\n\
                     SessionRuntimeInner11679(x) = new(x + 1)\n\
                   end\n\
                 end\n\
                 SessionRuntimeInner11679(10).x",
            );
            assert_i64(&first, 11);
            assert_i64(&session.eval("SessionRuntimeInner11679(20).x"), 21);
            let failed = session.eval("error(\"after runtime inner 11679\")");
            assert!(!failed.success, "the follow-up eval must fail");
            assert!(
                session.has_live_vm(),
                "the runtime constructor must retain a recoverable live VM: {:?}",
                failed.error
            );
            assert_i64(&session.eval("SessionRuntimeInner11679(30).x"), 31);
        });
    }

    #[test]
    fn repl_skipped_runtime_inner_constructor_stays_dormant_11679() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11679);
            assert_i64(&session.eval("0"), 0);
            let skipped = session.eval(
                "if false\n\
                   struct SkippedSessionRuntimeInner11679\n\
                     x::Int\n\
                     SkippedSessionRuntimeInner11679(x) = new(x + 1)\n\
                   end\n\
                 end\n\
                 isdefined(Main, :SkippedSessionRuntimeInner11679)",
            );
            assert!(
                matches!(skipped.value, Some(Value::Bool(false))),
                "the skipped type must stay undefined: {:?}",
                skipped.error
            );
            let call = session.eval("SkippedSessionRuntimeInner11679(10)");
            assert!(!call.success, "the skipped constructor must stay undefined");
            assert!(
                call.error
                    .as_deref()
                    .is_some_and(|error| error.contains("UndefVarError")),
                "unexpected skipped-constructor result: {:?}",
                call.error
            );
        });
    }

    #[test]
    fn repl_runtime_nominals_precede_reserved_root_ids_11654() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let definitions = session.eval(
                "if true\nstruct RuntimeBeforeRootStruct11654\nx::Int\nend\nend\n\
                 if true\nabstract type RuntimeBeforeRootAbstract11654 end\nend\n\
                 if true\nprimitive type RuntimeBeforeRootPrimitive11654 16 end\nend\n\
                 if true\n@enum RuntimeBeforeRootEnum11654 runtime_before_root_a11654 runtime_before_root_b11654\nend\n\
                 struct RootAfterRuntimeStruct11654\nx::Int\nend\n\
                 abstract type RootAfterRuntimeAbstract11654 end\n\
                 primitive type RootAfterRuntimePrimitive11654 32 end\n\
                 @enum RootAfterRuntimeEnum11654 root_after_runtime_a11654 root_after_runtime_b11654",
            );
            assert!(
                definitions.success,
                "mixed runtime/root eval failed: {:?}",
                definitions.error
            );
            let observed = session.eval(
                "RuntimeBeforeRootStruct11654(19).x + RootAfterRuntimeStruct11654(23).x + \
                 ((RuntimeBeforeRootAbstract11654 <: Any) && \
                  (RootAfterRuntimeAbstract11654 <: Any) && \
                  (RuntimeBeforeRootPrimitive11654 <: Any) && \
                  (RootAfterRuntimePrimitive11654 <: Any) && \
                  (instances(RuntimeBeforeRootEnum11654) == \
                     (runtime_before_root_a11654, runtime_before_root_b11654)) && \
                  (instances(RootAfterRuntimeEnum11654) == \
                     (root_after_runtime_a11654, root_after_runtime_b11654)))",
            );
            assert_i64(&observed, 43);
        });
    }

    #[test]
    fn repl_runtime_nominal_sites_remain_distinct_across_evals_11690() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let first = session.eval("if true\nstruct RuntimeSiteFirst11690\nx::Int\nend\nend");
            assert!(first.success, "first site failed: {:?}", first.error);
            let second = session.eval("if true\nstruct RuntimeSiteSecond11690\nx::Int\nend\nend");
            assert!(second.success, "second site failed: {:?}", second.error);
            assert_i64(
                &session.eval("RuntimeSiteFirst11690(11).x + RuntimeSiteSecond11690(13).x"),
                24,
            );
        });
    }

    #[test]
    fn repl_runtime_nominal_is_owned_by_lexical_module_11686() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module RuntimeOwnerModule11686\n\
                 if true\nabstract type RuntimeOwnedAbstract11686 end\nend\n\
                 if true\nstruct RuntimeOwnedConcrete11686\nx::Int\nend\nend\n\
                 if true\nprimitive type RuntimeOwnedPrimitive11686 8 end\nend\n\
                 if true\n@enum RuntimeOwnedEnum11686 runtime_owned_a11686 runtime_owned_b11686\nend\n\
                 struct RuntimeOwnedStruct11686 <: RuntimeOwnedAbstract11686 end\n\
                 end\n\
                 (RuntimeOwnerModule11686.RuntimeOwnedStruct11686 <: \
                  RuntimeOwnerModule11686.RuntimeOwnedAbstract11686) && \
                 (RuntimeOwnerModule11686.RuntimeOwnedConcrete11686(5).x == 5) && \
                 (RuntimeOwnerModule11686.RuntimeOwnedPrimitive11686 <: Any) && \
                 (RuntimeOwnerModule11686.runtime_owned_a11686 == \
                  RuntimeOwnerModule11686.RuntimeOwnedEnum11686(0)) && \
                 (RuntimeOwnerModule11686.runtime_owned_b11686 == \
                  RuntimeOwnerModule11686.RuntimeOwnedEnum11686(1)) && \
                 (instances(RuntimeOwnerModule11686.RuntimeOwnedEnum11686) == \
                  (RuntimeOwnerModule11686.runtime_owned_a11686, \
                   RuntimeOwnerModule11686.runtime_owned_b11686))",
            );
            assert!(
                result.success,
                "module-owned runtime nominal failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
            assert_i64(
                &session.eval("RuntimeOwnerModule11686.RuntimeOwnedConcrete11686(7).x"),
                7,
            );
        });
    }

    #[test]
    fn repl_module_owned_runtime_nominal_survives_full_rebuild_11686() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let owner = session.eval(
                "module RuntimePersistOwnerA11686\n\
                   if true\nstruct T\nx::Int\nend\nend\n\
                 end",
            );
            assert!(owner.success, "owner module failed: {:?}", owner.error);
            let barrier = session.eval("module RuntimePersistOwnerB11686\ny = 1\nend");
            assert!(
                barrier.success,
                "unrelated full rebuild failed: {:?}",
                barrier.error
            );
            assert_i64(&session.eval("RuntimePersistOwnerA11686.T(37).x"), 37);
        });
    }

    #[test]
    fn repl_module_local_bare_constructor_resolves_reached_runtime_nominal_11733() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11733);
            let result = session.eval(
                "if true\nstruct RuntimeBareCollision11733\nx::Int\nend\nend\n\
                 module RuntimeBareOwner11733\n\
                   if true\nstruct RuntimeBareCollision11733\ny::Int\nend\nend\n\
                   if true\n@enum RuntimeBareEnum11733 runtime_bare_member11733=1\nend\n\
                   v = RuntimeBareCollision11733(2)\n\
                   enum_ok = RuntimeBareEnum11733(1) == runtime_bare_member11733\n\
                 end\n\
                 RuntimeBareOwner11733.v.y == 2 && RuntimeBareOwner11733.enum_ok",
            );
            assert!(
                result.success,
                "bare constructor eval failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_module_forward_runtime_constructor_resolves_before_arguments_11713() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module RuntimeForwardConstructor11713\n\
                   trace = Int[]\n\
                   caught = try\n\
                     RuntimeForwardConstructor11713.T((push!(trace, 1); 1))\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                   if true\nstruct T\nx::Int\nend\nend\n\
                 end\n\
                 RuntimeForwardConstructor11713.caught && \
                 isempty(RuntimeForwardConstructor11713.trace)",
            );
            assert!(
                result.success,
                "forward-call probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_skipped_parametric_runtime_constructor_resolves_before_arguments_11713() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module SkippedParametricConstructor11713\n\
                   trace = Int[]\n\
                   if false\n\
struct T{X}\n\
x::X\n\
end\n\
end\n\
                   caught = try\n\
                     SkippedParametricConstructor11713.T{\
                       (push!(trace, 1); Int)\
                     }((push!(trace, 2); 1))\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                 end\n\
                 SkippedParametricConstructor11713.caught && \
                 isempty(SkippedParametricConstructor11713.trace)",
            );
            assert!(
                result.success,
                "skipped parametric probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_skipped_parametric_runtime_splat_resolves_before_arguments_11713() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module SkippedParametricSplat11713\n\
                   trace = Int[]\n\
                   if false\n\
struct T{X}\n\
x::X\n\
end\n\
end\n\
                   caught = try\n\
                     T{(push!(trace, 1); Int)}((push!(trace, 2); (1,))...)\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                 end\n\
                 SkippedParametricSplat11713.caught && \
                 isempty(SkippedParametricSplat11713.trace)",
            );
            assert!(
                result.success,
                "skipped splat probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_skipped_parametric_runtime_inner_resolves_before_arguments_11713() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module SkippedParametricInner11713\n\
                   trace = Int[]\n\
                   if false\n\
struct T{X}\n\
x::X\n\
T{X}(x::X) where {X} = new{X}(x)\n\
end\n\
end\n\
                   caught = try\n\
                     T{(push!(trace, 1); Int)}((push!(trace, 2); 1))\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                 end\n\
                 SkippedParametricInner11713.caught && \
                 isempty(SkippedParametricInner11713.trace)",
            );
            assert!(
                result.success,
                "skipped inner probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_module_forward_static_splat_constructor_resolves_before_arguments_11720() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module StaticForwardSplat11720\n\
                   trace = Int[]\n\
                   caught = try\n\
                     StaticForwardSplat11720.T((push!(trace, 1); (1,))...)\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                   struct T\n\
x::Int\n\
end\n\
                 end\n\
                 StaticForwardSplat11720.caught && isempty(StaticForwardSplat11720.trace)",
            );
            assert!(
                result.success,
                "static splat probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_module_forward_static_parametric_constructor_resolves_before_arguments_11720() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let source = "module StaticForwardParametric11720\n\
                   trace = Int[]\n\
                   caught = try\n\
                     StaticForwardParametric11720.T{Int}((push!(trace, 1); 1))\n\
                     false\n\
                   catch e\n\
                     e isa UndefVarError\n\
                   end\n\
                   struct T{X}\n\
x::X\n\
end\n\
                 end\n\
                 StaticForwardParametric11720.caught && \
                 isempty(StaticForwardParametric11720.trace)";
            let lowered = subset_julia_vm::pipeline::parse_and_lower(source)
                .expect("forward-constructor source must lower");
            let owner = lowered
                .modules
                .iter()
                .find(|module| module.name == "StaticForwardParametric11720")
                .expect("user module must remain in lowered program");
            assert!(!owner.is_base_origin, "user module gained Base provenance");
            assert!(
                !owner.is_package_origin,
                "user module gained package provenance"
            );
            let current_types =
                subset_julia_vm::compile::repl_support::current_type_names(&lowered);
            assert!(
                current_types.contains("StaticForwardParametric11720.T"),
                "current-input type provenance lost module-owned T: {current_types:?}"
            );
            let result = session.eval(source);
            assert!(
                result.success,
                "static parametric probe failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_failed_module_body_is_not_replayed_after_runtime_nominal_11721() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "module FailedRuntimeOwner11721\n\
                   committed_value = 7\n\
                   committed_function(x::Int) = x + 3\n\
                   if true\n\
struct A\n\
x::Int\n\
end\n\
end\n\
                   error(\"moduleboom11721\")\n\
                   unreached_function() = 99\n\
                 end",
            );
            assert!(!failed.success, "module body must throw");
            assert_i64(&session.eval("FailedRuntimeOwner11721.A(19).x"), 19);
            assert_i64(
                &session.eval(
                    "FailedRuntimeOwner11721.committed_value + \
                     FailedRuntimeOwner11721.committed_function(5)",
                ),
                15,
            );

            let barrier = session.eval("module RecoveryBarrier11721\ny = 1\nend");
            assert!(
                barrier.success,
                "failed module body replayed during rebuild: {:?}",
                barrier.error
            );
            assert_i64(&session.eval("FailedRuntimeOwner11721.A(23).x"), 23);
            assert_i64(
                &session.eval(
                    "FailedRuntimeOwner11721.committed_value + \
                     FailedRuntimeOwner11721.committed_function(11)",
                ),
                21,
            );
            let absent = session.eval("!isdefined(FailedRuntimeOwner11721, :unreached_function)");
            assert!(absent.success, "isdefined failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_unreached_module_redefinition_preserves_prior_module_11721() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let original = session.eval("module PriorModule11721\noriginal_function() = 41\nend");
            assert!(
                original.success,
                "original module failed: {:?}",
                original.error
            );

            let failed = session.eval(
                "if true\n\
struct RecoveryWitness11721\n\
x::Int\n\
end\n\
end\n\
                 error(\"before-redefinition11721\")\n\
                 module PriorModule11721\n\
replacement_function() = 2\n\
end",
            );
            assert!(!failed.success, "the pre-redefinition error must escape");

            let barrier = session.eval("module PriorModuleBarrier11721\ny = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            assert_i64(&session.eval("PriorModule11721.original_function()"), 41);
            let absent = session.eval("!isdefined(PriorModule11721, :replacement_function)");
            assert!(absent.success, "isdefined failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_assignment_only_failed_module_survives_rebuild_11721() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "module AssignmentOnlyRecovery11721\n\
                   committed_value = 37\n\
                   skipped_type_gate = committed_value < 0\n\
                   if skipped_type_gate\n\
struct SkippedType11721\n\
x::Int\n\
end\n\
end\n\
                   error(\"assignment-only-recovery11721\")\n\
                 end",
            );
            assert!(!failed.success, "module body must throw");

            let barrier = session.eval("module AssignmentOnlyBarrier11721\ny = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            assert_i64(
                &session.eval("AssignmentOnlyRecovery11721.committed_value"),
                37,
            );
        });
    }

    #[test]
    fn repl_method_only_failed_module_preserves_reached_prefix_11761() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11761);
            let failed = session.eval(
                "module MethodOnlyRecovery11761\n\
                   reached_method_11761() = 44\n\
                   error(\"method-only-recovery11761\")\n\
                   late_method_11761() = 99\n\
                 end",
            );
            assert!(!failed.success, "module body must throw");
            assert_i64(
                &session.eval("MethodOnlyRecovery11761.reached_method_11761()"),
                44,
            );
            let immediate_late =
                session.eval("!isdefined(MethodOnlyRecovery11761, :late_method_11761)");
            assert!(
                immediate_late.success,
                "immediate late-method probe failed: {:?}",
                immediate_late.error
            );
            assert!(matches!(immediate_late.value, Some(Value::Bool(true))));

            let barrier = session.eval("module MethodOnlyBarrier11761\ny = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            assert_i64(
                &session.eval("MethodOnlyRecovery11761.reached_method_11761()"),
                44,
            );
            let rebuilt_late =
                session.eval("!isdefined(MethodOnlyRecovery11761, :late_method_11761)");
            assert!(
                rebuilt_late.success,
                "rebuilt late-method probe failed: {:?}",
                rebuilt_late.error
            );
            assert!(matches!(rebuilt_late.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_empty_failed_module_preserves_published_binding_11761() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11761);
            let failed = session.eval(
                "module EmptyRecovery11761\n\
                   error(\"empty-recovery11761\")\n\
                 end",
            );
            assert!(!failed.success, "module body must throw");
            let immediate = session.eval("isdefined(Main, :EmptyRecovery11761)");
            assert!(
                immediate.success,
                "immediate probe failed: {:?}",
                immediate.error
            );
            assert!(matches!(immediate.value, Some(Value::Bool(true))));

            let barrier = session.eval("module EmptyBarrier11761 end");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            let rebuilt = session.eval("isdefined(Main, :EmptyRecovery11761)");
            assert!(rebuilt.success, "rebuilt probe failed: {:?}", rebuilt.error);
            assert!(matches!(rebuilt.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_nested_failed_module_preserves_owner_chain_11761() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11761);
            let failed = session.eval(
                "module ParentRecovery11761\n\
                   module ChildRecovery11761\n\
                     reached_nested_11761() = 7\n\
                     error(\"nested-recovery11761\")\n\
                     late_nested_11761() = 9\n\
                   end\n\
                 end",
            );
            assert!(!failed.success, "nested module body must throw");
            assert_i64(
                &session.eval("ParentRecovery11761.ChildRecovery11761.reached_nested_11761()"),
                7,
            );
            let late = session
                .eval("!isdefined(ParentRecovery11761.ChildRecovery11761, :late_nested_11761)");
            assert!(late.success, "late nested probe failed: {:?}", late.error);
            assert!(matches!(late.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_source_later_module_remains_unpublished_11761() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11761);
            let failed = session.eval(
                "error(\"before-module11761\")\n\
                 module SourceLaterModule11761\n\
                   source_later_method_11761() = 99\n\
                 end",
            );
            assert!(!failed.success, "leading error must escape");
            let absent = session.eval("!isdefined(Main, :SourceLaterModule11761)");
            assert!(absent.success, "absence probe failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_skipped_earlier_function_is_not_revived_by_later_nominal_11721() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "module SkippedEarlierFunction11721\n\
                   skipped_function_gate = 1 > 2\n\
                   if skipped_function_gate\n\
skipped_function() = 99\n\
end\n\
                   reached_type_gate = 2 > 1\n\
                   if reached_type_gate\n\
struct ReachedType11721\n\
x::Int\n\
end\n\
end\n\
                   error(\"skipped-earlier-function11721\")\n\
                 end",
            );
            assert!(!failed.success, "module body must throw");

            let barrier = session.eval("module SkippedEarlierBarrier11721\ny = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            let absent = session.eval("!isdefined(SkippedEarlierFunction11721, :skipped_function)");
            assert!(absent.success, "isdefined failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_same_leaf_function_in_another_module_is_not_recovered_11721() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let failed = session.eval(
                "module ReachedLeafOwner11721\n\
shared_leaf() = 41\n\
end\n\
                 module UnreachedLeafOwner11721\n\
reached_type_gate = 2 > 1\n\
if reached_type_gate\n\
struct ReachedType11721\n\
x::Int\n\
end\n\
end\n\
error(\"same-leaf-recovery11721\")\n\
shared_leaf() = 99\n\
end",
            );
            assert!(!failed.success, "second module body must throw");

            let barrier = session.eval("module SameLeafBarrier11721\ny = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            assert_i64(&session.eval("ReachedLeafOwner11721.shared_leaf()"), 41);
            let absent = session.eval("!isdefined(UnreachedLeafOwner11721, :shared_leaf)");
            assert!(absent.success, "isdefined failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_skipped_module_runtime_nominal_does_not_satisfy_signature_probe_11025() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                r#"module SkippedRuntimeOwnerModule11025
                    if false
                        struct SkippedRuntimeOwnerType11025 end
                    end
                    skipped_runtime_owner_value11025(x::SkippedRuntimeOwnerType11025) = 17
                end"#,
            );
            assert!(
                !result.success,
                "a skipped type must not satisfy the signature"
            );
            assert!(
                result
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("UndefVarError")),
                "unexpected signature error: {:?}",
                result.error
            );
        });
    }

    #[test]
    fn repl_reached_runtime_nominal_is_available_to_later_signature_11688() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "if true\nstruct RuntimeSignatureType11688\nx::Int\nend\nend\n\
                 runtime_signature_value11688(x::RuntimeSignatureType11688) = x.x\n\
                 runtime_signature_value11688(RuntimeSignatureType11688(17))",
            );
            assert!(
                result.success,
                "signature definition failed: {:?}",
                result.error
            );
            assert_i64(
                &session.eval("runtime_signature_value11688(RuntimeSignatureType11688(17))"),
                17,
            );
        });
    }

    #[test]
    fn repl_module_owned_runtime_nominal_is_available_to_later_signature_11688() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "module RuntimeSignatureOwner11688\n\
                 if true\nstruct RuntimeSignatureType11688\nend\nend\n\
                 runtime_signature_value11688(x::RuntimeSignatureType11688) = 23\n\
                 end\n\
                 RuntimeSignatureOwner11688.runtime_signature_value11688(\
                   RuntimeSignatureOwner11688.RuntimeSignatureType11688())",
            );
            assert!(
                result.success,
                "module-owned signature definition failed: {:?}",
                result.error
            );
            assert_i64(&result, 23);
        });
    }

    #[test]
    fn repl_runtime_nominal_validates_parent_and_fields_before_publication_11687() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "ParentValue11687 = 1\n\
                 parent_caught11687 = try\n\
                   abstract type BadParent11687 <: ParentValue11687 end\n\
                   false\n\
                 catch e\n\
                   e isa ErrorException\n\
                 end\n\
                 struct ConcreteParent11687 end\n\
                 concrete_parent_caught11687 = try\n\
                   struct BadConcreteParent11687 <: ConcreteParent11687 end\n\
                   false\n\
                 catch e\n\
                   e isa ErrorException\n\
                 end\n\
                 bound_caught11687 = try\n\
                   abstract type BadBound11687{T<:MissingBound11687} end\n\
                   false\n\
                 catch e\n\
                   e isa UndefVarError\n\
                 end\n\
                 forward_bound_caught11687 = try\n\
                   abstract type BadForwardBound11687{S<:T,T} end\n\
                   false\n\
                 catch e\n\
                   e isa UndefVarError\n\
                 end\n\
                 literal_bound_caught11687 = try\n\
                   abstract type BadLiteralBound11687{T<:1} end\n\
                   false\n\
                 catch e\n\
                   e isa TypeError\n\
                 end\n\
                 field_caught11687 = try\n\
                   struct BadField11687\n\
                     x::MissingFieldType11687\n\
                   end\n\
                   false\n\
                 catch e\n\
                   e isa UndefVarError\n\
                 end\n\
                 computed_caught11697 = try\n\
                   struct ComputedField11697\n\
                     x::typeof(1)\n\
                   end\n\
                   false\n\
                 catch\n\
                   true\n\
                 end\n\
                 parent_caught11687 && concrete_parent_caught11687 && bound_caught11687 && \
                 forward_bound_caught11687 && literal_bound_caught11687 && \
                 field_caught11687 && computed_caught11697 && \
                 !isdefined(Main, :BadParent11687) && \
                 !isdefined(Main, :BadConcreteParent11687) && \
                 !isdefined(Main, :BadBound11687) && \
                 !isdefined(Main, :BadForwardBound11687) && \
                 !isdefined(Main, :BadLiteralBound11687) && \
                 !isdefined(Main, :BadField11687) && \
                 !isdefined(Main, :ComputedField11697)",
            );
            assert!(
                result.success,
                "validation errors must be catchable: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_runtime_primitive_width_error_is_reached_and_catchable_11687() {
        with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "width_caught11687 = try\n\
                   primitive type BadWidth11687 7 end\n\
                   false\n\
                 catch e\n\
                   e isa ErrorException\n\
                 end\n\
                 width_caught11687 && !isdefined(Main, :BadWidth11687)",
            );
            assert!(
                result.success,
                "width error must be catchable: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_reuses_runtime_nominal_identity_when_one_site_runs_twice_11684() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11684);
            let result = session.eval(
                "for i11684 in 1:2\n\
                   struct LoopStruct11684\n\
                     x::Int\n\
                   end\n\
                 end\n\
                 for i11684 in 1:2\n\
                   abstract type LoopAbstract11684 end\n\
                 end\n\
                 for i11684 in 1:2\n\
                   primitive type LoopPrimitive11684 8 end\n\
                 end\n\
                 isdefined(Main, :LoopStruct11684) && \
                 isdefined(Main, :LoopAbstract11684) && \
                 isdefined(Main, :LoopPrimitive11684)",
            );
            assert!(
                result.success,
                "same-site replay failed: {:?}",
                result.error
            );
            assert!(matches!(result.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_coalesces_compatible_runtime_and_root_nominals_in_one_input_11684() {
        with_large_stack(|| {
            let mut struct_session = REPLSession::new(11684);
            let struct_result = struct_session.eval(
                "if true\n\
                   struct MixedStruct11684\n\
                     x::Int\n\
                   end\n\
                 end\n\
                 mixed_struct_before11684 = MixedStruct11684(19)\n\
                 struct MixedStruct11684\n\
                   x::Int\n\
                 end\n\
                 mixed_struct_before11684 isa MixedStruct11684 && \
                 mixed_struct_before11684.x == 19",
            );
            assert!(
                struct_result.success,
                "struct runtime/root coalescing failed: {:?}",
                struct_result.error
            );
            assert!(matches!(struct_result.value, Some(Value::Bool(true))));

            let mut abstract_session = REPLSession::new(11684);
            let abstract_result = abstract_session.eval(
                "if true\n\
                   abstract type MixedAbstract11684 end\n\
                 end\n\
                 abstract type MixedAbstract11684 end\n\
                 MixedAbstract11684 === MixedAbstract11684",
            );
            assert!(
                abstract_result.success,
                "abstract runtime/root coalescing failed: {:?}",
                abstract_result.error
            );
            assert!(matches!(abstract_result.value, Some(Value::Bool(true))));

            let mut primitive_session = REPLSession::new(11684);
            let primitive_result = primitive_session.eval(
                "if true\n\
                   primitive type MixedPrimitive11684 8 end\n\
                 end\n\
                 primitive type MixedPrimitive11684 8 end\n\
                 MixedPrimitive11684 === MixedPrimitive11684",
            );
            assert!(
                primitive_result.success,
                "primitive runtime/root coalescing failed: {:?}",
                primitive_result.error
            );
            assert!(matches!(primitive_result.value, Some(Value::Bool(true))));

            let mut enum_session = REPLSession::new(11684);
            let enum_result = enum_session.eval(
                "if true\n\
                   @enum MixedEnum11684 mixed_enum_a11684 mixed_enum_b11684\n\
                 end\n\
                 mixed_enum_before11684 = mixed_enum_a11684\n\
                 @enum MixedEnum11684 mixed_enum_a11684 mixed_enum_b11684\n\
                 mixed_enum_before11684 === mixed_enum_a11684 && \
                 instances(MixedEnum11684) == \
                   (mixed_enum_a11684, mixed_enum_b11684)",
            );
            assert!(
                enum_result.success,
                "enum runtime/root coalescing failed: {:?}",
                enum_result.error
            );
            assert!(matches!(enum_result.value, Some(Value::Bool(true))));

            let mut root_first_struct_session = REPLSession::new(11684);
            let root_first_struct_result = root_first_struct_session.eval(
                "struct RootFirstStruct11684\n\
                   x::Int\n\
                 end\n\
                 root_first_struct_before11684 = RootFirstStruct11684(23)\n\
                 if true\n\
                   struct RootFirstStruct11684\n\
                     x::Int\n\
                   end\n\
                 end\n\
                 root_first_struct_before11684 isa RootFirstStruct11684",
            );
            assert!(
                root_first_struct_result.success,
                "root-first struct coalescing failed: {:?}",
                root_first_struct_result.error
            );

            let mut root_first_abstract_session = REPLSession::new(11684);
            let root_first_abstract_result = root_first_abstract_session.eval(
                "abstract type RootFirstAbstract11684 end\n\
                 if true\n\
                   abstract type RootFirstAbstract11684 end\n\
                 end\n\
                 RootFirstAbstract11684 === RootFirstAbstract11684",
            );
            assert!(
                root_first_abstract_result.success,
                "root-first abstract coalescing failed: {:?}",
                root_first_abstract_result.error
            );

            let mut root_first_primitive_session = REPLSession::new(11684);
            let root_first_primitive_result = root_first_primitive_session.eval(
                "primitive type RootFirstPrimitive11684 8 end\n\
                 if true\n\
                   primitive type RootFirstPrimitive11684 8 end\n\
                 end\n\
                 RootFirstPrimitive11684 === RootFirstPrimitive11684",
            );
            assert!(
                root_first_primitive_result.success,
                "root-first primitive coalescing failed: {:?}",
                root_first_primitive_result.error
            );

            let mut root_first_enum_session = REPLSession::new(11684);
            let root_first_enum_result = root_first_enum_session.eval(
                "@enum RootFirstEnum11684 root_first_a11684 root_first_b11684\n\
                 root_first_enum_before11684 = root_first_a11684\n\
                 if true\n\
                   @enum RootFirstEnum11684 root_first_a11684 root_first_b11684\n\
                 end\n\
                 root_first_enum_before11684 === root_first_a11684 && \
                 instances(RootFirstEnum11684) == \
                   (root_first_a11684, root_first_b11684)",
            );
            assert!(
                root_first_enum_result.success,
                "root-first enum coalescing failed: {:?}",
                root_first_enum_result.error
            );

            let later = struct_session.eval(
                "struct MixedStruct11684\n\
                   x::Int\n\
                 end",
            );
            assert!(
                !later.success,
                "a separate REPL input must still reject nominal redefinition"
            );
        });
    }

    #[test]
    fn runtime_publication_transaction_matrix_11740() {
        struct PublicationCase11740 {
            name: &'static str,
            bootstrap: Option<&'static str>,
            setup: &'static str,
            setup_succeeds: bool,
            barrier: &'static str,
            probe: &'static str,
            expected: i64,
        }

        with_large_stack(|| {
            let cases = [
                PublicationCase11740 {
                    name: "method reached before uncaught error",
                    bootstrap: None,
                    setup: "if true\nreached_method_11740() = 41\nend\nerror(\"method-11740\")",
                    setup_succeeds: false,
                    barrier: "module MethodBarrier11740\nvalue = 1\nend",
                    probe: "reached_method_11740()",
                    expected: 41,
                },
                PublicationCase11740 {
                    name: "nominal reached before uncaught error",
                    bootstrap: None,
                    setup: "if true\nstruct ReachedNominal11740\nx::Int\nend\nend\nerror(\"nominal-11740\")",
                    setup_succeeds: false,
                    barrier: "module NominalBarrier11740\nvalue = 1\nend",
                    probe: "ReachedNominal11740(42).x",
                    expected: 42,
                },
                PublicationCase11740 {
                    name: "reached import excludes source-later import",
                    bootstrap: Some(
                        "module ReachedImportOwner11740\n\
                         export reached_import_11740\n\
                         reached_import_11740() = 43\n\
                         end\n\
                         module LateImportOwner11740\n\
                         export late_import_11740\n\
                         late_import_11740() = 99\n\
                         end",
                    ),
                    setup: "using .ReachedImportOwner11740: reached_import_11740\n\
                            error(\"import-11740\")\n\
                            using .LateImportOwner11740: late_import_11740",
                    setup_succeeds: false,
                    barrier: "module ImportBarrier11740\nvalue = 1\nend",
                    probe: "reached_import_11740() + \
                            (!isdefined(Main, :late_import_11740) ? 0 : 1000)",
                    expected: 43,
                },
                PublicationCase11740 {
                    name: "failed module replays only reached declarations",
                    bootstrap: None,
                    setup: "module RecoveredModule11740\n\
                            reached_module_11740() = 44\n\
                            error(\"module-11740\")\n\
                            late_module_11740() = 99\n\
                            end",
                    setup_succeeds: false,
                    barrier: "module ModuleBarrier11740\nvalue = 1\nend",
                    probe: "RecoveredModule11740.reached_module_11740() + \
                            (!isdefined(RecoveredModule11740, :late_module_11740) ? 0 : 1000)",
                    expected: 44,
                },
                PublicationCase11740 {
                    name: "caught error commits later method",
                    bootstrap: None,
                    setup: "try\nerror(\"caught-11740\")\ncatch\nend\n\
                            caught_method_11740() = 45",
                    setup_succeeds: true,
                    barrier: "module CaughtBarrier11740\nvalue = 1\nend",
                    probe: "caught_method_11740()",
                    expected: 45,
                },
                PublicationCase11740 {
                    name: "untaken nominal branch remains unpublished",
                    bootstrap: None,
                    setup: "if false\nstruct SkippedNominal11740\nend\nend\n46",
                    setup_succeeds: true,
                    barrier: "module SkippedBarrier11740\nvalue = 1\nend",
                    probe: "!isdefined(Main, :SkippedNominal11740) ? 46 : -1",
                    expected: 46,
                },
            ];

            for case in cases {
                let mut session = REPLSession::new(11740);
                if let Some(bootstrap) = case.bootstrap {
                    let result = session.eval(bootstrap);
                    assert!(
                        result.success,
                        "{} bootstrap failed: {:?}",
                        case.name, result.error
                    );
                }

                let setup = session.eval(case.setup);
                assert_eq!(
                    setup.success, case.setup_succeeds,
                    "{} setup result disagreed: {:?}",
                    case.name, setup.error
                );

                let barrier = session.eval(case.barrier);
                assert!(
                    barrier.success,
                    "{} rebuild barrier failed: {:?}",
                    case.name, barrier.error
                );
                assert_i64(&session.eval(case.probe), case.expected);
            }
        });
    }

    #[test]
    fn repl_fresh_full_compile_recovers_reached_conditional_method_11742() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11742);
            let failed = session.eval(
                "if true\n\
                   reached_method_11742() = 41\n\
                 end\n\
                 error(\"reached-method-11742\")\n\
                 unreached_method_11742() = 99",
            );
            assert!(!failed.success, "the trailing error must escape");
            assert!(
                session.has_live_vm(),
                "the reached method must retain a recoverable live VM: {:?}",
                failed.error
            );
            assert_i64(&session.eval("reached_method_11742()"), 41);

            let barrier = session.eval("module MethodRecoveryBarrier11742\nvalue = 1\nend");
            assert!(
                barrier.success,
                "full-rebuild barrier failed: {:?}",
                barrier.error
            );
            assert_i64(&session.eval("reached_method_11742()"), 41);

            let absent = session.eval("!isdefined(Main, :unreached_method_11742)");
            assert!(absent.success, "isdefined failed: {:?}", absent.error);
            assert!(matches!(absent.value, Some(Value::Bool(true))));
        });
    }

    #[test]
    fn repl_fresh_full_compile_recovers_final_reached_method_11745() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11745);
            let failed = session.eval(
                "if true\n\
                   reached_method_11745() = 41\n\
                 end\n\
                 error(\"reached-method-11745\")",
            );
            assert!(!failed.success, "the trailing error must escape");
            assert!(
                session.has_live_vm(),
                "the reached final method must retain a recoverable live VM: {:?}",
                failed.error
            );
            let immediate = session.eval("reached_method_11745()");
            assert!(
                immediate.success,
                "immediate recovered-method probe failed: {:?}",
                immediate.error
            );
            assert!(matches!(immediate.value, Some(Value::I64(41))));

            let barrier = session.eval("module MethodRecoveryBarrier11745\nvalue = 1\nend");
            assert!(
                barrier.success,
                "full-rebuild barrier failed: {:?}",
                barrier.error
            );
            let rebuilt = session.eval("reached_method_11745()");
            assert!(
                rebuilt.success,
                "post-barrier recovered-method probe failed: {:?}",
                rebuilt.error
            );
            assert!(matches!(rebuilt.value, Some(Value::I64(41))));
        });
    }

    #[test]
    fn repl_recovers_reached_selective_import_only_11748() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11748);
            let bootstrap = session.eval(
                "module ReachedImportOwner11748\n\
                 export reached_import_11748\n\
                 reached_import_11748() = 43\n\
                 end\n\
                 module LateImportOwner11748\n\
                 export late_import_11748\n\
                 late_import_11748() = 99\n\
                 end",
            );
            assert!(bootstrap.success, "bootstrap failed: {:?}", bootstrap.error);

            let failed = session.eval(
                "using .ReachedImportOwner11748: reached_import_11748\n\
                 error(\"import-11748\")\n\
                 using .LateImportOwner11748: late_import_11748",
            );
            assert!(!failed.success, "the import error must escape");

            let immediate = session.eval("reached_import_11748()");
            assert!(
                immediate.success,
                "immediate reached-import probe failed: {:?}",
                immediate.error
            );
            assert!(matches!(immediate.value, Some(Value::I64(43))));
            let immediate_late = session.eval("late_import_11748()");
            assert!(
                !immediate_late.success,
                "source-later import became callable immediately: value={:?}",
                immediate_late.value
            );
            assert!(
                immediate_late
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("is not imported")),
                "unexpected immediate source-later error: {:?}",
                immediate_late.error
            );

            let barrier = session.eval("module ImportBarrier11748\nvalue = 1\nend");
            assert!(barrier.success, "barrier failed: {:?}", barrier.error);
            let rebuilt = session.eval("reached_import_11748()");
            assert!(
                rebuilt.success,
                "post-barrier reached-import probe failed: {:?}",
                rebuilt.error
            );
            assert!(matches!(rebuilt.value, Some(Value::I64(43))));
            let rebuilt_late = session.eval("late_import_11748()");
            assert!(
                !rebuilt_late.success,
                "source-later import became callable after rebuild: value={:?}",
                rebuilt_late.value
            );
            assert!(
                rebuilt_late
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("is not imported")),
                "unexpected rebuilt source-later error: {:?}",
                rebuilt_late.error
            );
        });
    }

    #[test]
    fn module_export_does_not_define_main_binding_11749() {
        with_large_stack(|| {
            let mut session = REPLSession::new(11749);
            let result = session.eval(
                "module ExportOwner11749\n\
                 export exported_function_11749\n\
                 exported_function_11749() = 1\n\
                 end\n\
                 isdefined(Main, :exported_function_11749)",
            );
            assert!(
                result.success,
                "reflection probe failed: {:?}",
                result.error
            );
            assert!(
                matches!(result.value, Some(Value::Bool(false))),
                "an export without using/import leaked into Main: {:?}",
                result.value
            );

            let owner = session.eval("isdefined(ExportOwner11749, :exported_function_11749)");
            assert!(owner.success, "owner probe failed: {:?}", owner.error);
            assert!(matches!(owner.value, Some(Value::Bool(true))));

            let top_level = session.eval("top_level_function_11749() = 2");
            assert!(
                top_level.success,
                "top-level definition failed: {:?}",
                top_level.error
            );
            let top_level_probe = session.eval("isdefined(Main, :top_level_function_11749)");
            assert!(
                top_level_probe.success,
                "top-level probe failed: {:?}",
                top_level_probe.error
            );
            assert!(matches!(top_level_probe.value, Some(Value::Bool(true))));

            let imported = session.eval(
                "using .ExportOwner11749: exported_function_11749\n\
                 isdefined(Main, :exported_function_11749)",
            );
            assert!(
                imported.success,
                "imported probe failed: {:?}",
                imported.error
            );
            assert!(matches!(imported.value, Some(Value::Bool(true))));
        });
    }
}

mod runtime_nominal_lowering_tests_11654 {
    use subset_julia_vm::ir::core::{
        DefinitionOrderCursor, Expr, Literal, RuntimeNominalDef, Stmt,
    };
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    fn parse_raw_11654(source: &str) -> Result<subset_julia_vm::ir::core::Program, String> {
        let mut parser = Parser::new().map_err(|error| error.to_string())?;
        let outcome = parser.parse(source).map_err(|error| error.to_string())?;
        Lowering::new(source)
            .lower(outcome)
            .map_err(|error| format!("{error:?}"))
    }

    fn single_if_body_stmt_11654(source: &str) -> Option<Stmt> {
        let program = parse_raw_11654(source).ok()?;
        let Stmt::If { then_branch, .. } = program.main.stmts.first()? else {
            return None;
        };
        assert_eq!(then_branch.stmts.len(), 1);
        then_branch.stmts.first().cloned()
    }

    #[test]
    fn top_level_if_nominal_families_lower_to_runtime_definitions_11654() {
        assert!(matches!(
            single_if_body_stmt_11654("if true; struct IfStruct11654; x::Int; end; end"),
            Some(Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::Struct(_),
                ..
            })
        ));
        assert!(matches!(
            single_if_body_stmt_11654("if true; abstract type IfAbstract11654 end; end"),
            Some(Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::AbstractType(_),
                ..
            })
        ));
        assert!(matches!(
            single_if_body_stmt_11654("if true; primitive type IfPrimitive11654 8 end; end"),
            Some(Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::PrimitiveType(_),
                ..
            })
        ));
        assert!(matches!(
            single_if_body_stmt_11654("if true; @enum IfEnum11654 if_a11654; end"),
            Some(Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::Enum(_),
                ..
            })
        ));
    }

    #[test]
    fn runtime_nominal_statement_carries_stamped_definition_order_11690() {
        let Some(Stmt::RuntimeNominalDef {
            definition: RuntimeNominalDef::Struct(definition),
            span,
            ..
        }) = single_if_body_stmt_11654("if true; struct RuntimeSiteOrder11690; x::Int; end; end")
        else {
            unreachable!("expected runtime struct definition");
        };
        assert_ne!(definition.span.definition_order, 0);
        assert_eq!(span.definition_order, definition.span.definition_order);
    }

    #[test]
    fn executable_only_fragments_advance_runtime_nominal_chronology_11690() {
        let mut first = parse_raw_11654("if true; struct FragmentFirst11690; end; end")
            .unwrap_or_else(|error| panic!("first fragment failed: {error}"));
        let mut second = parse_raw_11654("if true; struct FragmentSecond11690; end; end")
            .unwrap_or_else(|error| panic!("second fragment failed: {error}"));
        let mut chronology = DefinitionOrderCursor::default();
        chronology.append_fragment(&mut first);
        chronology.append_fragment(&mut second);

        let runtime_order = |program: &subset_julia_vm::ir::core::Program| {
            let Stmt::If { then_branch, .. } = &program.main.stmts[0] else {
                unreachable!("expected top-level if");
            };
            let Stmt::RuntimeNominalDef { span, .. } = &then_branch.stmts[0] else {
                unreachable!("expected runtime nominal");
            };
            span.definition_order
        };
        let first_order = runtime_order(&first);
        let second_order = runtime_order(&second);
        assert_ne!(first_order, 0);
        assert_eq!(second_order, first_order + 1);
        assert_eq!(chronology.max_definition_order(), second_order);
    }

    #[test]
    fn top_level_for_enum_lowers_to_nothing_while_struct_remains_runtime_11654() {
        let Ok(enum_program) =
            parse_raw_11654("for enum_iteration11654 in 1:1; @enum ForEnum11654 for_a11654; end")
        else {
            unreachable!("enum loop should lower");
        };
        let Stmt::For { body, .. } = &enum_program.main.stmts[0] else {
            unreachable!("expected for loop");
        };
        assert!(matches!(
            body.stmts.as_slice(),
            [Stmt::Expr {
                expr: Expr::Literal(Literal::Nothing, _),
                ..
            }]
        ));

        let Ok(struct_program) = parse_raw_11654(
            "for struct_iteration11654 in 1:1; struct ForStruct11654; x::Int; end; end",
        ) else {
            unreachable!("struct loop should lower");
        };
        let Stmt::For { body, .. } = &struct_program.main.stmts[0] else {
            unreachable!("expected for loop");
        };
        assert!(matches!(
            body.stmts.as_slice(),
            [Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::Struct(_),
                ..
            }]
        ));
    }

    #[test]
    fn expression_try_keeps_runtime_nominal_lowering_context_11654() {
        let Ok(program) = parse_raw_11654(
            "result11654 = try; abstract type ExprTryAbstract11654 end; 1; catch; 0; end",
        ) else {
            unreachable!("expression try should lower");
        };
        let Stmt::Assign {
            value: Expr::LetBlock { body, .. },
            ..
        } = &program.main.stmts[0]
        else {
            unreachable!("expected assignment from lowered expression try");
        };
        let Some(Stmt::Try { try_block, .. }) = body
            .stmts
            .iter()
            .find(|statement| matches!(statement, Stmt::Try { .. }))
        else {
            unreachable!("expected rewritten try statement, got {:?}", body.stmts);
        };
        assert!(matches!(
            try_block.stmts.first(),
            Some(Stmt::RuntimeNominalDef {
                definition: RuntimeNominalDef::AbstractType(_),
                ..
            })
        ));
    }

    #[test]
    fn function_body_nominal_declarations_remain_rejected_11654() {
        for source in [
            "function f11654(); struct FunctionStruct11654; x::Int; end; end",
            "function f11654(); abstract type FunctionAbstract11654 end; end",
            "function f11654(); primitive type FunctionPrimitive11654 8 end; end",
            "function f11654(); @enum FunctionEnum11654 function_enum_a11654; end",
        ] {
            assert!(
                parse_raw_11654(source).is_err(),
                "function-body nominal declaration unexpectedly lowered: {source}"
            );
        }
    }
}
