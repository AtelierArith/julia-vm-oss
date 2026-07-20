//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod specialization_disable_flags_cache_restore_10334_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower_strict;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm::vm_bytecode_file::{load, save};
    use subset_julia_vm_bytecode::{CompiledProgram, SpecializationDisableFlags};

    const GETINDEX_SOURCE: &str = r#"
module SpecializationGetindex10334
const AliasVector10334 = Vector{Int64}
Base.getindex(v::AliasVector10334, i::Int64) = -111
read_index_10334(v) = v[1]
end
values_10334 = [10, 20]
println(SpecializationGetindex10334.read_index_10334(values_10334))
println(SpecializationGetindex10334.read_index_10334(values_10334))
true
"#;

    const SETINDEX_SOURCE: &str = r#"
module SpecializationSetindex10334
const AliasVector10334 = Vector{Int64}
Base.setindex!(v::AliasVector10334, x::Int64, i::Int64) = v
write_index_10334(v, x) = (v[1] = x; v)
end
values_10334 = [10, 20]
SpecializationSetindex10334.write_index_10334(values_10334, 99)
println(values_10334 == [10, 20])
SpecializationSetindex10334.write_index_10334(values_10334, 77)
println(values_10334 == [10, 20])
true
"#;

    const FIELD_ACCESS_SOURCE: &str = r#"
module SpecializationFieldAccess10334
struct PropertyTarget10334
    value::Float64
end
Base.getproperty(x::PropertyTarget10334, name::Symbol) =
    name === :value ? 2.0 * getfield(x, :value) : getfield(x, name)
end
# The override stays module-owned (the #10334 input). Use the established #8127
# two-argument hot-loop specialization shape; the simpler consumer gap is #11556.
accum_property_10334(x::T, target::SpecializationFieldAccess10334.PropertyTarget10334) where {T} =
    x + target.value
target_10334 = SpecializationFieldAccess10334.PropertyTarget10334(10.0)
acc_10334 = 0.0
for _ in 1:50000
    global acc_10334 += accum_property_10334(1.0, target_10334)
end
println(acc_10334 == 50000 * 21.0)
true
"#;

    struct Case {
        name: &'static str,
        source: &'static str,
        expected_flags: SpecializationDisableFlags,
        expected_output: &'static str,
    }

    const CASES: &[Case] = &[
        Case {
            name: "getindex",
            source: GETINDEX_SOURCE,
            expected_flags: SpecializationDisableFlags {
                array_getindex: true,
                array_setindex: false,
                field_access: false,
            },
            expected_output: "-111\n-111\n",
        },
        Case {
            name: "setindex",
            source: SETINDEX_SOURCE,
            expected_flags: SpecializationDisableFlags {
                array_getindex: false,
                array_setindex: true,
                field_access: false,
            },
            expected_output: "true\ntrue\n",
        },
        Case {
            name: "field_access",
            source: FIELD_ACCESS_SOURCE,
            expected_flags: SpecializationDisableFlags {
                array_getindex: false,
                array_setindex: false,
                field_access: true,
            },
            expected_output: "true\n",
        },
    ];

    fn context_flags(compiled: &CompiledProgram) -> SpecializationDisableFlags {
        let context = compiled
            .compile_context
            .as_ref()
            .expect("policy corpus must create a runtime compile context");
        SpecializationDisableFlags {
            array_getindex: context.disable_array_getindex_specialization,
            array_setindex: context.disable_array_setindex_specialization,
            field_access: context.disable_field_access_specialization,
        }
    }

    fn run_output(compiled: CompiledProgram, seed: u64) -> String {
        let mut vm = Vm::new_program(compiled, StableRng::new(seed));
        vm.run().expect("policy corpus should execute");
        vm.get_output().to_string()
    }

    #[test]
    fn specialization_disable_flags_survive_sjvmbc_restore_10334() {
        let dir = tempfile::tempdir().expect("create .sjvmbc test directory");

        for (index, case) in CASES.iter().enumerate() {
            let program = parse_and_lower_strict(case.source).expect("policy corpus should lower");
            let compiled = compile_with_cache(&program).expect("policy corpus should compile");
            assert_eq!(
                compiled.specialization_disable_flags, case.expected_flags,
                "{} fresh persisted flags",
                case.name
            );
            assert_eq!(
                context_flags(&compiled),
                case.expected_flags,
                "{} fresh runtime gates",
                case.name
            );

            let path = dir.path().join(format!("{}_10334.sjvmbc", case.name));
            save(&program, &compiled, &path).expect("save should succeed");
            let fresh_output = run_output(compiled, index as u64);
            assert_eq!(fresh_output, case.expected_output, "{} fresh", case.name);

            let restored = load(&path).expect("load should succeed");
            assert_eq!(
                restored.specialization_disable_flags, case.expected_flags,
                "{} restored persisted flags",
                case.name
            );
            assert_eq!(
                context_flags(&restored),
                case.expected_flags,
                "{} restored runtime gates",
                case.name
            );
            assert_eq!(
                run_output(restored, index as u64),
                fresh_output,
                "{} restored execution must preserve override dispatch",
                case.name
            );
        }
    }
}

