//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod builtin_type_registry_10954_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::Instr;

    fn compile_source(source: &str) -> subset_julia_vm_bytecode::CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    #[test]
    fn builtin_type_expr_var_emits_registry_projection_issue_10954() {
        let compiled = compile_source(
            "builtin_types_10954() = \
             (Int, UInt, ComplexF64, SubString, Vector, MemoryRef)\n\
             builtin_types_10954()",
        );
        let function = compiled
            .functions
            .iter()
            .find(|function| function.name == "builtin_types_10954")
            .expect("builtin_types_10954 function");
        let body = &compiled.code[function.code_start..function.code_end];
        let pushed: Vec<&str> = body
            .iter()
            .filter_map(|instr| match instr {
                Instr::PushDataType(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();

        for expected in [
            "Int64",
            "UInt64",
            "Complex{Float64}",
            "SubString",
            "Vector",
            "MemoryRef",
        ] {
            assert!(
                pushed.contains(&expected),
                "bare builtin {expected} must emit its canonical registry projection: {body:?}"
            );
        }
    }
}

mod lowering_helper_reflection_provenance_11685_tests {
    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    #[test]
    fn source_function_wins_inference_name_collision_with_private_helper_11685() {
        let result = compile_and_run_value(
            r#"
__lambda_0(x) = x + 1
saved_collision_11685 = identity(x -> 1.5)
call_user_collision_11685(x) = __lambda_0(x)
(
    Base.infer_return_type(call_user_collision_11685, Tuple{Int64}) === Int64,
    Base.infer_return_type(saved_collision_11685, Tuple{Int64}) === Float64,
)
"#,
            0,
        )
        .unwrap_or_else(|error| panic!("collision program failed: {error}"));

        assert!(matches!(
            result,
            Value::Tuple(tuple)
                if matches!(tuple.elements.as_slice(), [Value::Bool(true), Value::Bool(true)])
        ));
    }

    #[test]
    fn generator_reflection_selects_encoded_private_helper_identity_11685() {
        let result = compile_and_run_value(
            r#"
__gen_body_50(x)="abc"
saved_generator_11685=(x+1 for x in 1:3)
(
    Base.infer_return_type(collect, Tuple{typeof(saved_generator_11685)}) === Vector{Int64},
    collect(saved_generator_11685) == [2, 3, 4],
)
"#,
            0,
        )
        .unwrap_or_else(|error| panic!("generator collision program failed: {error}"));

        assert!(matches!(
            result,
            Value::Tuple(tuple)
                if matches!(tuple.elements.as_slice(), [Value::Bool(true), Value::Bool(true)])
        ));
    }
}

mod call_resolution_10461_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::Instr;

    fn compile_source(source: &str) -> subset_julia_vm_bytecode::CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    #[test]
    fn call_dynamic_carries_resolved_callee_identity_10461() {
        let source = r#"
            direct_pick_10461(::Int64, ::Bool) = :int
            direct_pick_10461(::String, ::Bool) = :string
            direct_apply_10461(x, flag) = direct_pick_10461(x, flag)

            module IdentityModule10461
                pick(::Int64, ::Bool) = :int
                pick(::String, ::Bool) = :string
            end
            # Single-argument qualified Any dispatch is tracked separately in #11622.
            qualified_apply_10461(x, flag) = IdentityModule10461.pick(x, flag)
        "#;
        let compiled = compile_source(source);

        for (caller, expected_callee) in [
            ("direct_apply_10461", "direct_pick_10461"),
            ("qualified_apply_10461", "IdentityModule10461.pick"),
        ] {
            let function = compiled
                .functions
                .iter()
                .find(|function| function.name == caller)
                .unwrap_or_else(|| panic!("missing function {caller}"));
            let body = &compiled.code[function.code_start..function.code_end];
            let dynamic = body.iter().find_map(|instruction| {
                let Instr::CallDynamic(operands) = instruction else {
                    return None;
                };
                (operands.arg_count == 2).then_some(operands.as_ref())
            });
            let dynamic = dynamic.unwrap_or_else(|| {
                panic!("{caller} must retain runtime dispatch bytecode: {body:?}")
            });
            assert_eq!(dynamic.callee_name, expected_callee, "{caller}: {body:?}");
            assert_eq!(dynamic.candidates.len(), 2, "{caller}: {body:?}");
        }
    }

    #[test]
    fn invoke_function_variable_emits_all_declared_signature_lanes_11619() {
        #[derive(Clone, Copy)]
        enum InvokeLane {
            StaticPositional,
            StaticKeyword,
            DynamicPositional,
            DynamicKeyword,
        }

        let source = r#"
            invoke_pick_11619(::Any; offset = 0) = :any
            invoke_pick_11619(::Integer; offset = 0) = :integer
            invoke_pick_11619(::Int; offset = 0) = :int
            invoke_static_pos_11619(fn, x) = invoke(fn, Tuple{Any}, x)
            invoke_static_kw_11619(fn, x) = invoke(fn, Tuple{Any}, x; offset = 7)
            invoke_dynamic_pos_11619(fn, sig, x) = invoke(fn, sig, x)
            invoke_dynamic_kw_11619(fn, sig, x) = invoke(fn, sig, x; offset = 7)
        "#;
        let compiled = compile_source(source);

        for (caller, lane) in [
            ("invoke_static_pos_11619", InvokeLane::StaticPositional),
            ("invoke_static_kw_11619", InvokeLane::StaticKeyword),
            ("invoke_dynamic_pos_11619", InvokeLane::DynamicPositional),
            ("invoke_dynamic_kw_11619", InvokeLane::DynamicKeyword),
        ] {
            let function = compiled
                .functions
                .iter()
                .find(|function| function.name == caller)
                .unwrap_or_else(|| panic!("missing function {caller}"));
            let body = &compiled.code[function.code_start..function.code_end];
            let emitted = match lane {
                InvokeLane::StaticPositional => body.iter().any(|instruction| {
                    matches!(instruction, Instr::InvokeFunctionVariable(1, declared) if declared == &["Any"])
                }),
                InvokeLane::StaticKeyword => body.iter().any(|instruction| {
                    matches!(instruction, Instr::InvokeFunctionVariableWithKwargs(operands)
                        if operands.arg_count == 1 && operands.declared_signature == ["Any"])
                }),
                InvokeLane::DynamicPositional => body.iter().any(
                    |instruction| matches!(instruction, Instr::InvokeFunctionVariableDynamicSignature(1)),
                ),
                InvokeLane::DynamicKeyword => body.iter().any(|instruction| {
                    matches!(instruction, Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(1, _, _))
                }),
            };
            assert!(emitted, "{caller} emitted the wrong invoke lane: {body:?}");
        }
    }
}

mod annotation_inference_9121_tests {
    //! Bytecode-level regression guards for Issues #9121 and #9132.
    //!
    //! `x::T = rhs` lowers to `x = convert(T, rhs)` (lowering/stmt/assignment.rs).
    //! Before the fix, the compile-time type oracle (`infer_expr_type`) had no case
    //! for `convert` calls and inferred them as `Any`, so a function WITH a type
    //! annotation compiled `x * 2.0` to `CallDynamicBinaryBoth` (22-candidate
    //! runtime dispatch) while the identical function WITHOUT the annotation
    //! compiled a single `MulF64` — more type information produced worse code.
    //! The same lowering behind `const B::Float64 = 2.0` made the global pre-scan
    //! store `Any`, so every use of the typed const compiled to `LoadAny` +
    //! dynamic dispatch (Issue #9132).
    //!
    //! These tests pin the fixed codegen: annotated locals and typed const globals
    //! compile to the same typed instructions as their un-annotated equivalents.

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

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

    fn has_dynamic_binary_dispatch(body: &[Instr]) -> bool {
        body.iter().any(|instr| {
            matches!(
                instr,
                Instr::CallDynamicBinary(_, _, _) | Instr::CallDynamicBinaryBoth(_, _)
            )
        })
    }

    const ANNOTATED_LOCAL_SRC: &str = r#"
    function get_x()::Float64
        return 3.0
    end

    function test_no_annot()
        x = get_x()
        return x * 2.0
    end

    function test_with_annot()
        x::Float64 = get_x()
        return x * 2.0
    end

    println(test_no_annot())
    println(test_with_annot())
    "#;

    /// Issue #9121: the annotated function must compile the multiply to `MulF64`,
    /// exactly like the un-annotated function — no runtime binary dispatch.
    #[test]
    fn annotated_local_compiles_static_mul_9121() {
        let compiled = compile_source_with_base(ANNOTATED_LOCAL_SRC);

        let no_annot = get_function(&compiled, "test_no_annot");
        let no_annot_body = function_body(&compiled, no_annot);
        assert!(
            no_annot_body.iter().any(|i| matches!(i, Instr::MulF64)),
            "un-annotated baseline must use MulF64, got: {:?}",
            no_annot_body
        );

        let with_annot = get_function(&compiled, "test_with_annot");
        let with_annot_body = function_body(&compiled, with_annot);
        assert!(
            with_annot_body.iter().any(|i| matches!(i, Instr::MulF64)),
            "annotated function must use MulF64 (Issue #9121), got: {:?}",
            with_annot_body
        );
        assert!(
            !has_dynamic_binary_dispatch(with_annot_body),
            "annotated function must not fall back to runtime binary dispatch \
             (Issue #9121), got: {:?}",
            with_annot_body
        );
    }

    const TYPED_CONST_SRC: &str = r#"
    const A = 2.0
    const B::Float64 = 2.0

    function f_untyped(n)
        s = 0.0
        for i in 1:n
            s += i * A
        end
        return s
    end

    function f_typed(n)
        s = 0.0
        for i in 1:n
            s += i * B
        end
        return s
    end

    println(f_untyped(10))
    println(f_typed(10))
    "#;

    /// Issue #9132: a typed const global (`const B::Float64 = 2.0`) must keep its
    /// declared type in the global pre-scan, so uses load through `LoadF64` and
    /// arithmetic stays on the typed `MulF64`/`AddF64` path — identical to an
    /// un-annotated const.
    #[test]
    fn typed_const_global_keeps_declared_type_9132() {
        let compiled = compile_source_with_base(TYPED_CONST_SRC);

        let typed = get_function(&compiled, "f_typed");
        let typed_body = function_body(&compiled, typed);
        assert!(
            typed_body
                .iter()
                .any(|i| matches!(i, Instr::LoadF64(name) if name == "B")),
            "typed const must load via LoadF64 (Issue #9132), got: {:?}",
            typed_body
        );
        assert!(
            typed_body.iter().any(|i| matches!(i, Instr::MulF64)),
            "typed-const loop must multiply via MulF64 (Issue #9132), got: {:?}",
            typed_body
        );
        assert!(
            !typed_body
                .iter()
                .any(|i| matches!(i, Instr::LoadAny(name) if name == "B")),
            "typed const must not degrade to LoadAny (Issue #9132), got: {:?}",
            typed_body
        );
        assert!(
            !has_dynamic_binary_dispatch(typed_body),
            "typed-const loop must not use runtime binary dispatch (Issue #9132), \
             got: {:?}",
            typed_body
        );
    }

    const TYPED_ARRAY_PARAM_SRC: &str = r#"
    function sum_f64(a::Vector{Float64})
        s = 0.0
        for i in 1:length(a)
            s = s + a[i]
        end
        return s
    end

    a = [1.0, 2.0, 3.0, 4.0, 5.0]
    println(sum_f64(a))
    "#;

    /// Issue #9133: a `Vector{Float64}` parameter annotation must propagate its
    /// element type — the loop accumulator stays a typed F64 slot (`AddF64`, no
    /// `CallDynamicBinaryBoth`) and `length(a)` compiles to the `Length` builtin
    /// (no resolved Pure Julia call + `DynamicToI64`).
    #[test]
    fn typed_array_param_propagates_element_type_9133() {
        let compiled = compile_source_with_base(TYPED_ARRAY_PARAM_SRC);

        let f = get_function(&compiled, "sum_f64");
        let body = function_body(&compiled, f);
        assert!(
            body.iter().any(|i| matches!(i, Instr::AddF64)),
            "typed-array-param loop must accumulate via AddF64 (Issue #9133), got: {:?}",
            body
        );
        assert!(
            !has_dynamic_binary_dispatch(body),
            "typed-array-param loop must not use runtime binary dispatch \
             (Issue #9133), got: {:?}",
            body
        );
        assert!(
            body.iter().any(|i| matches!(
                i,
                Instr::CallBuiltin(subset_julia_vm::builtins::BuiltinId::Length, 1)
            )),
            "length(a::Vector{{Float64}}) must compile to the Length builtin \
             (Issue #9133), got: {:?}",
            body
        );
        assert!(
            !body.iter().any(|i| matches!(i, Instr::DynamicToI64)),
            "length result must be I64 directly, no DynamicToI64 (Issue #9133), \
             got: {:?}",
            body
        );
    }
}

mod bare_abstract_numeric_param_5076_tests {
    //! Regression tests for Issue #5076.
    //!
    //! A method annotated with a *bare* abstract numeric type (`x::Real`,
    //! `x::Number`, `x::Integer`, `x::Signed`, ...) must preserve the concrete
    //! argument type when its body calls a type-generic function (`zero`, `one`,
    //! `oneunit`). Before the fix, `type_helpers::julia_type_to_value_type` widened
    //! `Real`/`Number` params to `ValueType::F64` (and `Integer` to `ValueType::I64`)
    //! in the compiler's `locals`, so `infer_julia_type` reported `Float64`/`Int64`
    //! and statically bound `zero(x)` to `zero(::Float64)`. That made
    //! `f(x::Real)=zero(x); f(3)` error ("expected I64, got Float64") and
    //! `f(Int8(3))` return `0.0::Float64`.
    //!
    //! The fix makes `infer_julia_type` report `Any` for params already tracked in
    //! `abstract_numeric_params` (which already load via `LoadAny`), so type-generic
    //! calls dispatch on the concrete runtime value — exactly like the untyped
    //! `f(x)=zero(x)` and `where {T<:Real}` forms, and matching upstream Julia.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    fn run(src: &str) -> Value {
        compile_and_run_value(src, 0).unwrap_or_else(|e| panic!("run failed for {src:?}: {e}"))
    }

    #[test]
    fn bare_real_zero_preserves_int64() {
        // Previously errored "expected I64, got Float64".
        assert!(matches!(run("f(x::Real) = zero(x)\nf(3)"), Value::I64(0)));
    }

    #[test]
    fn bare_real_zero_preserves_int8() {
        // Previously returned 0.0::Float64.
        assert!(matches!(
            run("f(x::Real) = zero(x)\nf(Int8(3))"),
            Value::I8(0)
        ));
    }

    #[test]
    fn bare_real_zero_preserves_int32() {
        assert!(matches!(
            run("f(x::Real) = zero(x)\nf(Int32(7))"),
            Value::I32(0)
        ));
    }

    #[test]
    fn bare_real_zero_preserves_float64() {
        match run("f(x::Real) = zero(x)\nf(3.0)") {
            Value::F64(v) => assert_eq!(v, 0.0),
            other => panic!("expected F64(0.0), got {other:?}"),
        }
    }

    #[test]
    fn bare_number_zero_preserves_int8() {
        assert!(matches!(
            run("f(x::Number) = zero(x)\nf(Int8(3))"),
            Value::I8(0)
        ));
    }

    #[test]
    fn bare_integer_zero_preserves_int8() {
        assert!(matches!(
            run("f(x::Integer) = zero(x)\nf(Int8(3))"),
            Value::I8(0)
        ));
    }

    #[test]
    fn bare_signed_zero_preserves_int32() {
        assert!(matches!(
            run("f(x::Signed) = zero(x)\nf(Int32(7))"),
            Value::I32(0)
        ));
    }

    #[test]
    fn bare_real_one_preserves_int8() {
        assert!(matches!(
            run("f(x::Real) = one(x)\nf(Int8(3))"),
            Value::I8(1)
        ));
    }

    #[test]
    fn bare_number_one_preserves_int64() {
        assert!(matches!(run("f(x::Number) = one(x)\nf(3)"), Value::I64(1)));
    }

    #[test]
    fn bare_real_oneunit_preserves_int8() {
        assert!(matches!(
            run("f(x::Real) = oneunit(x)\nf(Int8(3))"),
            Value::I8(1)
        ));
    }

    #[test]
    fn bare_abstract_matches_untyped_form_int() {
        // Both the bare-abstract and untyped forms must agree (and equal Int64 0).
        let bare = run("f(x::Real) = zero(x)\nf(3)");
        let untyped = run("f(x) = zero(x)\nf(3)");
        assert!(matches!(bare, Value::I64(0)));
        assert!(matches!(untyped, Value::I64(0)));
    }
}