mod type_alias_signature_source_order_11086_tests {
    use subset_julia_vm::compile::host_support::{compile_core_program, compile_with_cache};
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    const SOURCE: &str = r#"
    later_error_11086 = nothing
    try
        later_cache_method_11086(x::LaterCacheAlias11086) = x
    catch e
        global later_error_11086 = e
    end
    const LaterCacheAlias11086 = Int64
    println(later_error_11086 isa UndefVarError)

    const EarlierCacheAlias11086 = Int64
    earlier_cache_method_11086(x::EarlierCacheAlias11086) = x + 1
    println(earlier_cache_method_11086(41))
    "#;

    fn run_output(compiled: subset_julia_vm_bytecode::CompiledProgram) -> String {
        let mut vm = Vm::new_program(compiled, StableRng::new(11086));
        vm.run().expect("VM execution failed");
        vm.get_output().to_string()
    }

    fn cached_output() -> String {
        let program = parse_and_lower(SOURCE).expect("parse_and_lower failed");
        run_output(compile_with_cache(&program).expect("cached compile failed"))
    }

    fn uncached_output() -> String {
        let program = parse_and_lower(SOURCE).expect("parse_and_lower failed");
        run_output(compile_core_program(&program).expect("uncached compile failed"))
    }

    #[test]
    fn source_order_matches_on_uncached_prime_and_cached_lanes_11086() {
        let uncached = uncached_output();
        let prime = cached_output();
        let cached = cached_output();
        assert_eq!(uncached, "true\n42\n");
        assert_eq!(prime, uncached);
        assert_eq!(cached, uncached);
    }
}

mod constructor_struct_provenance_10959_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;

    fn assert_module(module: &subset_julia_vm::ir::core::Module) {
        assert!(
            module.structs.iter().all(|def| def.is_base_origin),
            "all structs in Base module {} must retain Base provenance",
            module.name
        );
        for submodule in &module.submodules {
            assert_module(submodule);
        }
    }

    #[test]
    fn base_struct_provenance_is_marked_recursively_issue_10959() {
        let Some(program) = subset_julia_vm::base_loader::get_base_program() else {
            panic!("Base program should load");
        };
        assert!(!program.structs.is_empty(), "Base must declare structs");
        assert!(
            program.structs.iter().all(|def| def.is_base_origin),
            "all top-level Base structs must retain Base provenance"
        );
        for module in &program.modules {
            assert_module(module);
        }
    }

    #[test]
    fn weakkeydict_compile_context_keeps_base_provenance_issue_10959() {
        let program = parse_and_lower("true").expect("lower user program");
        let compiled = compile_with_cache(&program).expect("compile user program");
        let context = compiled.compile_context.expect("runtime compile context");
        for name in ["WeakKeyDict", "UnitRange", "Channel"] {
            let parametric = context
                .parametric_structs
                .get(name)
                .unwrap_or_else(|| panic!("{name} parametric schema"));
            assert!(
                parametric.def.is_base_origin,
                "{name} must retain Base provenance"
            );
        }
    }
}