mod binary_both_dispatch_cache_8168_tests {
    //! Issue #8168: per-call-site cache for the `CallDynamicBinaryBoth` resolver.
    //!
    //! When two `Any`-typed struct operands flow into a binary operator (here `+`
    //! on a `Vector{Any}` of `V2`), the operator compiles to
    //! `CallDynamicBinaryBoth` and the VM must pick the matching method from the
    //! operator's full candidate list on every call. The dispatch decision is fully
    //! determined by the operand type names for struct/struct pairs, so it is
    //! memoized per call site. These tests pin both the correctness of the cached
    //! decision and that the fast path is actually taken.

    #[cfg(feature = "profiling")]
    use std::collections::HashMap;
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::profiler;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, Value};

    const ANY_STRUCT_ADD_SOURCE: &str = r#"
    struct V2
        x::Float64
        y::Float64
    end
    import Base: +
    +(a::V2, b::V2) = V2(a.x + b.x, a.y + b.y)

    function sumany(xs, n)
        acc = xs[1]
        for _ in 1:n
            for k in 1:length(xs)
                acc = acc + xs[k]
            end
        end
        acc.x + acc.y
    end

    xs = Any[V2(1.0, 2.0), V2(3.0, 4.0)]
    sumany(xs, 3)
    "#;

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn run_any_struct_add() -> Value {
        let mut vm = Vm::new_program(compile_source(ANY_STRUCT_ADD_SOURCE), StableRng::new(0));
        vm.run().expect("run sumany")
    }

    /// The cached dynamic dispatch must select the user `+(::V2, ::V2)` method on
    /// every iteration, matching upstream Julia's result (33.0). Guards against the
    /// cache returning a stale / wrong method index. (Issue #8168)
    #[test]
    fn any_struct_add_cached_dispatch_matches_upstream_8168() {
        match run_any_struct_add() {
            Value::F64(value) => assert!(
                (value - 33.0).abs() < 1.0e-12,
                "unexpected sumany result: {value}"
            ),
            other => panic!("expected Float64 result, got {other:?}"),
        }
    }

    /// The repeated struct/struct dispatch must take the per-call-site resolver
    /// cache after the first resolution. (Issue #8168)
    #[cfg(feature = "profiling")]
    #[test]
    fn any_struct_add_takes_binary_both_resolver_cache_8168() {
        profiler::clear();
        profiler::enable();
        let result = run_any_struct_add();
        profiler::disable();

        match result {
            Value::F64(value) => assert!((value - 33.0).abs() < 1.0e-12),
            other => panic!("expected Float64 result, got {other:?}"),
        }

        let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
        assert!(
            counts
                .get("BinaryBothResolverCacheHit")
                .copied()
                .unwrap_or(0)
                > 0,
            "repeated struct/struct + should hit the binary-both resolver cache: {counts:?}"
        );
    }
}

mod dispatch_inline_cache_8561_tests {
    //! Issue #8561 per-call-site dynamic dispatch inline cache tests.
    //!
    //! End-to-end coverage for the acceptance criteria:
    //!
    //! - a monomorphic dynamic call site in a hot loop hits the call-site inline
    //!   cache (asserted through the opt-in #8559 `StackVmMetrics` counters);
    //! - a two-type (mixed `Int64`/`Float64` array) site stays cached thanks to
    //!   the two-way slot;
    //! - redefining a method after cache warm-up produces the *new* behavior —
    //!   the bug class the generation-based invalidation exists for.
    //!
    //! Every fixture source was verified against upstream Julia
    //! (`julia --startup-file=no`); expected outputs are pinned from it.

    use std::sync::Mutex;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::{set_stack_vm_metrics_forced, StackVmMetrics, Vm};

    /// Serializes process-global state (the forced metrics gate) across tests:
    /// nextest runs one process per test, but plain `cargo test` shares them.
    static GLOBAL_STATE_LOCK: Mutex<()> = Mutex::new(());

    /// Typed dynamic dispatch (`CallTypedDispatch("g", ...)`, verified via
    /// bytecode inspection) in a hot loop over a homogeneous `Any[]` array:
    /// after the first resolution the site is monomorphic. Upstream Julia
    /// prints 1000.
    const MONO_SRC: &str = r#"
    g(x::Int64) = 1
    g(x::Float64) = 2
    function warm(xs, n)
        s = 0
        k = 0
        while k < n
            for x in xs
                s += g(x)
            end
            k += 1
        end
        s
    end
    xs = Any[1, 2, 3, 4]
    println(warm(xs, 250))
    "#;
    const MONO_EXPECTED: &str = "1000\n";
    /// `warm(xs, 250)` executes the `g(x)` site 1000 times.
    const MONO_CALLS: u64 = 1000;