mod cached_base_inference_parity_6538_tests {
    //! Issue #6538: cached-Base compile path must give the inference engine the
    //! same view of multi-method Base callees as a fresh full compile.
    //!
    //! Before the fix, `build_method_tables` short-circuited cached Base functions
    //! without registering their `MethodSig`s into the inference engine, and
    //! `InferenceEngine::add_function` drops multi-signature names as ambiguous,
    //! so a user function calling a multi-method Base function (without a
    //! registered tfunc) inferred `Any` on the cached path while the uncached
    //! path (`SUBSET_JULIA_VM_DISABLE_CACHE=1`) inferred precisely via the
    //! method-table snapshot channel.
    //!
    //! These tests pin the parity end to end: the same source is compiled through
    //! `compile_with_cache` (cached path) and `compile_core_program` (the exact
    //! call the disabled-cache path makes), and `Base.infer_return_type` output
    //! must agree — and match the precise expected types (verified against
    //! upstream julia 1.12).

    use subset_julia_vm::compile::host_support::{compile_core_program, compile_with_cache};
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    /// Multi-method Base callees WITHOUT registered tfuncs (`mod1`, `factorial`,
    /// `flipsign`) plus the original #6538 repro (`error`, now also covered by the
    /// #6532 tfunc). Expected lines verified against upstream julia 1.12:
    /// `Union{}`, `Int64`, `Int64`, `Int64`.
    const INFERENCE_BATTERY_SOURCE: &str = r#"
    b_err6538() = error("x")
    b_mod1_6538(x::Int) = mod1(x, 7)
    b_fact6538(n::Int) = factorial(n)
    b_flipsign6538(x::Int, y::Int) = flipsign(x, y)
    println(Base.infer_return_type(b_err6538, Tuple{}))
    println(Base.infer_return_type(b_mod1_6538, Tuple{Int}))
    println(Base.infer_return_type(b_fact6538, Tuple{Int}))
    println(Base.infer_return_type(b_flipsign6538, Tuple{Int,Int}))
    "#;

    const EXPECTED_OUTPUT: &str = "Union{}\nInt64\nInt64\nInt64\n";

    fn run_output(compiled: subset_julia_vm_bytecode::CompiledProgram) -> String {
        let mut vm = Vm::new_program(compiled, StableRng::new(42));
        vm.run().expect("VM execution failed");
        vm.get_output().to_string()
    }

    fn cached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        let compiled = compile_with_cache(&program).expect("cached compile failed");
        run_output(compiled)
    }

    fn uncached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        // Exactly what `compile_with_cache` does when
        // SUBSET_JULIA_VM_DISABLE_CACHE=1 is set (compile/cache.rs).
        let compiled = compile_core_program(&program).expect("uncached compile failed");
        run_output(compiled)
    }

    /// The structural pin: cached-path inference for multi-method Base callees
    /// must match the uncached path (Issue #6538).
    #[test]
    fn cached_base_inference_matches_uncached_for_multi_method_callees_6538() {
        let cached = cached_path_output(INFERENCE_BATTERY_SOURCE);
        let uncached = uncached_path_output(INFERENCE_BATTERY_SOURCE);
        assert_eq!(
            cached, uncached,
            "cached-Base path inference diverged from the uncached path \
             (Issue #6538)\ncached:\n{cached}\nuncached:\n{uncached}"
        );
    }

    /// The precision pin: both paths must produce the upstream-verified precise
    /// types, not a tfunc-registry `Any` fallback.
    #[test]
    fn cached_base_inference_is_precise_for_multi_method_callees_6538() {
        let cached = cached_path_output(INFERENCE_BATTERY_SOURCE);
        assert_eq!(
            cached, EXPECTED_OUTPUT,
            "cached-Base path must infer multi-method Base callees precisely \
             via the seeded method tables (Issue #6538)"
        );
    }
}

mod keys_values_pairs_cached_base_8602_tests {
    //! Issue #8602 (follow-up of #8555, slice of #8442): retiring the
    //! Base-cache-disable fallback for user `keys`/`values`/`pairs` extensions.
    //!
    //! Historically (Issue #4671) any user `keys`/`values`/`pairs` method
    //! disabled the whole Base cache: cached Base bytecode carries
    //! compile-time-frozen dispatch candidate lists
    //! (`CallTypedDispatchOrBuiltin(DictKeys/DictValues/DictPairs, ..)` and the
    //! generic `CallTypedDispatch`-family), so a user method added later was
    //! invisible to those sites. The retirement keeps the Base cache loaded and
    //! refreshes the frozen candidate lists post-merge
    //! (`compile/pipeline_ctx.rs::refresh_cached_base_dispatch_candidates`),
    //! bypassing only for methods that could pirate Base-known types (the exact
    //! `keys(::Dict{String,Float64})` scenario #4671 was filed about).
    //!
    //! Expected outputs verified against upstream julia 1.12.