    /// The same loop over a mixed `Int64`/`Float64` array: the site alternates
    /// between two exact scalar identities, which the two-way slot must keep
    /// cached simultaneously. Upstream Julia prints 1500.
    const MIXED_SRC: &str = r#"
    g(x::Int64) = 1
    g(x::Float64) = 2
    function warm(xs, n)
        s = 0
        k = 0
        while k < n
            for x in xs
                s += g(x)
            end
            k += 1
        end
        s
    end
    xs = Any[1, 2.0, 3, 4.0]
    println(warm(xs, 250))
    "#;
    const MIXED_EXPECTED: &str = "1500\n";

    /// Method redefinition after warm-up: the second `warm(xs)` run must observe
    /// the redefined `g(::Int)`. Upstream Julia prints 4 then 202.
    const REDEFINITION_SRC: &str = r#"
    g(x::Int) = 1
    g(x::Float64) = 2
    function warm(xs)
        s = 0
        for x in xs
            s += g(x)
        end
        s
    end
    xs = Any[1, 2.0, 1]
    println(warm(xs))
    @eval g(x::Int) = 100
    println(warm(xs))
    "#;
    const REDEFINITION_EXPECTED: &str = "4\n202\n";

    fn run_with_metrics(src: &str) -> (String, StackVmMetrics) {
        let program = parse_and_lower_with_base_dir(src, None)
            .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
        let compiled =
            compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
        set_stack_vm_metrics_forced(true);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        set_stack_vm_metrics_forced(false);
        vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
        let metrics = vm
            .stack_vm_metrics()
            .expect("metrics were forced on for this Vm");
        (vm.get_output().to_string(), metrics)
    }

    /// A monomorphic typed dynamic dispatch site in a hot loop must be served
    /// from the call-site inline cache: at least one hit per post-warm-up call
    /// of the `g(x)` site, and only a handful of resolver runs (first-fill
    /// misses) across the whole program.
    #[test]
    fn monomorphic_dynamic_call_site_hits_inline_cache_issue_8561() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (output, metrics) = run_with_metrics(MONO_SRC);
        assert_eq!(output, MONO_EXPECTED, "upstream-Julia-pinned output");

        assert!(
            metrics.dispatch_inline_cache_hits >= MONO_CALLS - 1,
            "the 1000-execution monomorphic g(x) site must be cache-served \
             after its first fill; hits={} misses={}",
            metrics.dispatch_inline_cache_hits,
            metrics.dispatch_inline_cache_misses,
        );
        assert!(
            metrics.dispatch_inline_cache_misses <= 32,
            "cache-eligible resolver runs must stay bounded by first-fills, \
             not scale with loop iterations; hits={} misses={}",
            metrics.dispatch_inline_cache_hits,
            metrics.dispatch_inline_cache_misses,
        );
    }

    /// The counters are deterministic: two identical runs record identical
    /// hit/miss counts (the evidence protocol for a machine that runs other
    /// builds concurrently — counters, not wall time).
    #[test]
    fn inline_cache_counters_are_deterministic_issue_8561() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (output_a, metrics_a) = run_with_metrics(MONO_SRC);
        let (output_b, metrics_b) = run_with_metrics(MONO_SRC);
        assert_eq!(output_a, output_b);
        assert_eq!(
            metrics_a.dispatch_inline_cache_hits,
            metrics_b.dispatch_inline_cache_hits
        );
        assert_eq!(
            metrics_a.dispatch_inline_cache_misses,
            metrics_b.dispatch_inline_cache_misses
        );
    }

    /// A two-identity site (mixed Int64/Float64 array) must stay cached through
    /// the two-way slot instead of thrashing a single way.
    #[test]
    fn mixed_two_type_call_site_hits_inline_cache_issue_8561() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (output, metrics) = run_with_metrics(MIXED_SRC);
        assert_eq!(output, MIXED_EXPECTED, "upstream-Julia-pinned output");

        assert!(
            metrics.dispatch_inline_cache_hits >= MONO_CALLS - 2,
            "both scalar identities of the alternating g(x) site must stay \
             cached (two-way slot); hits={} misses={}",
            metrics.dispatch_inline_cache_hits,
            metrics.dispatch_inline_cache_misses,
        );
        assert!(
            metrics.dispatch_inline_cache_misses <= 32,
            "an Int64/Float64-alternating site must not thrash; hits={} misses={}",
            metrics.dispatch_inline_cache_hits,
            metrics.dispatch_inline_cache_misses,
        );
    }

    /// Redefining a method after cache warm-up must produce the NEW behavior
    /// (Issue #8561 acceptance criterion — upstream Julia prints 4 then 202).
    #[test]
    fn method_redefinition_after_warmup_observes_new_method_issue_8561() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (output, _metrics) = run_with_metrics(REDEFINITION_SRC);
        assert_eq!(
            output, REDEFINITION_EXPECTED,
            "upstream-Julia-pinned output"
        );
    }
}

mod i64_resolved_call_6314_tests {
    #[cfg(feature = "profiling")]
    use std::collections::HashMap;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::profiler;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value};

    const NESTED_RESOLVED_I64_CALL_SOURCE: &str = r#"
    function score6314(x::Int64)
        y = x * x
        return y + 1
    end

    function sum_score6314(n::Int64)
        total = 0
        for i in 1:n
            total += score6314(i)
        end
        return total
    end

    sum_score6314(20)
    "#;

    const DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE: &str = r#"
    function score6315(x::Int64, y::Int64)
        z = x + y
        return z * y
    end

    function sum_score6315(n::Int64)
        total = 0
        step = 2
        for i in 1:n
            total += score6315(i, step)
        end
        return total
    end

    sum_score6315(20)
    "#;

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a [Instr] {
        let func = get_function(compiled, name);
        &compiled.code[func.entry..func.code_end]
    }

    #[test]
    fn nested_resolved_i64_helper_preserves_result_6314() {
        let compiled = compile_source(NESTED_RESOLVED_I64_CALL_SOURCE);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run nested resolved I64 helper");

        match result {
            Value::I64(value) => assert_eq!(value, 2890),
            other => panic!("expected Int64 sum_score6314 result, got {other:?}"),
        }
    }

    #[test]
    fn direct_slot_resolved_i64_helper_preserves_result_6315() {
        let compiled = compile_source(DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run direct slot resolved I64 helper");

        match result {
            Value::I64(value) => assert_eq!(value, 500),
            other => panic!("expected Int64 sum_score6315 result, got {other:?}"),
        }
    }

    #[test]
    fn direct_slot_resolved_i64_helper_uses_slot_call_6315() {
        let compiled = compile_source(DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE);
        let body = function_body(&compiled, "sum_score6315");
        let slot_calls = body
            .iter()
            .filter(|instr| match instr {
                Instr::CallResolvedI64Slots(operands) if operands.slots.len() == 2 => compiled
                    .functions
                    .get(operands.func_index)
                    .map(|func| func.name == "score6315")
                    .unwrap_or(false),
                _ => false,
            })
            .count();

        assert!(
            slot_calls > 0,
            "resolved non-gcd I64 helper should read call arguments from slots: {body:?}"
        );
        assert!(
            !body.windows(3).any(|window| {
                matches!(
                    window,
                    [
                        Instr::LoadSlotI64(_),
                        Instr::LoadSlotI64(_),
                        Instr::CallResolved(func_index, 2)
                    ] if compiled
                        .functions
                        .get(*func_index)
                        .map(|func| func.name == "score6315")
                        .unwrap_or(false)
                )
            }),
            "non-gcd helper should not keep the old LoadSlotI64/LoadSlotI64/CallResolved sequence: {body:?}"
        );
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn nested_resolved_i64_helper_uses_nested_i64_function_block_6314() {
        let compiled = compile_source(NESTED_RESOLVED_I64_CALL_SOURCE);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));

        profiler::clear();
        profiler::enable();
        let result = vm.run().expect("run nested resolved I64 helper");
        profiler::disable();

        match result {
            Value::I64(value) => assert_eq!(value, 2890),
            other => panic!("expected Int64 sum_score6314 result, got {other:?}"),
        }

        let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
        assert!(
            counts
                .get("ExecutableBlock::I64FunctionNestedCall")
                .copied()
                .unwrap_or(0)
                > 0,
            "resolved helper calls inside I64Function blocks should use nested I64 execution: {counts:?}"
        );
    }
}

mod const_lattice_folding_5086_tests {
    use subset_julia_vm::builtins::BuiltinId;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, Instr};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_core_program(&program).expect("compile failed")
    }

    #[test]
    fn const_numeric_bindings_fold_to_immediate_bytecode_issue_5086() {
        let compiled = compile_source(
            r#"
    const A = 20
    const B = 22
    A + B
    "#,
        );
        let main = &compiled.code[compiled.entry..];

        assert!(
            main.windows(2)
                .any(|w| matches!(w, [Instr::PushI64(42), Instr::ReturnI64])),
            "const-folded main expression should return PushI64(42): {main:?}"
        );
        assert!(
            !main.iter().any(|instr| matches!(instr, Instr::AddI64)),
            "const-folded main expression should not emit AddI64: {main:?}"
        );
    }

    #[test]
    fn const_bool_binding_eliminates_dead_if_branch_issue_5086() {
        let compiled = compile_source(
            r#"
    const FLAG = false
    x = 0
    if FLAG
        x = 1
    else
        x = 2
    end
    x
    "#,
        );
        let main = &compiled.code[compiled.entry..];
        let user_tail = &main[main.len().saturating_sub(12)..];

        assert!(
            !user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::JumpIfZero(_))),
            "const bool condition should remove the conditional jump: {main:?}"
        );
        assert!(
            !user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::PushI64(1))),
            "dead then branch should not be emitted: {main:?}"
        );
        assert!(
            user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::PushI64(2))),
            "live else branch should be emitted: {main:?}"
        );
    }

    #[test]
    fn const_interprocedural_call_folds_to_immediate_bytecode_issue_8443() {
        let compiled = compile_source(
            r#"
    f(x) = x + 1
    f(41)
    "#,
        );
        let main = &compiled.code[compiled.entry..];
        let user_tail = &main[main.len().saturating_sub(20)..];

        assert!(
            user_tail
                .windows(2)
                .any(|w| matches!(w, [Instr::PushI64(42), Instr::ReturnI64])),
            "interprocedural const-folded call should return PushI64(42): {main:?}"
        );
        assert!(
            !user_tail.iter().any(|instr| matches!(
                instr,
                Instr::CallResolved(..)
                    | Instr::CallDynamic(..)
                    | Instr::AddI64
                    | Instr::LoadAddConstI64Slot(..)
            )),
            "interprocedural const-folded call should not emit a runtime call/add: {main:?}"
        );
    }

    #[test]
    fn const_interprocedural_tuple_return_folds_to_tuple_literal_issue_8443() {
        let compiled = compile_source(
            r#"
    pair_const() = (1, 2)
    pair_const()
    "#,
        );
        let main = &compiled.code[compiled.entry..];
        let user_tail = &main[main.len().saturating_sub(20)..];

        assert!(
            user_tail.windows(4).any(|w| matches!(
                w,
                [
                    Instr::PushI64(1),
                    Instr::PushI64(2),
                    Instr::NewTuple(2),
                    Instr::ReturnTuple
                ]
            )),
            "pure constant tuple call should fold to tuple literal construction: {main:?}"
        );
        assert!(
            !user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::CallResolved(..) | Instr::CallDynamic(..))),
            "pure constant tuple call should not emit a runtime call: {main:?}"
        );
    }

    #[test]
    fn const_typeof_literal_folds_to_datatype_issue_8443() {
        let compiled = compile_source("typeof(1)");
        let main = &compiled.code[compiled.entry..];
        let user_tail = &main[main.len().saturating_sub(12)..];

        assert!(
            user_tail.windows(2).any(|w| matches!(
                w,
                [Instr::PushDataType(name), Instr::ReturnAny] if name == "Int64"
            )),
            "typeof(1) should fold to the Int64 DataType object: {main:?}"
        );
        assert!(
            !user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::TypeOf, _))),
            "typeof(1) should not emit a runtime typeof call: {main:?}"
        );
    }

    #[test]
    fn const_typeof_string_literal_keeps_runtime_typeof_issue_8443() {
        let compiled = compile_source(r#"typeof("Array{Float64}")"#);
        let main = &compiled.code[compiled.entry..];
        let user_tail = &main[main.len().saturating_sub(12)..];

        assert!(
            user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::TypeOf, 1))),
            "typeof(::String literal) must keep the runtime TypeOf path because typed Array allocation uses string-backed type sentinels: {main:?}"
        );
        assert!(
            !user_tail
                .iter()
                .any(|instr| matches!(instr, Instr::PushDataType(name) if name == "String")),
            "typeof(::String literal) must not fold to String in this compiler path: {main:?}"
        );
    }
}