    use subset_julia_vm::compile::host_support::{
        clear_compile_cache, compile_core_program, compile_with_cache, is_compile_cache_initialized,
    };
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    /// User-type-anchored keys/values/pairs extension. Upstream julia prints
    /// `[:a, :b] / [1, 2] / true / 3 / [:x] / (:p, :q)`.
    const ANCHORED_DICT_VIEWS_SOURCE: &str = r#"
    import Base: keys, values, pairs

    struct SmallMap8602
        ks::Vector{Symbol}
        vs::Vector{Int}
    end

    keys(m::SmallMap8602) = m.ks
    values(m::SmallMap8602) = m.vs
    pairs(m::SmallMap8602) = [k => v for (k, v) in zip(m.ks, m.vs)]

    m = SmallMap8602([:a, :b], [1, 2])
    println(keys(m))
    println(values(m))
    println(pairs(m) == [:a => 1, :b => 2])
    println(sum(values(m)))
    d = Dict(:x => 10)
    println(collect(keys(d)))
    println(keys((p = 1, q = 2)))
    "#;

    const ANCHORED_EXPECTED_OUTPUT: &str = "[:a, :b]\n[1, 2]\ntrue\n3\n[:x]\n(:p, :q)\n";

    /// Base-type piracy (`values` over a Base-known `Dict` instantiation — the
    /// scenario Issue #4671 was originally filed about) still takes the
    /// full-compile bypass; upstream julia prints `42`.
    const PIRACY_DICT_VIEWS_SOURCE: &str = r#"
    import Base: values

    values(d::Dict{String,Float64}) = 42

    d = Dict("x" => 1.0)
    println(values(d))
    "#;

    fn run_output(compiled: subset_julia_vm_bytecode::CompiledProgram) -> String {
        let mut vm = Vm::new_program(compiled, StableRng::new(42));
        vm.run().expect("VM execution failed");
        vm.get_output().to_string()
    }