mod type_propagation_call_tests {
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::intrinsics::Intrinsic;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::specialize::specialize_function;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::ArrayElementType;
    use subset_julia_vm_bytecode::DynamicCallCandidate;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};
    use subset_julia_vm_bytecode::{Value, ValueType};

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
                Instr::CallDynamic(_)
                    | Instr::CallDynamicBinary(_, _, _)
                    | Instr::CallDynamicBinaryBoth(_, _)
                    | Instr::CallDynamicOrBuiltin(_, _)
                    | Instr::CallFunctionVariable(_)
                    | Instr::CallTypedDispatch(_, _, _, _)
            )
        })
    }

    fn resolve_value(v: &Value, heap: &[subset_julia_vm_bytecode::value::StructInstance]) -> Value {
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
            Instr::CallDynamic(operands) => {
                operands.arg_count == 2 && operands.candidates.len() == 2
            }
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
                Instr::CallDynamic(operands)
                    if operands.arg_count == 1
                        && operands.fallback_func_index == usize::MAX
                        && operands.candidates.iter().any(|c| matches!(c,
                            DynamicCallCandidate::Method(idx)
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

        // The compile-time dispatch-miss site now emits the
        // `ThrowMethodErrorWithArgs` builtin (message + callable name, with the
        // argument values kept on the stack) instead of a bare
        // `Instr::ThrowMethodError`, so a caught MethodError carries upstream's
        // `.f`/`.args` payload (Issue #11374). The raise is still a catchable
        // runtime MethodError with the same message.
        let has_methoderror_message = body.iter().any(|instr| {
            matches!(
                instr,
                Instr::PushStr(msg)
                    if msg.contains("no method matching h6007(::Int64)")
            )
        });
        let has_methoderror_raise = body.iter().any(|instr| {
            matches!(
                instr,
                Instr::CallBuiltin(
                    subset_julia_vm::builtins::BuiltinId::ThrowMethodErrorWithArgs,
                    _
                )
            )
        });
        assert!(
            has_methoderror_message && has_methoderror_raise,
            "direct static no-method call should emit a catchable runtime MethodError \
             via ThrowMethodErrorWithArgs with the argument payload, got {body:?}"
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

        // The SSA pipeline (default since Issue #8832) eliminates the `y` intermediate
        // slot via dead-store elimination and emits a typed specialized call directly.
        // Both the legacy path (StoreSlotI64 / StoreI64("y")) and the SSA path
        // (CallSpecializeI64Slots) produce typed I64 bytecode — accept both.
        assert!(
            body.iter().any(|instr| {
                matches!(instr, Instr::StoreSlotI64(_))
                    || matches!(instr, Instr::StoreI64(name) if name == "y")
                    || matches!(instr, Instr::CallSpecializeI64Slots(_))
            }),
            "call result with inferred Int64 return type should use typed I64 operations \
             (StoreSlotI64, StoreI64, or CallSpecializeI64Slots): {:?}",
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
        // SSA pipeline (Issue #8832 default flip) constant-folds `2.0 * 0.0 * 0.0`
        // when `zr = 0.0` and `zi = 0.0` are literal constants. Use parameter values
        // instead so the n-ary multiply cannot be folded away and the typed F64
        // arithmetic must appear in the bytecode.
        let compiled = compile_source_with_base(
            r#"
    function nary_mul_slot(cr::Float64, ci::Float64)
        zr = cr
        zi = ci
        zi = 2.0 * zr * zi + ci
        zi
    end

    nary_mul_slot(1.0, 1.0)
    "#,
        );
        let func = get_function(&compiled, "nary_mul_slot");
        let body = function_body(&compiled, func);

        // Both paths (legacy slot-based and SSA register-based) must use typed F64
        // arithmetic — not generic Any/dynamic dispatch. The SSA pipeline inlines
        // single-use vars, so `zi` may not appear in slot_names; check absence of
        // dynamic dispatch and presence of typed F64 instructions instead.
        assert!(
            body.iter()
                .all(|instr| !matches!(instr, Instr::CallDynamicBinaryBoth(_, _))),
            "n-ary Float64 operator chain should not force dynamic binary dispatch: {:?}",
            body
        );
        // At least one typed F64 arithmetic instruction must appear.
        assert!(
            body.iter().any(|instr| matches!(
                instr,
                Instr::StoreSlotF64(_)
                    | Instr::LoadMulF64Slot(_)
                    | Instr::LoadAddF64Slot(_)
                    | Instr::MulF64
                    | Instr::AddF64
            )),
            "n-ary Float64 body should contain typed F64 arithmetic: {:?}",
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
}

mod issue_3710_predicate_narrowing {
    use std::collections::{BTreeSet, HashMap};
    use subset_julia_vm::inference_core::{CorePrimitive, CoreType};

    use subset_julia_vm::compile::abstract_interp::InferenceEngine;
    use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    use subset_julia_vm::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
    use subset_julia_vm::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn call(function: &str, args: Vec<Expr>) -> Expr {
        let splat_mask = vec![false; args.len()];
        Expr::Call {
            function: function.into(),
            args,
            kwargs: vec![],
            kwargs_splat_mask: vec![],
            splat_mask,
            span: dummy_span(),
        }
    }

    fn ret(value: Expr) -> Stmt {
        Stmt::Return {
            value: Some(value),
            span: dummy_span(),
        }
    }

    fn unannotated_param(name: &str) -> TypedParam {
        TypedParam::new(name.to_string(), None, dummy_span())
    }

    #[test]
    fn predicate_call_refines_nullable_actual_argument() {
        let is_present = Function {
            name: "is_present_3710".to_string(),
            params: vec![unannotated_param("x")],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![ret(Expr::BinaryOp {
                    op: BinaryOp::NotEgal,
                    left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                    right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
                    span: dummy_span(),
                })],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        };

        let caller = Function {
            name: "nullable_caller_3710".to_string(),
            params: vec![unannotated_param("x")],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: call(
                        "is_present_3710",
                        vec![Expr::Var("x".to_string().into(), dummy_span())],
                    ),
                    then_branch: Block {
                        stmts: vec![ret(Expr::Var("x".to_string().into(), dummy_span()))],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![ret(Expr::Literal(Literal::Int(0), dummy_span()))],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        };

        let mut nullable = BTreeSet::new();
        nullable.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        nullable.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));

        let mut function_table = HashMap::new();
        function_table.insert("is_present_3710".to_string(), is_present);
        function_table.insert("nullable_caller_3710".to_string(), caller.clone());
        let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

        let result = engine.infer_function_with_arg_types(&caller, &[LatticeType::Union(nullable)]);

        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn predicate_call_refines_isa_actual_argument() {
        let is_int = Function {
            name: "is_int_3710".to_string(),
            params: vec![unannotated_param("x")],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![ret(call(
                    "isa",
                    vec![
                        Expr::Var("x".to_string().into(), dummy_span()),
                        Expr::Var("Int64".to_string().into(), dummy_span()),
                    ],
                ))],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        };

        let caller = Function {
            name: "isa_caller_3710".to_string(),
            params: vec![unannotated_param("x")],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: call(
                        "is_int_3710",
                        vec![Expr::Var("x".to_string().into(), dummy_span())],
                    ),
                    then_branch: Block {
                        stmts: vec![ret(Expr::Var("x".to_string().into(), dummy_span()))],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![ret(Expr::Literal(Literal::Int(0), dummy_span()))],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        };

        let mut int_or_string = BTreeSet::new();
        int_or_string.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        int_or_string.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        let mut function_table = HashMap::new();
        function_table.insert("is_int_3710".to_string(), is_int);
        function_table.insert("isa_caller_3710".to_string(), caller.clone());
        let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

        let result =
            engine.infer_function_with_arg_types(&caller, &[LatticeType::Union(int_or_string)]);

        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }
}

mod broadcast_dispatch_analysis_tests {
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::rng::StableRng;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::profiler;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

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
                | Instr::CallDynamic(_)
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
                | Instr::CallSpecializeI64Slots(_)
                | Instr::CallSpecializeInboundsI64Slots(_)
                // Resolved direct-call family (CallResolved added in PR #5411, Issue #5418).
                | Instr::CallResolved(_, _)
                | Instr::CallInbounds(_, _)
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
                // Resolved direct-call family — CallResolved/CallInbounds added in
                // PR #5411 (Issue #5418) must be recognized alongside Call/CallSpecialize.
                Instr::Call(func_index, _)
                | Instr::CallSpecialize(func_index, _)
                | Instr::CallResolved(func_index, _)
                | Instr::CallInbounds(func_index, _) => compiled
                    .functions
                    .get(*func_index)
                    .map(|fi| fi.name.clone()),
                Instr::CallSpecializeI64Slots(operands)
                | Instr::CallSpecializeInboundsI64Slots(operands) => compiled
                    .specializable_functions
                    .get(operands.spec_func_index)
                    .map(|fi| fi.name.clone()),
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
                    if candidates.len() == 1 && candidates[0] == *fallback =>
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
            .map(std::convert::AsRef::as_ref)
            .filter(|f| f.name == name)
            .max_by_key(|f| f.code_end - f.code_start)
    }

    // Runtime profiling helpers (and the test below) require the `profiling`
    // feature, which arms the VM instruction profiler (Issue #5090). In the default
    // build the profiler is a compiled-out no-op, so these are gated off.
    #[cfg(feature = "profiling")]
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

    #[cfg(feature = "profiling")]
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
                .any(|i| matches!(
                    i,
                    Instr::CallSpecialize(_, _)
                        | Instr::CallSpecializeI64Slots(_)
                        | Instr::CallSpecializeInboundsI64Slots(_)
                )),
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
            &compiled.code
                [copyto.code_start..std::cmp::min(copyto.code_start + 80, copyto.code_end)]
        );

        assert!(
            copyto_call_count > 0,
            "copyto! still contains direct call sites (not fully inlined/static)"
        );
        let helper_names = copyto_targets
            .iter()
            .chain(copyto_typed_targets.iter())
            .collect::<Vec<_>>();
        assert!(
            helper_names.iter().any(|name| {
                *name == "_broadcast_getindex"
                    || *name == "_broadcast_getindex_2d"
                    || name.starts_with("_copyto_fastpath")
            }),
            "expected copyto! to call broadcast helper(s), got direct/specialized {:?}, typed {:?}",
            copyto_targets,
            copyto_typed_targets
        );
    }

    #[cfg(feature = "profiling")]
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
}

mod expr_field_type_drift_9673_tests {
    //! Bytecode-level regression guard for Issue #9673 (prevention issue for
    //! #9557).
    //!
    //! Root cause (#9557): `Expr.args` field access had two compiler-facing
    //! type sources. `compile_field_access` (compile/expr/struct_.rs) already
    //! answered `Vector{Any}`, but `infer_expr_type`
    //! (compile/expr/infer/mod.rs) still answered the legacy
    //! `ValueType::Array`. `push!` codegen for a *non-variable* receiver
    //! (`BuiltinOp::Push` in compile/expr/builtin.rs) consults
    //! `infer_expr_type` to decide whether the pushed value needs Float64
    //! coercion, so `push!(e.args, 7)` — where `e.args` is a `FieldAccess`,
    //! not a bare `Var` — silently routed through the Float64 array-coercion
    //! path (`PushI64(7); ToF64; ArrayPush`), corrupting the pushed `Int64`.
    //!
    //! This test pins the fixed bytecode shape directly (no `ToF64` between
    //! `GetExprField` and `ArrayPush`), complementing the runtime-level fixture
    //! `tests/fixtures/metaprogramming/expr_args_push_preserves_int_9557.jl`
    //! and the compile/infer type-table unit tests in
    //! `compile/expr/struct_.rs` (`mod tests`, Issue #9673).

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

    // Issue #9673: use the SAME `compile_with_cache` entry point the real
    // `sjulia` CLI and the fixture-test harness use (it resolves Base/prelude
    // through the persistent cache), not a manual prelude-merge-and-compile.
    // A manual merge via `compile_core_program` resolves `push!` through
    // generic multi-candidate typed dispatch (`CallTypedDispatchOrBuiltin`)
    // instead of the cached pipeline's single-candidate inline `ArrayPush`
    // path, which would make this bytecode pin assert the wrong shape.
    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
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

    const PUSH_EXPR_ARGS_SRC: &str = r#"
    function build_expr_args_9673()
        e = :(f())
        push!(e.args, 7)
        return e
    end

    println(build_expr_args_9673())
    "#;

    /// Issue #9673 / #9557: `push!(e.args, 7)` — a non-`Var` receiver
    /// (`e.args` is a `FieldAccess`) — must emit `GetExprField` -> `PushI64`
    /// -> `ArrayPush` with NO `ToF64` coercion in between. A `ToF64` here
    /// means `infer_expr_type` has drifted away from `compile_field_access`'s
    /// `Vector{Any}` answer for `Expr.args` again.
    #[test]
    fn push_onto_expr_args_field_access_never_emits_to_f64_9673() {
        let compiled = compile_source_with_base(PUSH_EXPR_ARGS_SRC);
        let func = get_function(&compiled, "build_expr_args_9673");
        let body = function_body(&compiled, func);

        let get_args_idx = body
            .iter()
            .position(|instr| matches!(instr, Instr::GetExprField(1)))
            .unwrap_or_else(|| panic!("expected GetExprField(1) (Expr.args) in {:?}", body));
        let array_push_idx = body
            .iter()
            .position(|instr| matches!(instr, Instr::ArrayPush))
            .unwrap_or_else(|| panic!("expected ArrayPush in {:?}", body));
        assert!(
            get_args_idx < array_push_idx,
            "expected GetExprField(1) before ArrayPush, got {:?}",
            body
        );

        let coercions_between: Vec<&Instr> = body[get_args_idx..array_push_idx]
            .iter()
            .filter(|instr| matches!(instr, Instr::ToF64))
            .collect();
        assert!(
            coercions_between.is_empty(),
            "Issue #9673/#9557: push!(e.args, 7) must not coerce the pushed \
             Int64 to Float64 — found ToF64 between GetExprField and ArrayPush \
             in {:?}",
            body
        );
        assert!(
            !body.iter().any(|instr| matches!(instr, Instr::ToF64)),
            "Issue #9673/#9557: build_expr_args_9673 must not emit ToF64 at \
             all, got {:?}",
            body
        );
    }
}

mod comprehension_runtime_dispatch_10315_tests {
    //! Bytecode-level prevention for Issue #10315. The runtime collector can
    //! narrow an unresolved comprehension body to a concrete element type, so
    //! assignment must not turn its placeholder into a statically proven
    //! `Vector{Any}` and resolve the call before the vector exists.

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::types::JuliaType;
    use subset_julia_vm_bytecode::{CompiledProgram, DynamicCallCandidate, FunctionInfo, Instr};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("function {name:?} not found"))
    }

    fn candidate_targets(
        compiled: &CompiledProgram,
        candidate: &DynamicCallCandidate,
        name: &str,
    ) -> bool {
        let DynamicCallCandidate::Method(index) = candidate else {
            return false;
        };
        compiled
            .functions
            .get(*index)
            .is_some_and(|function| function.name == name)
    }

    #[test]
    fn assigned_runtime_typejoin_comprehension_keeps_dynamic_vector_dispatch_10315() {
        let source = r#"
            narrow_int_10315(x) = x < 0 ? "negative" : x
            which_vector_10315(x::Vector{Any}) = :any
            which_vector_10315(x::Vector{Int64}) = :int

            function assigned_runtime_narrow_10315()
                values = [narrow_int_10315(i) for i in 1:3]
                return which_vector_10315(values)
            end

            function assigned_explicit_any_10315()
                values = Any[narrow_int_10315(i) for i in 1:3]
                return which_vector_10315(values)
            end

            function assigned_static_int_10315()
                values = [i for i in 1:3]
                return which_vector_10315(values)
            end

            assigned_runtime_narrow_10315()
        "#;
        let compiled = compile_source(source);
        let function = get_function(&compiled, "assigned_runtime_narrow_10315");
        let body = &compiled.code[function.code_start..function.code_end];

        let has_both_overloads_at_runtime = body.iter().any(|instruction| {
            let Instr::CallDynamic(operands) = instruction else {
                return false;
            };
            if operands.arg_count != 1 {
                return false;
            }
            operands
                .candidates
                .iter()
                .filter(|candidate| candidate_targets(&compiled, candidate, "which_vector_10315"))
                .count()
                == 2
        });
        assert!(
            has_both_overloads_at_runtime,
            "assigned runtime-typejoin comprehension must dispatch across both vector overloads: {body:?}"
        );

        let statically_resolved_to_vector_overload = body.iter().any(|instruction| {
            let Instr::CallResolved(index, 1) = instruction else {
                return false;
            };
            compiled
                .functions
                .get(*index)
                .is_some_and(|target| target.name == "which_vector_10315")
        });
        assert!(
            !statically_resolved_to_vector_overload,
            "runtime-narrowed vector must not resolve a vector overload statically: {body:?}"
        );

        for (control_name, expected_parameter) in [
            (
                "assigned_explicit_any_10315",
                JuliaType::VectorOf(Box::new(JuliaType::Any)),
            ),
            (
                "assigned_static_int_10315",
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            ),
        ] {
            let control = get_function(&compiled, control_name);
            let control_body = &compiled.code[control.code_start..control.code_end];
            let resolved_to_expected_overload = control_body.iter().any(|instruction| {
                let Instr::CallResolved(index, 1) = instruction else {
                    return false;
                };
                compiled.functions.get(*index).is_some_and(|target| {
                    target.name == "which_vector_10315"
                        && target.param_julia_types == [expected_parameter.clone()]
                })
            });
            assert!(
                resolved_to_expected_overload,
                "{control_name} must statically resolve the exact vector overload: {control_body:?}"
            );

            let dynamically_dispatched_to_vector_overload =
                control_body.iter().any(|instruction| {
                    let Instr::CallDynamic(operands) = instruction else {
                        return false;
                    };
                    operands.arg_count == 1
                        && operands.candidates.iter().any(|candidate| {
                            candidate_targets(&compiled, candidate, "which_vector_10315")
                        })
                });
            assert!(
                !dynamically_dispatched_to_vector_overload,
                "{control_name} must keep exact static vector dispatch: {control_body:?}"
            );
        }
    }
}