    fn cached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        let compiled = compile_with_cache(&program).expect("cached compile failed");
        run_output(compiled)
    }

    fn uncached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        let compiled = compile_core_program(&program).expect("uncached compile failed");
        run_output(compiled)
    }

    /// The soundness gate that justified the old disable: user-type-anchored
    /// `keys`/`values`/`pairs` methods must keep upstream-identical semantics on
    /// the cached path.
    #[test]
    fn user_keys_values_pairs_semantics_on_cached_base_path_8602() {
        clear_compile_cache();
        let cached = cached_path_output(ANCHORED_DICT_VIEWS_SOURCE);
        assert_eq!(
            cached, ANCHORED_EXPECTED_OUTPUT,
            "cached-Base path must dispatch user keys/values/pairs methods \
             (Issue #8602 / #4671)"
        );
        let uncached = uncached_path_output(ANCHORED_DICT_VIEWS_SOURCE);
        assert_eq!(
            cached, uncached,
            "cached path diverged from the full-compile path for a user \
             keys/values/pairs extension"
        );
    }

    /// The "Base cache was loaded, not fully recompiled" telemetry pin:
    /// `compile_with_cache` initializes the thread-local Base cache only on the
    /// non-bypass path (`get_or_init_base_cache`), so a still-uninitialized cache
    /// after the compile would prove the old #4671 full-compile bypass fired.
    #[test]
    fn user_keys_values_pairs_compile_loads_base_cache_instead_of_bypassing_8602() {
        clear_compile_cache();
        assert!(!is_compile_cache_initialized());
        let output = cached_path_output(ANCHORED_DICT_VIEWS_SOURCE);
        assert!(
            is_compile_cache_initialized(),
            "compiling a user-type-anchored keys/values/pairs program must load \
             the Base cache, not take the full-compile bypass (Issue #8602)"
        );
        assert_eq!(output, ANCHORED_EXPECTED_OUTPUT);
    }

    /// The fixture-chunk scenario: the Base cache is already warm from a previous
    /// compile in the same process/thread when the keys/values/pairs program
    /// arrives. The user methods must be visible through the reused Base
    /// bytecode.
    #[test]
    fn user_keys_values_pairs_after_warm_base_cache_8602() {
        clear_compile_cache();
        // Warm the thread-local Base cache with an unrelated program first.
        let warmup = parse_and_lower(r#"println(collect(keys(Dict(:w => 1))))"#)
            .expect("parse_and_lower failed");
        let warm_out = run_output(compile_with_cache(&warmup).expect("warmup compile failed"));
        assert_eq!(warm_out, "[:w]\n");
        assert!(is_compile_cache_initialized());

        let cached = cached_path_output(ANCHORED_DICT_VIEWS_SOURCE);
        assert_eq!(
            cached, ANCHORED_EXPECTED_OUTPUT,
            "warm-cache compile must still see the user keys/values/pairs \
             methods (Issue #4671 regression)"
        );
    }

    /// A later cache-path compile WITHOUT the user methods must not inherit the
    /// previous program's refreshed candidates (the merge copies pristine cached
    /// Base bytecode per compile).
    #[test]
    fn keys_values_pairs_refresh_does_not_leak_into_later_compiles_8602() {
        clear_compile_cache();
        let _ = cached_path_output(ANCHORED_DICT_VIEWS_SOURCE);
        let neutral = parse_and_lower(r#"println(collect(values(Dict("k" => 7))))"#)
            .expect("parse_and_lower failed");
        let out = run_output(compile_with_cache(&neutral).expect("neutral compile failed"));
        assert_eq!(out, "[7]\n");
    }

    /// Base-type piracy keeps upstream semantics through the (retained) bypass:
    /// a user method over a Base-known Dict instantiation cannot be reached by
    /// candidate refresh alone (cached Base bytecode may hold static resolutions
    /// over Base-known types), so the full-compile path must still fire.
    #[test]
    fn base_type_pirating_dict_views_stay_correct_8602() {
        clear_compile_cache();
        let cached = cached_path_output(PIRACY_DICT_VIEWS_SOURCE);
        assert_eq!(cached, "42\n");
        assert!(
            !is_compile_cache_initialized(),
            "a Base-type-pirating values(::Dict{{String,Float64}}) must still \
             take the full-compile bypass (Issue #8602)"
        );
    }

    /// Structural pin of the refresh mechanics (Issue #8602): after a cached
    /// compile, every keys/values/pairs dispatch site inside the cached Base
    /// segment whose arity matches a user hook method must carry that user
    /// method's global index in its candidate list.
    ///
    /// Measured layout of the current Base bytecode (2026-07, for orientation —
    /// intentionally NOT pinned): 0 named `CallTypedDispatch*` hook sites, 2
    /// nameless `CallDynamic` hook sites (`_dump_impl`'s `keys(x)`/`values(x)`,
    /// io.jl), and 5 `CallBuiltin(DictKeys/DictValues/DictPairs, _)` sites. The
    /// `CallBuiltin` sites are emitted only for receivers statically known to be
    /// Base-native pairs-view types (arrays/tuples/NamedTuples), which an
    /// anchored user method can never match, so they are correct without any
    /// refresh; piracy over those Base-known types keeps the full-compile bypass.
    #[test]
    fn cached_base_hook_sites_carry_user_candidates_after_refresh_8602() {
        use subset_julia_vm::vm::instr::{DynamicCallCandidate, Instr};
        clear_compile_cache();
        let program = parse_and_lower(ANCHORED_DICT_VIEWS_SOURCE).expect("parse_and_lower failed");
        let compiled = compile_with_cache(&program).expect("cached compile failed");
        let base_count = compiled.base_function_count;
        let base_code_end = compiled.functions[..base_count]
            .iter()
            .map(|f| f.code_end)
            .max()
            .unwrap_or(0);
        let is_hook = |name: &str| {
            matches!(
                name.rsplit('.').next().unwrap_or(name),
                "keys" | "values" | "pairs"
            )
        };
        let fn_is_hook = |idx: usize| {
            compiled
                .functions
                .get(idx)
                .is_some_and(|f| is_hook(&f.name))
        };
        // The anchored program defines exactly one arity-1 user method per hook.
        let user_hook_arity = 1usize;
        let has_user_candidate = |c: &[usize]| c.iter().any(|&i| i >= base_count && fn_is_hook(i));
        let mut hook_sites = 0usize;
        for (pc, instr) in compiled.code[..base_code_end].iter().enumerate() {
            match instr {
                Instr::CallTypedDispatch(name, arg_count, _, c)
                    if is_hook(name) && *arg_count == user_hook_arity =>
                {
                    hook_sites += 1;
                    assert!(
                        has_user_candidate(c),
                        "unrefreshed CallTypedDispatch `{name}` at pc={pc}"
                    );
                }
                Instr::CallTypedDispatchOrBuiltin(_, name, arg_count, c)
                | Instr::CallTypedDispatchOrBuiltinResult(_, name, arg_count, c)
                    if is_hook(name) && *arg_count == user_hook_arity =>
                {
                    hook_sites += 1;
                    assert!(
                        has_user_candidate(c),
                        "unrefreshed CallTypedDispatchOrBuiltin `{name}` at pc={pc}"
                    );
                }
                Instr::CallTypedDispatchOrBuiltinStoreDict(op)
                | Instr::CallTypedDispatchOrBuiltinStoreDictResult(op)
                    if is_hook(&op.function_name) && op.arg_count == user_hook_arity =>
                {
                    hook_sites += 1;
                    assert!(
                        has_user_candidate(&op.candidates),
                        "unrefreshed StoreDict dispatch `{}` at pc={pc}",
                        op.function_name
                    );
                }
                // No arity at push sites: the refresh appends unconditionally.
                Instr::PushResolvedFunction(op) if is_hook(&op.name) => {
                    hook_sites += 1;
                    assert!(
                        has_user_candidate(&op.candidate_indices),
                        "unrefreshed PushResolvedFunction `{}` at pc={pc}",
                        op.name
                    );
                }
                Instr::CallDynamic(operands) => {
                    let hookish = fn_is_hook(operands.fallback_func_index)
                        || operands.candidates.iter().any(|cand| match cand {
                            DynamicCallCandidate::Method(i) => fn_is_hook(*i),
                            DynamicCallCandidate::NativeIterator(_) => false,
                        });
                    if hookish && operands.arg_count == user_hook_arity {
                        hook_sites += 1;
                        assert!(
                            operands.candidates.iter().any(|cand| match cand {
                                DynamicCallCandidate::Method(i) =>
                                    *i >= base_count && fn_is_hook(*i),
                                DynamicCallCandidate::NativeIterator(_) => false,
                            }),
                            "unrefreshed CallDynamic hook site at pc={pc}"
                        );
                    }
                }
                Instr::CallDynamicOrBuiltin(_, c) if c.iter().any(|&i| fn_is_hook(i)) => {
                    hook_sites += 1;
                    assert!(
                        has_user_candidate(c),
                        "unrefreshed CallDynamicOrBuiltin hook site at pc={pc}"
                    );
                }
                _ => {}
            }
        }
        assert!(
            hook_sites > 0,
            "expected at least one keys/values/pairs dispatch site in the cached \
             Base segment (found none — did the Base bytecode layout change? \
             Update this test's orientation comment)"
        );
    }
}

mod promote_rule_cached_base_8555_tests {
    //! Issue #8555 (slice of #8442): retiring the Base-cache-disable fallback
    //! for user `promote_rule` extensions.
    //!
    //! Historically (Issue #4048) any user `promote_rule` method disabled the
    //! whole Base cache: cached Base bytecode carries compile-time-frozen
    //! dispatch candidate lists (`CallTypedDispatch`-family), so a user method
    //! added later was invisible to cached `promote_type` call sites and
    //! `promote_type(MyReal, Float64)` silently fell through to the generic
    //! `Union{}` fallback + `typejoin` (`Any`). The retirement keeps the Base
    //! cache loaded and instead refreshes the frozen candidate lists post-merge
    //! (`compile/pipeline_ctx.rs::refresh_cached_base_dispatch_candidates`),
    //! bypassing only for methods that could pirate Base-known type pairs.
    //!
    //! Expected outputs verified against upstream julia 1.12
    //! (`scripts/fixture_julia_parity.sh`-style manual runs).

    use subset_julia_vm::compile::host_support::{
        clear_compile_cache, compile_core_program, compile_with_cache, is_compile_cache_initialized,
    };
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    /// User-type-anchored promote_rule extension: the exact #4048 soundness
    /// scenario. Upstream julia prints `MyReal / MyReal / Float64 / MyReal MyReal
    /// / 2.0`.
    const ANCHORED_PROMOTE_RULE_SOURCE: &str = r#"
    import Base: promote_rule, convert

    struct MyReal8555
        value::Float64
    end

    promote_rule(::Type{MyReal8555}, ::Type{Float64}) = MyReal8555
    convert(::Type{MyReal8555}, x::Float64) = MyReal8555(x)

    println(promote_type(MyReal8555, Float64))
    println(promote_type(Float64, MyReal8555))
    println(promote_type(Int64, Float64))
    a, b = promote(MyReal8555(1.5), 2.0)
    println(typeof(a), " ", typeof(b))
    println(b.value)
    "#;

    const ANCHORED_EXPECTED_OUTPUT: &str =
        "MyReal8555\nMyReal8555\nFloat64\nMyReal8555 MyReal8555\n2.0\n";

    /// Base-type piracy still takes the full-compile bypass; upstream julia
    /// prints `Int64`.
    const PIRACY_PROMOTE_RULE_SOURCE: &str = r#"
    import Base: promote_rule

    promote_rule(::Type{Char}, ::Type{Bool}) = Int64

    println(promote_type(Char, Bool))
    "#;

    fn run_output(compiled: subset_julia_vm_bytecode::CompiledProgram) -> String {
        let mut vm = Vm::new_program(compiled, StableRng::new(42));
        vm.run().expect("VM execution failed");
        vm.get_output().to_string()
    }

    fn cached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        let compiled = compile_with_cache(&program).expect("cached compile failed");
        run_output(compiled)
    }

    fn uncached_path_output(src: &str) -> String {
        let program = parse_and_lower(src).expect("parse_and_lower failed");
        let compiled = compile_core_program(&program).expect("uncached compile failed");
        run_output(compiled)
    }

    /// The soundness gate that justified the old disable: a user-type-anchored
    /// `promote_rule` must keep upstream-identical semantics on the cached path.
    #[test]
    fn user_promote_rule_semantics_on_cached_base_path_8555() {
        clear_compile_cache();
        let cached = cached_path_output(ANCHORED_PROMOTE_RULE_SOURCE);
        assert_eq!(
            cached, ANCHORED_EXPECTED_OUTPUT,
            "cached-Base path must dispatch user promote_rule methods \
             (Issue #8555 / #4048)"
        );
        let uncached = uncached_path_output(ANCHORED_PROMOTE_RULE_SOURCE);
        assert_eq!(
            cached, uncached,
            "cached path diverged from the full-compile path for a user \
             promote_rule extension"
        );
    }

    /// The "Base cache was loaded, not fully recompiled" telemetry pin:
    /// `compile_with_cache` initializes the thread-local Base cache only on the
    /// non-bypass path (`get_or_init_base_cache`), so a still-uninitialized cache
    /// after the compile would prove the old full-compile bypass fired.
    #[test]
    fn user_promote_rule_compile_loads_base_cache_instead_of_bypassing_8555() {
        clear_compile_cache();
        assert!(!is_compile_cache_initialized());
        let output = cached_path_output(ANCHORED_PROMOTE_RULE_SOURCE);
        assert!(
            is_compile_cache_initialized(),
            "compiling a user-type-anchored promote_rule program must load the \
             Base cache, not take the full-compile bypass (Issue #8555)"
        );
        assert_eq!(output, ANCHORED_EXPECTED_OUTPUT);
    }

    /// The #4048 fixture-chunk scenario: the Base cache is already warm from a
    /// previous compile in the same process/thread when the promote_rule program
    /// arrives. The user method must be visible through the reused Base bytecode.
    #[test]
    fn user_promote_rule_after_warm_base_cache_8555() {
        clear_compile_cache();
        // Warm the thread-local Base cache with an unrelated program first.
        let warmup = parse_and_lower("println(promote_type(Int64, Float64))")
            .expect("parse_and_lower failed");
        let warm_out = run_output(compile_with_cache(&warmup).expect("warmup compile failed"));
        assert_eq!(warm_out, "Float64\n");
        assert!(is_compile_cache_initialized());

        let cached = cached_path_output(ANCHORED_PROMOTE_RULE_SOURCE);
        assert_eq!(
            cached, ANCHORED_EXPECTED_OUTPUT,
            "warm-cache compile must still see the user promote_rule method \
             (Issue #4048 regression)"
        );
    }

    /// A later cache-path compile WITHOUT the user promote_rule must not inherit
    /// the previous program's refreshed candidates (the merge copies pristine
    /// cached Base bytecode per compile).
    #[test]
    fn promote_rule_refresh_does_not_leak_into_later_compiles_8555() {
        clear_compile_cache();
        let _ = cached_path_output(ANCHORED_PROMOTE_RULE_SOURCE);
        let neutral =
            parse_and_lower("println(promote_type(Int64, Int32))").expect("parse_and_lower failed");
        let out = run_output(compile_with_cache(&neutral).expect("neutral compile failed"));
        assert_eq!(out, "Int64\n");
    }

    /// Base-type piracy keeps upstream semantics through the (retained) bypass.
    #[test]
    fn base_type_pirating_promote_rule_stays_correct_8555() {
        clear_compile_cache();
        let cached = cached_path_output(PIRACY_PROMOTE_RULE_SOURCE);
        assert_eq!(cached, "Int64\n");
        assert!(
            !is_compile_cache_initialized(),
            "a Base-type-pirating promote_rule must still take the full-compile \
             bypass (Issue #8555)"
        );
    }

    /// User iterator-trait extension (#4088 scenario): `IteratorSize`/
    /// `IteratorEltype`/`eltype` anchored to a user type must drive `collect`
    /// through cached Base bytecode exactly like a full compile. Upstream julia
    /// 1.12 prints `[3, 2, 1] / Vector{Int64} / SizeUnknown() / HasEltype()`
    /// (modulo the known `Base.` display-prefix gap).
    const ANCHORED_ITERATOR_TRAITS_SOURCE: &str = r#"
    import Base: iterate, eltype, IteratorSize, IteratorEltype

    struct Countdown8555
        start::Int
    end

    iterate(c::Countdown8555) = c.start <= 0 ? nothing : (c.start, c.start - 1)
    iterate(c::Countdown8555, state::Int) = state <= 0 ? nothing : (state, state - 1)
    IteratorSize(::Type{Countdown8555}) = Base.SizeUnknown()
    IteratorEltype(::Type{Countdown8555}) = Base.HasEltype()
    eltype(::Type{Countdown8555}) = Int

    v = collect(Countdown8555(3))
    println(v)
    println(typeof(v))
    println(Base.IteratorSize(Countdown8555))
    println(Base.IteratorEltype(Countdown8555))
    "#;

    const ANCHORED_ITERATOR_TRAITS_EXPECTED: &str =
        "[3, 2, 1]\nVector{Int64}\nSizeUnknown()\nHasEltype()\n";

    /// The iterator-trait analogue of the promote_rule soundness gate: the
    /// cached path must match the full-compile path (which matches upstream).
    /// Before #8555 the cached path inferred `Vector{Any}` because the user
    /// `eltype` method was invisible to `_collect`'s frozen candidate lists.
    #[test]
    fn user_iterator_traits_semantics_on_cached_base_path_8555() {
        clear_compile_cache();
        let cached = cached_path_output(ANCHORED_ITERATOR_TRAITS_SOURCE);
        assert_eq!(
            cached, ANCHORED_ITERATOR_TRAITS_EXPECTED,
            "cached-Base path must dispatch user iterator-trait methods \
             (Issue #8555 / #4088)"
        );
        assert!(
            is_compile_cache_initialized(),
            "a user-type-anchored iterator-trait program must load the Base \
             cache, not take the full-compile bypass (Issue #8555)"
        );
        let uncached = uncached_path_output(ANCHORED_ITERATOR_TRAITS_SOURCE);
        assert_eq!(
            cached, uncached,
            "cached path diverged from the full-compile path for a user \
             iterator-trait extension"
        );
    }
}
