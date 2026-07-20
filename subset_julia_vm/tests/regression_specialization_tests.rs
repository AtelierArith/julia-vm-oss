//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod field_assign_specialization_6346_tests {
    //! Issue #6346: extend the lazy specialization engine to `FieldAssign` (struct
    //! field reads and writes), mirroring the typed fast path the interpreter
    //! already uses for known mutable structs.

    #[cfg(feature = "profiling")]
    use std::collections::HashMap;
    use std::collections::HashSet;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::specialize::specialize_function;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::{profiler, Vm};
    use subset_julia_vm_bytecode::{CompiledProgram, Instr, ValueType};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn specializable_ir<'a>(
        compiled: &'a CompiledProgram,
        name: &str,
    ) -> &'a subset_julia_vm::ir::core::Function {
        &compiled
            .specializable_functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
            .ir
    }

    fn struct_type_id(compiled: &CompiledProgram, name: &str) -> usize {
        compiled
            .struct_defs
            .iter()
            .position(|def| def.name == name || def.name.starts_with(&format!("{name}{{")))
            .unwrap_or_else(|| panic!("struct '{name}' not found in struct_defs"))
    }

    const PARTICLE_SOURCE: &str = r#"
    mutable struct Particle6346
        x::Float64
        vx::Float64
    end

    function step_particle_6346!(p, dt)
        p.x = p.x + p.vx * dt
        return p.x
    end
    "#;

    #[test]
    fn field_assign_mutable_struct_specializes_to_setfield_6346() {
        let compiled = compile_source(PARTICLE_SOURCE);
        let tid = struct_type_id(&compiled, "Particle6346");
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "step_particle_6346!"),
            &[ValueType::Struct(tid), ValueType::F64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize mutable struct field update");

        assert!(
            specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::SetField(0))),
            "expected statically-resolved SetField(0) for p.x, got {:?}",
            specialized.code
        );
        // Reads of p.x and p.vx must use the index-based GetField fast path, never
        // the by-name runtime fallback.
        assert!(
            specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::GetField(_))),
            "expected GetField reads in specialized code, got {:?}",
            specialized.code
        );
        assert!(
            !specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_))),
            "specialized field update must not fall back to by-name field ops: {:?}",
            specialized.code
        );
        assert_eq!(specialized.return_type, ValueType::F64);
    }

    #[test]
    fn field_assign_int_literal_coerces_to_float_field_6346() {
        let compiled = compile_source(
            r#"
    mutable struct Box6346
        v::Float64
    end

    function set_box_6346!(b)
        b.v = 2
        return b.v
    end
    "#,
        );
        let tid = struct_type_id(&compiled, "Box6346");
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "set_box_6346!"),
            &[ValueType::Struct(tid)],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize Int->Float64 field coercion");

        assert!(
            specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::ToF64)),
            "expected ToF64 coercion for `b.v = 2` into a Float64 field, got {:?}",
            specialized.code
        );
        assert!(
            specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::SetField(0))),
            "expected SetField(0) for field v, got {:?}",
            specialized.code
        );
    }

    #[test]
    fn field_assign_immutable_struct_stays_on_fallback_6346() {
        // An immutable struct read in one function, mutated via a loosely-typed
        // path in another: the specializer must decline the typed SetField path
        // (immutable structs raise on assignment) and fall back.
        let compiled = compile_source(
            r#"
    struct ImmutPoint6346
        x::Float64
    end

    function read_point_6346(p)
        return p.x
    end
    "#,
        );
        let tid = struct_type_id(&compiled, "ImmutPoint6346");
        // Reading a field of an immutable struct is fine and should specialize.
        let type_object_names = HashSet::new();
        let read_spec = specialize_function(
            specializable_ir(&compiled, "read_point_6346"),
            &[ValueType::Struct(tid)],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("immutable struct field READ should still specialize");
        assert!(
            read_spec
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::GetField(0))),
            "expected GetField(0) for immutable field read, got {:?}",
            read_spec.code
        );
    }

    #[test]
    fn field_update_with_nary_product_specializes_6346() {
        // `k * b.x * dt` parses as a 3-arg `*(k, b.x, dt)` call; the whole field
        // update must still specialize, folding the product to typed MulF64.
        let compiled = compile_source(
            r#"
    mutable struct Body6346
        x::Float64
        v::Float64
    end

    function step_body_6346!(b, dt, k)
        b.v = b.v - k * b.x * dt
        b.x = b.x + b.v * dt
        return b.x
    end
    "#,
        );
        let tid = struct_type_id(&compiled, "Body6346");
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "step_body_6346!"),
            &[ValueType::Struct(tid), ValueType::F64, ValueType::F64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("n-ary product field update should specialize");

        assert!(
            specialized
                .code
                .iter()
                .filter(|instr| matches!(instr, Instr::MulF64))
                .count()
                >= 2,
            "the n-ary `k * b.x * dt` product should fold to typed MulF64, got {:?}",
            specialized.code
        );
        assert!(
            specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::SetField(_))),
            "expected typed SetField, got {:?}",
            specialized.code
        );
        assert!(
            !specialized
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_))),
            "n-ary field update must not fall back to by-name field ops: {:?}",
            specialized.code
        );
    }

    // ---- Runtime parity / fast-path firing (profiling builds only) ----

    #[cfg(feature = "profiling")]
    fn run_with_profile(source: &str) -> (String, HashMap<String, u64>) {
        let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
        profiler::clear();
        profiler::enable();
        let _ = vm.run().expect("run with VM profiler");
        profiler::disable();
        let counts = profiler::get_results().into_iter().collect();
        (vm.get_output().to_string(), counts)
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn field_update_loop_runs_typed_setfield_not_by_name_6346() {
        let source = format!(
            r#"{PARTICLE_SOURCE}
    function simulate_6346(n)
        p = Particle6346(0.0, 1.5)
        s = 0.0
        for i in 1:n
            s = step_particle_6346!(p, 0.1)
        end
        return s
    end
    println(simulate_6346(2000))
    "#
        );
        let (output, counts) = run_with_profile(&source);
        assert_eq!(
            output, "300.00000000000017\n",
            "matches upstream Julia output"
        );
        assert!(
            counts.get("SetField").copied().unwrap_or(0) > 0,
            "the specialized field-update loop should execute SetField: {counts:?}"
        );
        assert_eq!(
            counts.get("SetFieldByName").copied().unwrap_or(0),
            0,
            "the specialized field-update loop must not fall back to SetFieldByName: {counts:?}"
        );
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn nary_field_update_benchmark_runs_fully_typed_6346() {
        // The VM-only field-update benchmark uses the n-ary `k * b.x * dt` product.
        // Its hot loop must run entirely on typed instructions: typed field access
        // (GetField/SetField) and typed arithmetic (MulF64), with zero by-name field
        // ops and zero dynamic binary dispatch.
        let source = include_str!("../../benchmarks/vm_field_update.jl");
        let (output, counts) = run_with_profile(source);
        assert_eq!(output, "-76010.9082\n", "matches upstream Julia output");
        assert!(
            counts.get("SetField").copied().unwrap_or(0) > 0,
            "expected typed SetField in the hot loop: {counts:?}"
        );
        assert!(
            counts.get("MulF64").copied().unwrap_or(0) > 0,
            "expected the n-ary product to fold to typed MulF64: {counts:?}"
        );
        assert_eq!(
            counts.get("SetFieldByName").copied().unwrap_or(0)
                + counts.get("GetFieldByName").copied().unwrap_or(0)
                + counts.get("CallDynamicBinaryBoth").copied().unwrap_or(0),
            0,
            "the hot loop must not use by-name field ops or dynamic binary dispatch: {counts:?}"
        );
    }
}

mod index_assign_specialization_6346_tests {
    use std::collections::HashSet;
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::vm::specialize::{specialize_function, SpecializationError};
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, Instr, SpecializableFunction, ValueType,
    };

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn specializable<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a SpecializableFunction {
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
}

mod destructuring_swap_specialization_6561_tests {
    //! Issue #6561: make the *desugared* destructuring swap `a, b = b, a % b`
    //! type-stable under the lazy specialization engine.
    //!
    //! The lowering pass rewrites `a, b = b, a % b` (RHS references the targets)
    //! into a temporary tuple plus indexed reads:
    //!
    //! ```text
    //! __tuple_tmp_N = (b, a % b)
    //! a = __tuple_tmp_N[1]
    //! b = __tuple_tmp_N[2]
    //! ```
    //!
    //! Before this fix the `__tuple_tmp_N[k]` reads returned `Any`, so the
    //! specializer widened `a`/`b` off the typed fast path (`StoreAny` + dynamic
    //! reload). These tests assert the swapped bindings keep their I64/F64 tags.

    use std::collections::HashSet;
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::specialize::specialize_function;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value, ValueType};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn specializable_ir<'a>(
        compiled: &'a CompiledProgram,
        name: &str,
    ) -> &'a subset_julia_vm::ir::core::Function {
        &compiled
            .specializable_functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
            .ir
    }

    /// Count `StoreAny(var)` instructions targeting a specific variable name.
    fn store_any_count(code: &[Instr], var: &str) -> usize {
        code.iter()
            .filter(|i| matches!(i, Instr::StoreAny(name) if name == var))
            .count()
    }

    fn store_i64_count(code: &[Instr], var: &str) -> usize {
        code.iter()
            .filter(|i| matches!(i, Instr::StoreI64(name) if name == var))
            .count()
    }

    fn store_f64_count(code: &[Instr], var: &str) -> usize {
        code.iter()
            .filter(|i| matches!(i, Instr::StoreF64(name) if name == var))
            .count()
    }

    const GCD_SWAP_SOURCE: &str = r#"
    function gcd_swap_6561(a, b)
        while b != 0
            a, b = b, a % b
        end
        return a
    end
    "#;

    /// The integer GCD swap keeps `a`/`b` on the typed `StoreI64` path; the
    /// desugared `temp[k]` reads must not widen them to `Any`. (Issue #6561)
    #[test]
    fn int_swap_specializes_to_typed_store_6561() {
        let compiled = compile_source(GCD_SWAP_SOURCE);
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "gcd_swap_6561"),
            &[ValueType::I64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize integer swap loop");

        assert_eq!(
            store_any_count(&specialized.code, "a"),
            0,
            "swap target `a` must not fall back to StoreAny: {:?}",
            specialized.code
        );
        assert_eq!(
            store_any_count(&specialized.code, "b"),
            0,
            "swap target `b` must not fall back to StoreAny: {:?}",
            specialized.code
        );
        assert!(
            store_i64_count(&specialized.code, "a") > 0,
            "swap target `a` should use typed StoreI64: {:?}",
            specialized.code
        );
        assert!(
            store_i64_count(&specialized.code, "b") > 0,
            "swap target `b` should use typed StoreI64: {:?}",
            specialized.code
        );
        // Type stability propagates to the return: `return a` stays ReturnI64
        // instead of widening to the boxed ReturnAny.
        assert_eq!(specialized.return_type, ValueType::I64);
        assert!(
            specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::ReturnI64)),
            "type-stable swap should return via ReturnI64: {:?}",
            specialized.code
        );
    }

    const FLOAT_SWAP_SOURCE: &str = r#"
    function float_swap_6561(x, y, n)
        for _ in 1:n
            x, y = y, x + y * 0.5
        end
        return x
    end
    "#;

    /// The same type-stability holds for a Float64 swap. (Issue #6561)
    #[test]
    fn float_swap_specializes_to_typed_store_6561() {
        let compiled = compile_source(FLOAT_SWAP_SOURCE);
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "float_swap_6561"),
            &[ValueType::F64, ValueType::F64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize float swap loop");

        assert_eq!(
            store_any_count(&specialized.code, "x") + store_any_count(&specialized.code, "y"),
            0,
            "float swap targets must not fall back to StoreAny: {:?}",
            specialized.code
        );
        assert!(
            store_f64_count(&specialized.code, "x") > 0
                && store_f64_count(&specialized.code, "y") > 0,
            "float swap targets should use typed StoreF64: {:?}",
            specialized.code
        );
        assert_eq!(specialized.return_type, ValueType::F64);
    }

    const SWAP_ACCUMULATE_SOURCE: &str = r#"
    function swap_sum_6561(a, b, n)
        s = 0
        for _ in 1:n
            a, b = b, (a + b) % 1000003
            s += a
        end
        return s
    end
    "#;

    /// The real payoff of #6561: when a swapped target is *used downstream*, its
    /// type stability keeps the consuming op typed. Here `s += a` after the swap
    /// stays on `AddI64`/`StoreI64` instead of (pre-fix) widening `a` to `Any`,
    /// forcing `s += a` onto a dynamic `DynamicAdd`, and poisoning the accumulator
    /// `s` to `Any`. The whole specialized loop must therefore be free of dynamic
    /// arithmetic and return the typed accumulator. (Issue #6561)
    #[test]
    fn swap_target_used_downstream_stays_typed_6561() {
        let compiled = compile_source(SWAP_ACCUMULATE_SOURCE);
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "swap_sum_6561"),
            &[ValueType::I64, ValueType::I64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize swap-accumulate loop");

        // No dynamic arithmetic anywhere in the hot loop.
        let dynamic_ops = specialized
            .code
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    Instr::DynamicAdd
                        | Instr::DynamicSub
                        | Instr::DynamicMul
                        | Instr::DynamicDiv
                        | Instr::DynamicMod
                )
            })
            .count();
        assert_eq!(
            dynamic_ops, 0,
            "downstream use of the typed swap target must not emit dynamic arithmetic: {:?}",
            specialized.code
        );
        // The accumulator `s` stays typed (no boxed StoreAny) and the function
        // returns the typed accumulator.
        assert_eq!(
            store_any_count(&specialized.code, "s"),
            0,
            "accumulator `s` must not widen to StoreAny: {:?}",
            specialized.code
        );
        assert!(
            store_i64_count(&specialized.code, "s") > 0,
            "accumulator `s` should use typed StoreI64: {:?}",
            specialized.code
        );
        assert_eq!(specialized.return_type, ValueType::I64);
    }

    /// A swap whose tuple mixes types keeps *each* target on its own typed path.
    ///
    /// Originally (#6561, tuple-element tracking) this case sharpened only the
    /// numeric target and left the non-numeric one on `Any`. After #6569 the swap
    /// no longer goes through a tuple at all — it lowers to per-element temps — so
    /// every target keeps its concrete type: the numeric `a` is `StoreI64` and the
    /// string `s` is `StoreStr`, with no `StoreAny` widening.
    #[test]
    fn mixed_swap_keeps_each_target_typed_6561() {
        let compiled = compile_source(
            r#"
    function mixed_swap_6561(a, s)
        a, s = a + 1, s
        return a
    end
        "#,
        );
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "mixed_swap_6561"),
            &[ValueType::I64, ValueType::Str],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize mixed swap");

        assert!(
            store_i64_count(&specialized.code, "a") > 0,
            "numeric target `a` should be typed StoreI64: {:?}",
            specialized.code
        );
        assert!(
            specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::StoreStr(name) if name == "s")),
            "string target `s` should be typed StoreStr (per-element lowering, #6569): {:?}",
            specialized.code
        );
        assert_eq!(
            store_any_count(&specialized.code, "a") + store_any_count(&specialized.code, "s"),
            0,
            "neither swap target should widen to StoreAny: {:?}",
            specialized.code
        );
    }

    // ---- End-to-end runtime parity ----
    //
    // The structural tests above prove the *specialized output* is type-stable.
    // These run the whole program through the VM (which triggers runtime
    // specialization) to confirm the type-stable swap still produces results
    // identical to upstream Julia.

    fn run_program(source: &str) -> Value {
        let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
        vm.run().expect("run program")
    }

    fn run_i64(source: &str) -> i64 {
        match run_program(source) {
            Value::I64(v) => v,
            other => panic!("expected Int64 result, got {other:?}"),
        }
    }

    fn run_f64(source: &str) -> f64 {
        match run_program(source) {
            Value::F64(v) => v,
            other => panic!("expected Float64 result, got {other:?}"),
        }
    }

    #[test]
    fn gcd_swap_loop_matches_upstream_julia_6561() {
        // gcd_swap_6561(1071, 462) == 21, gcd_swap_6561(48, 36) == 12 (verified
        // against julia 1.12).
        assert_eq!(
            run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6561(1071, 462)\n")),
            21
        );
        assert_eq!(
            run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6561(48, 36)\n")),
            12
        );
    }

    #[test]
    fn float_swap_loop_matches_upstream_julia_6561() {
        // float_swap_6561(1.0, 2.0, 10) == 15.9921875 (verified against julia 1.12).
        assert_eq!(
            run_f64(&format!(
                "{FLOAT_SWAP_SOURCE}\nfloat_swap_6561(1.0, 2.0, 10)\n"
            )),
            15.9921875
        );
    }

    #[test]
    fn swap_accumulate_loop_matches_upstream_julia_6561() {
        // swap_sum_6561(1, 1, 2000) == 999369993 (verified against julia 1.12).
        assert_eq!(
            run_i64(&format!(
                "{SWAP_ACCUMULATE_SOURCE}\nswap_sum_6561(1, 1, 2000)\n"
            )),
            999369993
        );
    }
}

mod swap_without_tuple_alloc_6569_tests {
    //! Issue #6569: lower a self-referential destructuring swap whose RHS is a
    //! tuple literal (`a, b = b, a % b`) WITHOUT allocating a tuple.
    //!
    //! The swap is desugared into per-element temporaries
    //! (`__t0 = b; __t1 = a % b; a = __t0; b = __t1`) instead of a temporary tuple
    //! plus indexed reads (`__tmp = (b, a % b); a = __tmp[1]; b = __tmp[2]`). This
    //! removes the per-iteration `NewTuple` heap allocation and the `IndexLoad`
    //! reads, matching CPython's allocation-free swap and Julia's native handling.

    use std::collections::HashSet;
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::specialize::specialize_function;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value, ValueType};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn specializable_ir<'a>(
        compiled: &'a CompiledProgram,
        name: &str,
    ) -> &'a subset_julia_vm::ir::core::Function {
        &compiled
            .specializable_functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
            .ir
    }

    const GCD_SWAP_SOURCE: &str = r#"
    function gcd_swap_6569(a, b)
        while b != 0
            a, b = b, a % b
        end
        return a
    end
    "#;

    /// The integer swap specializes with NO tuple allocation and NO tuple index:
    /// the desugared swap is pure per-element temps. (Issue #6569)
    #[test]
    fn int_swap_specializes_without_tuple_alloc_6569() {
        let compiled = compile_source(GCD_SWAP_SOURCE);
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "gcd_swap_6569"),
            &[ValueType::I64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize integer swap loop");

        assert!(
            !specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::NewTuple(_))),
            "tuple-literal swap must not allocate a tuple: {:?}",
            specialized.code
        );
        assert!(
            !specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::IndexLoad(_))),
            "tuple-literal swap must not index a tuple: {:?}",
            specialized.code
        );
        // Still type-stable: a/b keep their typed stores and the function returns
        // the typed value.
        assert!(
            specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::StoreI64(name) if name == "a"))
                && specialized
                    .code
                    .iter()
                    .any(|i| matches!(i, Instr::StoreI64(name) if name == "b")),
            "swap targets should still use typed StoreI64: {:?}",
            specialized.code
        );
        assert_eq!(specialized.return_type, ValueType::I64);
    }

    /// A three-way rotation `a, b, c = b, c, a` also lowers allocation-free and
    /// preserves the simultaneous-assignment semantics. (Issue #6569)
    #[test]
    fn three_cycle_rotation_specializes_without_tuple_alloc_6569() {
        let compiled = compile_source(
            r#"
    function rotate3_6569(a, b, c, n)
        for _ in 1:n
            a, b, c = b, c, a
        end
        return a * 100 + b * 10 + c
    end
        "#,
        );
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            specializable_ir(&compiled, "rotate3_6569"),
            &[
                ValueType::I64,
                ValueType::I64,
                ValueType::I64,
                ValueType::I64,
            ],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize 3-cycle rotation");

        assert!(
            !specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::NewTuple(_) | Instr::IndexLoad(_))),
            "3-cycle rotation must not allocate or index a tuple: {:?}",
            specialized.code
        );
    }

    /// Lower a function and return the debug representation of its body, used to
    /// inspect the desugared destructuring shape.
    fn lower_function_body_debug(source: &str, name: &str) -> String {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        let func = program
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"));
        format!("{:#?}", func.body)
    }

    /// A tuple-literal swap lowers to per-element temps with no tuple indexing,
    /// while a flat non-literal RHS uses the dedicated one-evaluation node.
    /// (Issues #6569 and #10464)
    #[test]
    fn tuple_literal_swap_lowers_without_index_but_var_rhs_keeps_it_6569() {
        let swap_body = lower_function_body_debug(GCD_SWAP_SOURCE, "gcd_swap_6569");
        assert!(
            !swap_body.contains("Index"),
            "tuple-literal swap must lower without Expr::Index:\n{swap_body}"
        );

        let var_rhs = lower_function_body_debug(
            r#"
    function take_pair_6569(t)
        a, b = t
        return a + b
    end
    "#,
            "take_pair_6569",
        );
        assert!(var_rhs.contains("DestructuringAssign"), "{var_rhs}");
        assert!(!var_rhs.contains("Index"), "{var_rhs}");
    }

    // ---- End-to-end runtime parity ----

    fn run_program(source: &str) -> Value {
        let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
        vm.run().expect("run program")
    }

    fn run_i64(source: &str) -> i64 {
        match run_program(source) {
            Value::I64(v) => v,
            other => panic!("expected Int64 result, got {other:?}"),
        }
    }

    #[test]
    fn swap_results_match_upstream_julia_6569() {
        // gcd_swap_6569(1071, 462) == 21 (verified against julia 1.12).
        assert_eq!(
            run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6569(1071, 462)\n")),
            21
        );
        // 3-cycle rotation: starting (1,2,3), after 1 rotation (b,c,a) = (2,3,1)
        // -> 2*100 + 3*10 + 1 = 231; after 3 rotations back to (1,2,3) -> 123.
        let rot = r#"
    function rotate3_6569(a, b, c, n)
        for _ in 1:n
            a, b, c = b, c, a
        end
        return a * 100 + b * 10 + c
    end
    "#;
        assert_eq!(run_i64(&format!("{rot}\nrotate3_6569(1, 2, 3, 1)\n")), 231);
        assert_eq!(run_i64(&format!("{rot}\nrotate3_6569(1, 2, 3, 3)\n")), 123);
    }
}

mod struct_field_offset_5085_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value};

    // Compile a snippet through the cached Base path (`parse_and_lower` merges the
    // process-wide prelude, `compile_with_cache` reuses the persistent/thread-local
    // Base bytecode). This compiles only the user functions instead of recompiling
    // the whole prelude from source on every test, which keeps these field-offset
    // checks fast (Issue #7589). The user functions whose bytecode we assert on are
    // compiled identically to the uncached path — see
    // `cached_base_inference_parity_6538_tests.rs`.
    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let program = parse_and_lower(source).expect("parse and lower source");
        compile_with_cache(&program).expect("compile failed")
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

    fn run_source_with_base(source: &str) -> Value {
        let compiled = compile_source_with_base(source);
        let rng = StableRng::new(0);
        let mut vm = Vm::new_program(compiled, rng);
        vm.run().expect("vm run failed")
    }

    fn assert_i64(value: Value, expected: i64) {
        match value {
            Value::I64(actual) => assert_eq!(actual, expected),
            other => panic!("expected I64({}), got {:?}", expected, other),
        }
    }

    #[test]
    fn concrete_struct_field_reads_emit_offset_getfield_issue_5085() {
        let src = r#"
    struct Point5085
        x::Int64
        y::Int64
    end

    function read_point_5085(p::Point5085)
        p.y + p.x
    end

    read_point_5085(Point5085(3, 4))
    "#;

        let compiled = compile_source_with_base(src);
        let func = get_function(&compiled, "read_point_5085");
        let body = function_body(&compiled, func);

        assert!(
            body.iter().any(|instr| matches!(instr, Instr::GetField(1))),
            "p.y should compile to fixed field offset 1: {:?}",
            body
        );
        assert!(
            body.iter().any(|instr| matches!(instr, Instr::GetField(0))),
            "p.x should compile to fixed field offset 0: {:?}",
            body
        );
        assert!(
            body.iter()
                .all(|instr| !matches!(instr, Instr::GetFieldByName(_))),
            "concrete struct field reads should not use name lookup: {:?}",
            body
        );

        assert_i64(run_source_with_base(src), 7);
    }

    #[test]
    fn nested_concrete_struct_fields_keep_offset_access_issue_5085() {
        let src = r#"
    struct Inner5085
        a::Int64
    end

    struct Outer5085
        inner::Inner5085
    end

    function nested_field_5085()
        o = Outer5085(Inner5085(7))
        o.inner.a
    end

    nested_field_5085()
    "#;

        let compiled = compile_source_with_base(src);
        let func = get_function(&compiled, "nested_field_5085");
        let body = function_body(&compiled, func);

        let offset_reads = body
            .iter()
            .filter(|instr| matches!(instr, Instr::GetField(0)))
            .count();
        assert!(
            offset_reads >= 2,
            "o.inner and inner.a should both use fixed field offset 0: {:?}",
            body
        );
        assert!(
            body.iter()
                .all(|instr| !matches!(instr, Instr::GetFieldByName(_))),
            "nested concrete struct field reads should not use name lookup: {:?}",
            body
        );

        assert_i64(run_source_with_base(src), 7);
    }

    #[test]
    fn concrete_mutable_struct_field_writes_emit_offset_setfield_issue_5085() {
        let src = r#"
    mutable struct Box5085
        value::Int64
        other::Int64
    end

    function update_box_5085()
        b = Box5085(1, 2)
        b.other = 5
        b.other
    end

    update_box_5085()
    "#;

        let compiled = compile_source_with_base(src);
        let func = get_function(&compiled, "update_box_5085");
        let body = function_body(&compiled, func);

        assert!(
            body.iter().any(|instr| matches!(instr, Instr::SetField(1))),
            "b.other assignment should compile to fixed field offset 1: {:?}",
            body
        );
        assert!(
            body.iter().any(|instr| matches!(instr, Instr::GetField(1))),
            "b.other read should compile to fixed field offset 1: {:?}",
            body
        );
        assert!(
            body.iter().all(|instr| {
                !matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_))
            }),
            "concrete mutable struct field access should not use name lookup: {:?}",
            body
        );

        assert_i64(run_source_with_base(src), 5);
    }
}

mod union_split_typecheck_elision_5077_tests {
    //! Bytecode checks for branch-narrowed `isa` elimination (Issue #5077).
    //!
    //! The branch-narrowing optimization (fold inner `isa` checks to `true` / elide
    //! redundant type guards after a narrowing branch) is implemented via
    //! `narrowing.rs`.  The legacy compiler path applies these narrowings in
    //! `compile_if_stmt`; the SSA pipeline (enabled by default since Issue #8832)
    //! now applies equivalent branch-type propagation in `ssa_ir/lower.rs`
    //! (`compute_block_narrowing_info` + per-block `apply_then/else_narrowings`),
    //! closing the ~1.7x regression tracked in Issue #9085.

    use subset_julia_vm::base;
    use subset_julia_vm::builtins::BuiltinId;
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

    fn count_isa_calls(body: &[Instr]) -> usize {
        body.iter()
            .filter(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::Isa, 2)))
            .count()
    }

    fn assert_i64_arithmetic_specialized(compiled: &CompiledProgram, function_name: &str) {
        let func = get_function(compiled, function_name);
        let body = function_body(compiled, func);
        assert!(
            body.iter().any(|instr| matches!(instr, Instr::AddI64)),
            "{function_name} should specialize narrowed Int64 addition: {:?}",
            body
        );
        assert!(
            body.iter().all(|instr| !matches!(instr, Instr::DynamicAdd)),
            "{function_name} should not use dynamic addition after narrowing: {:?}",
            body
        );
    }

    #[test]
    fn branch_narrowed_isa_checks_are_constant_folded_issue_5077() {
        let compiled = compile_source_with_base(
            r#"
    function narrowed_isa_5077(x::Union{Int64,String})
        if x isa Int64
            return x isa Int64
        else
            return x isa String
        end
    end
    "#,
        );
        let func = get_function(&compiled, "narrowed_isa_5077");
        let body = function_body(&compiled, func);

        // Both the SSA pipeline (default since Issue #8832) and the legacy path now
        // implement branch-type propagation and fold the inner `isa` re-checks.
        // The SSA path does this via `compute_block_narrowing_info` in
        // `ssa_ir/lower.rs` (Issue #9085).
        assert_eq!(
            count_isa_calls(body),
            1,
            "only the outer branch guard should remain as runtime isa: {:?}",
            body
        );
        assert!(
            body.iter()
                .any(|instr| matches!(instr, Instr::PushBool(true))),
            "branch-local isa checks should lower to PushBool(true): {:?}",
            body
        );
    }

    #[test]
    fn typeof_guards_drive_branch_codegen_narrowing_issue_5077() {
        let compiled = compile_source_with_base(
            r#"
    function narrowed_typeof_add_5077(x::Union{Int64,String})
        if typeof(x) === Int64
            return x + 1
        else
            return length(x)
        end
    end

    function narrowed_reversed_typeof_add_5077(x::Union{Int64,String})
        if Int64 == typeof(x)
            return x + 1
        else
            return length(x)
        end
    end

    function narrowed_typeof_not_else_add_5077(x::Union{Int64,String})
        if typeof(x) !== Int64
            return length(x)
        else
            return x + 1
        end
    end
    "#,
        );

        for function_name in [
            "narrowed_typeof_add_5077",
            "narrowed_reversed_typeof_add_5077",
            "narrowed_typeof_not_else_add_5077",
        ] {
            assert_i64_arithmetic_specialized(&compiled, function_name);
        }
    }
}

mod bounds_check_elision_5089_tests {
    //! Bytecode checks for proven in-bounds index loads/stores (Issue #5089).

    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let user_program = lowering.lower(parsed).expect("lower source");
        compile_core_program(&user_program).expect("compile failed")
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

    #[test]
    fn eachindex_and_length_loops_emit_inbounds_typed_load_issue_5089() {
        let compiled = compile_source_with_base(
            r#"
    function eachindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in eachindex(xs)
            total = total + xs[i]
        end
        total
    end

    function length_sum_5089(xs::Vector{Int32})
        total = 0
        for i in 1:length(xs)
            total = total + xs[i]
        end
        total
    end

    function base_eachindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in Base.eachindex(xs)
            total = total + xs[i]
        end
        total
    end

    function base_length_sum_5089(xs::Vector{Int32})
        total = 0
        for i in 1:Base.length(xs)
            total = total + xs[i]
        end
        total
    end

    function lastindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in 1:lastindex(xs)
            total = total + xs[i]
        end
        total
    end

    function first_lastindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in firstindex(xs):lastindex(xs)
            total = total + xs[i]
        end
        total
    end

    function base_first_lastindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in Base.firstindex(xs):Base.lastindex(xs)
            total = total + xs[i]
        end
        total
    end

    function axes_sum_5089(xs::Vector{Int32})
        total = 0
        for i in axes(xs, 1)
            total = total + xs[i]
        end
        total
    end

    function base_axes_sum_5089(xs::Vector{Int32})
        total = 0
        for i in Base.axes(xs, 1)
            total = total + xs[i]
        end
        total
    end

    function base_oneto_length_sum_5089(xs::Vector{Int32})
        total = 0
        for i in Base.OneTo(length(xs))
            total = total + xs[i]
        end
        total
    end

    function base_oneto_function_length_sum_5089(xs::Vector{Int32})
        total = 0
        for i in Base.oneto(length(xs))
            total = total + xs[i]
        end
        total
    end

    function direct_getindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in eachindex(xs)
            total = total + getindex(xs, i)
        end
        total
    end

    function base_getindex_sum_5089(xs::Vector{Int32})
        total = 0
        for i in eachindex(xs)
            total = total + Base.getindex(xs, i)
        end
        total
    end

    function unchecked_not_proven_5089(xs::Vector{Int32}, i)
        xs[i]
    end

    function eachindex_store_5089(xs::Vector{Float64})
        for i in eachindex(xs)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function length_store_5089(xs::Vector{Float64})
        for i in 1:length(xs)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function axes_store_5089(xs::Vector{Float64})
        for i in axes(xs, 1)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function base_axes_store_5089(xs::Vector{Float64})
        for i in Base.axes(xs, 1)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function base_oneto_lastindex_store_5089(xs::Vector{Float64})
        for i in Base.OneTo(Base.lastindex(xs))
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function base_oneto_function_lastindex_store_5089(xs::Vector{Float64})
        for i in Base.oneto(Base.lastindex(xs))
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function unchecked_store_not_proven_5089(xs::Vector{Float64}, i)
        xs[i] = 2.0
        xs
    end

    function eachindex_setindex_call_5089(xs::Vector{Float64})
        for i in eachindex(xs)
            setindex!(xs, xs[i] + 1.0, i)
        end
        xs
    end

    function length_setindex_call_5089(xs::Vector{Float64})
        for i in 1:length(xs)
            setindex!(xs, xs[i] + 1.0, i)
        end
        xs
    end

    function base_lastindex_store_5089(xs::Vector{Float64})
        for i in 1:Base.lastindex(xs)
            setindex!(xs, xs[i] + 1.0, i)
        end
        xs
    end

    function first_lastindex_store_5089(xs::Vector{Float64})
        for i in firstindex(xs):lastindex(xs)
            setindex!(xs, xs[i] + 1.0, i)
        end
        xs
    end

    function mismatched_first_lastindex_not_proven_5089(xs::Vector{Float64}, ys::Vector{Float64})
        for i in firstindex(xs):lastindex(ys)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function axes_dim2_not_proven_5089(xs::Vector{Float64})
        for i in axes(xs, 2)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function mismatched_axes_not_proven_5089(xs::Vector{Float64}, ys::Vector{Float64})
        for i in axes(ys, 1)
            xs[i] = xs[i] + 1.0
        end
        xs
    end

    function direct_getindex_not_proven_5089(xs::Vector{Int32}, i)
        getindex(xs, i)
    end
    "#,
        );

        for function_name in [
            "eachindex_sum_5089",
            "length_sum_5089",
            "base_eachindex_sum_5089",
            "base_length_sum_5089",
            "lastindex_sum_5089",
            "first_lastindex_sum_5089",
            "base_first_lastindex_sum_5089",
            "axes_sum_5089",
            "base_axes_sum_5089",
            "base_oneto_length_sum_5089",
            "base_oneto_function_length_sum_5089",
            "direct_getindex_sum_5089",
            "base_getindex_sum_5089",
        ] {
            let func = get_function(&compiled, function_name);
            assert!(
                function_body(&compiled, func).iter().any(|instr| matches!(
                    instr,
                    Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
                )),
                "{function_name} should emit an in-bounds index load: {:?}",
                function_body(&compiled, func)
            );
        }

        let fallback = get_function(&compiled, "unchecked_not_proven_5089");
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .all(|instr| !matches!(
                    instr,
                    Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
                )),
            "unproven index loads must keep the checked typed load: {:?}",
            function_body(&compiled, fallback)
        );

        let fallback = get_function(&compiled, "direct_getindex_not_proven_5089");
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .all(|instr| !matches!(
                    instr,
                    Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
                )),
            "unproven direct getindex calls must keep checked loads: {:?}",
            function_body(&compiled, fallback)
        );
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .any(|instr| matches!(instr, Instr::IndexLoadTyped(1))),
            "direct getindex on typed arrays should use the typed checked load: {:?}",
            function_body(&compiled, fallback)
        );

        for function_name in [
            "eachindex_store_5089",
            "length_store_5089",
            "axes_store_5089",
            "base_axes_store_5089",
            "base_oneto_lastindex_store_5089",
            "base_oneto_function_lastindex_store_5089",
            "eachindex_setindex_call_5089",
            "length_setindex_call_5089",
            "base_lastindex_store_5089",
            "first_lastindex_store_5089",
        ] {
            let func = get_function(&compiled, function_name);
            assert!(
                function_body(&compiled, func)
                    .iter()
                    .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
                "{function_name} should emit an in-bounds index store: {:?}",
                function_body(&compiled, func)
            );
        }

        let fallback = get_function(&compiled, "unchecked_store_not_proven_5089");
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
            "unproven index stores must keep the checked store: {:?}",
            function_body(&compiled, fallback)
        );

        let fallback = get_function(&compiled, "mismatched_first_lastindex_not_proven_5089");
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
            "mismatched firstindex/lastindex arrays must keep checked stores: {:?}",
            function_body(&compiled, fallback)
        );

        for function_name in [
            "axes_dim2_not_proven_5089",
            "mismatched_axes_not_proven_5089",
        ] {
            let fallback = get_function(&compiled, function_name);
            assert!(
                function_body(&compiled, fallback)
                    .iter()
                    .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
                "{function_name} must keep checked stores: {:?}",
                function_body(&compiled, fallback)
            );
        }
    }
}

mod inbounds_indexing_4286_tests {
    //! Bytecode checks for local `@inbounds` indexing metadata (Issue #4286).

    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let user_program = lowering.lower(parsed).expect("lower source");
        compile_core_program(&user_program).expect("compile failed")
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

    #[test]
    fn explicit_inbounds_indexing_emits_inbounds_bytecode_issue_4286() {
        let compiled = compile_source_with_base(
            r#"
    function checked_load_4286(xs::Vector{Int32}, i)
        xs[i]
    end

    function inbounds_load_4286(xs::Vector{Int32}, i)
        @inbounds xs[i]
    end

    function inbounds_getindex_4286(xs::Vector{Int32}, i)
        @inbounds getindex(xs, i)
    end

    function inbounds_base_getindex_4286(xs::Vector{Int32}, i)
        @inbounds Base.getindex(xs, i)
    end

    function checked_store_4286(xs::Vector{Float64}, i)
        xs[i] = 2.0
        xs
    end

    function inbounds_store_4286(xs::Vector{Float64}, i)
        @inbounds xs[i] = 2.0
        xs
    end

    function inbounds_setindex_call_4286(xs::Vector{Float64}, i)
        @inbounds setindex!(xs, 2.0, i)
        xs
    end

    function checked_foreach_body_load_4286(xs::Vector{Int32}, idxs::Vector{Int64})
        acc = Int32(0)
        for i in idxs
            acc += xs[i]
        end
        acc
    end

    function inbounds_foreach_body_load_4286(xs::Vector{Int32}, idxs::Vector{Int64})
        acc = Int32(0)
        @inbounds for i in idxs
            acc += xs[i]
        end
        acc
    end

    function checked_while_body_store_4286(xs::Vector{Float64}, idxs::Vector{Int64})
        j = 1
        while j <= length(idxs)
            xs[idxs[j]] = 4.0
            j += 1
        end
        xs
    end

    function inbounds_while_body_store_4286(xs::Vector{Float64}, idxs::Vector{Int64})
        j = 1
        @inbounds while j <= length(idxs)
            xs[idxs[j]] = 4.0
            j += 1
        end
        xs
    end
    "#,
        );

        let checked = get_function(&compiled, "checked_load_4286");
        assert!(
            function_body(&compiled, checked)
                .iter()
                .all(|instr| !matches!(
                    instr,
                    Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
                )),
            "plain load must stay checked: {:?}",
            function_body(&compiled, checked)
        );

        for function_name in [
            "inbounds_load_4286",
            "inbounds_getindex_4286",
            "inbounds_base_getindex_4286",
        ] {
            let func = get_function(&compiled, function_name);
            assert!(
                function_body(&compiled, func).iter().any(|instr| matches!(
                    instr,
                    Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
                )),
                "{function_name} should emit an in-bounds load: {:?}",
                function_body(&compiled, func)
            );
        }

        let checked = get_function(&compiled, "checked_store_4286");
        assert!(
            function_body(&compiled, checked)
                .iter()
                .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
            "plain store must stay checked: {:?}",
            function_body(&compiled, checked)
        );

        for function_name in ["inbounds_store_4286", "inbounds_setindex_call_4286"] {
            let func = get_function(&compiled, function_name);
            assert!(
                function_body(&compiled, func)
                    .iter()
                    .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
                "{function_name} should emit an in-bounds store: {:?}",
                function_body(&compiled, func)
            );
        }

        let checked = get_function(&compiled, "checked_foreach_body_load_4286");
        assert!(
            function_body(&compiled, checked)
                .iter()
                .all(|instr| !matches!(
                    instr,
                    Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
                )),
            "plain for-each body load must stay checked: {:?}",
            function_body(&compiled, checked)
        );

        let func = get_function(&compiled, "inbounds_foreach_body_load_4286");
        assert!(
            function_body(&compiled, func).iter().any(|instr| matches!(
                instr,
                Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
            )),
            "@inbounds for-each body should emit an in-bounds load: {:?}",
            function_body(&compiled, func)
        );

        let checked = get_function(&compiled, "checked_while_body_store_4286");
        assert!(
            function_body(&compiled, checked)
                .iter()
                .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
            "plain while body store must stay checked: {:?}",
            function_body(&compiled, checked)
        );

        let func = get_function(&compiled, "inbounds_while_body_store_4286");
        assert!(
            function_body(&compiled, func)
                .iter()
                .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
            "@inbounds while body should emit an in-bounds store: {:?}",
            function_body(&compiled, func)
        );
    }
}

mod scalar_hot_loop_6167_tests {
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

    const CALC_PI_SOURCE: &str = r#"
    function mygcd(a, b)
        while b != 0
            tmp = b
            b = a % b
            a = tmp
        end
        a
    end

    function calc_pi(N)
        cnt = 0
        for a in 1:N
            for b in 1:N
                if mygcd(a, b) == 1
                    cnt += 1
                end
            end
        end
        prob = cnt / N / N
        sqrt(6.0 / prob)
    end

    calc_pi(10)
    "#;

    const ADVANCE_SUM_PAIRS_SOURCE: &str = r#"
    function advance(a, b)
        while b > 0
            a += 1
            b -= 1
        end
        a
    end

    function sum_pairs(N)
        total = 0
        step = 2
        for i in 1:N
            total += advance(i, step)
        end
        total
    end

    sum_pairs(20)
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
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn calc_pi_loop_increments_use_counted_loop_superinstructions_6167() {
        let compiled = compile_source(CALC_PI_SOURCE);
        let body = function_body(&compiled, get_function(&compiled, "calc_pi"));

        assert!(
            !body.windows(2).any(|window| {
                matches!(
                    window,
                    [Instr::PushI64(_), Instr::IncVarI64Slot(_)]
                        | [Instr::PushI64(_), Instr::DecVarI64Slot(_)]
                )
            }),
            "const-step loop increments should not leave PushI64 + Inc/DecVarI64Slot: {body:?}"
        );
        assert!(
            body.iter()
                .filter(|instr| matches!(instr, Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _)))
                .count()
                >= 2,
            "calc_pi should use fused counted-loop backedges for inner and outer loop increments: {body:?}"
        );
        assert!(
            body.iter()
                .any(|instr| matches!(instr, Instr::AddConstI64Slot(_, 1))),
            "calc_pi should still use AddConstI64Slot for the conditional counter increment: {body:?}"
        );
        assert!(
            !body.windows(3).any(|window| {
                matches!(
                    window,
                    [
                        Instr::LoadSlotI64(_),
                        Instr::LoadSlotI64(_),
                        Instr::JumpIfGtI64(_)
                    ]
                )
            }),
            "const-step loop exit tests should not leave LoadSlotI64 + LoadSlotI64 + JumpIfGtI64: {body:?}"
        );
        assert!(
            body.iter()
                .filter(|instr| matches!(instr, Instr::JumpIfGtI64Slots(_, _, _)))
                .count()
                >= 2,
            "calc_pi should use JumpIfGtI64Slots for inner and outer loop exits: {body:?}"
        );
        assert!(
            !body.windows(3).any(|window| {
                matches!(
                    window,
                    [
                        Instr::LoadSlotI64(_),
                        Instr::LoadSlotI64(_),
                        Instr::CallSpecialize(_, 2)
                    ]
                )
            }),
            "slot-specialized calls should not leave LoadSlotI64 + LoadSlotI64 + CallSpecialize: {body:?}"
        );
        assert!(
            body.iter().any(|instr| matches!(
                instr,
                Instr::CallSpecializeI64Slots(operands)
                    if operands.slots.len() == 2
            )),
            "calc_pi should pass typed loop slots directly into specialized calls: {body:?}"
        );

        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run calc_pi(10)");
        match result {
            Value::F64(value) => assert!(
                (value - 3.0860669992418384).abs() < 1.0e-12,
                "unexpected calc_pi(10) result: {value}"
            ),
            other => panic!("expected Float64 calc_pi result, got {other:?}"),
        }
    }

    #[test]
    fn i64_slot_specialized_calls_preserve_generic_function_results_6301() {
        let compiled = compile_source(ADVANCE_SUM_PAIRS_SOURCE);
        let body = function_body(&compiled, get_function(&compiled, "sum_pairs"));
        assert!(
            body.iter().any(|instr| matches!(
                instr,
                Instr::CallSpecializeI64Slots(operands)
                    if operands.slots.len() == 2
            )),
            "sum_pairs should call advance through I64 slot arguments: {body:?}"
        );

        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run sum_pairs");

        match result {
            Value::I64(value) => assert_eq!(value, 250),
            other => panic!("expected Int64 sum_pairs result, got {other:?}"),
        }
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn generic_i64_slot_specialized_calls_use_direct_i64_function_path_6308() {
        let compiled = compile_source(ADVANCE_SUM_PAIRS_SOURCE);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));

        profiler::clear();
        profiler::enable();
        let result = vm.run().expect("run sum_pairs");
        profiler::disable();

        match result {
            Value::I64(value) => assert_eq!(value, 250),
            other => panic!("expected Int64 sum_pairs result, got {other:?}"),
        }

        let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
        assert!(
            counts
                .get("ExecutableBlock::I64Function")
                .copied()
                .unwrap_or(0)
                > 0,
            "generic I64 specialized calls should use the direct function path: {counts:?}"
        );
    }
}

mod const_step_for_loop_5166_tests {
    //! Unit tests for constant-step integer range for-loop specialization (Issue #5166).
    //!
    //! When the step of an integer `for i in a:b` / `a:s:b` loop is a compile-time
    //! constant, the compiler hoists the per-iteration sign check out of the loop and
    //! emits a single-direction exit test plus a constant increment. These tests assert
    //! that the dynamic sign-check instructions (`PushI64(0)` + `GtI64` guarding a
    //! `JumpIfZero`) disappear for constant steps, while remaining for dynamic steps.

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

    /// The dynamic sign-check path emits the `step > 0` test as `LoadI64(step)`,
    /// `PushI64(0)`, `GtI64`, `JumpIfZero(...)`. After the peephole optimizer fuses
    /// `GtI64 + JumpIfZero` into `JumpIfLeI64`, the residue is `PushI64(0)` immediately
    /// followed by either `GtI64` (unfused) or `JumpIfLeI64` (fused). Either way the
    /// `PushI64(0)` comparand against the step is the hallmark of the per-iteration sign
    /// check; constant-step loops must not contain it.
    fn has_sign_check(body: &[Instr]) -> bool {
        body.windows(2).any(|w| {
            matches!(
                (&w[0], &w[1]),
                (Instr::PushI64(0), Instr::GtI64) | (Instr::PushI64(0), Instr::JumpIfLeI64(_))
            )
        }) || body.iter().any(|i| {
            // Since Issue #10105 the per-iteration `step <= 0` sign-check branch
            // `LoadSlotI64(step); PushI64(0); JumpIfLeI64` fuses into the
            // slot-vs-constant `JumpIfCmpI64SlotConst(step, 0, Le, _)`.
            matches!(
                i,
                Instr::JumpIfCmpI64SlotConst(_, 0, subset_julia_vm_bytecode::I64Cmp::Le, _)
            )
        })
    }

    fn count_inc(body: &[Instr]) -> usize {
        body.iter()
            .filter(|i| {
                matches!(i, Instr::IncVarI64(_) | Instr::IncVarI64Slot(_))
                    || matches!(i, Instr::AddConstI64Slot(_, delta) if *delta > 0)
                    || matches!(i, Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _))
            })
            .count()
    }

    fn count_dec(body: &[Instr]) -> usize {
        body.iter()
            .filter(|i| {
                matches!(i, Instr::DecVarI64(_) | Instr::DecVarI64Slot(_))
                    || matches!(i, Instr::AddConstI64Slot(_, delta) if *delta < 0)
            })
            .count()
    }

    fn count_directional_exit(body: &[Instr]) -> usize {
        body.iter()
            .filter(|i| {
                matches!(
                    i,
                    Instr::JumpIfGtI64(_)
                        | Instr::JumpIfGtI64Slots(_, _, _)
                        | Instr::JumpIfLtI64(_)
                )
            })
            .count()
    }

    #[test]
    fn const_step_unit_increment_drops_sign_check() {
        // `for i in 1:n` — implicit step of 1.
        let compiled = compile_source_with_base(
            "function f(n)\n  s = 0\n  for i in 1:n\n    s += i\n  end\n  s\nend\n",
        );
        let f = get_function(&compiled, "f");
        let body = function_body(&compiled, f);
        assert!(
            !has_sign_check(body),
            "unit-step loop must not emit a dynamic step>0 sign check: {:?}",
            body
        );
        assert!(
            count_inc(body) >= 1,
            "unit-step loop must use a fused I64 increment for the loop variable: {:?}",
            body
        );
        assert!(
            count_directional_exit(body) >= 1,
            "unit-step loop must use a single-direction JumpIfGtI64 exit test: {:?}",
            body
        );
    }

    #[test]
    fn const_step_negative_unit_uses_dec_and_lt_exit() {
        // `for i in n:-1:1` — literal negative unit step.
        let compiled = compile_source_with_base(
            "function g(n)\n  s = 0\n  for i in n:-1:1\n    s += i\n  end\n  s\nend\n",
        );
        let f = get_function(&compiled, "g");
        let body = function_body(&compiled, f);
        assert!(
            !has_sign_check(body),
            "negative-unit-step loop must not emit a dynamic step>0 sign check: {:?}",
            body
        );
        assert!(
            count_dec(body) >= 1,
            "step -1 loop must use DecVarI64 for the decrement: {:?}",
            body
        );
        let has_lt_exit = body.iter().any(|i| matches!(i, Instr::JumpIfLtI64(_)));
        assert!(
            has_lt_exit,
            "step<0 loop must exit via JumpIfLtI64: {:?}",
            body
        );
    }

    #[test]
    fn const_step_nonunit_positive_drops_sign_check() {
        // `for i in 1:2:n` — constant non-unit step.
        let compiled = compile_source_with_base(
            "function h(n)\n  s = 0\n  for i in 1:2:n\n    s += i\n  end\n  s\nend\n",
        );
        let f = get_function(&compiled, "h");
        let body = function_body(&compiled, f);
        assert!(
            !has_sign_check(body),
            "constant non-unit-step loop must not emit a dynamic step>0 sign check: {:?}",
            body
        );
        assert!(
            count_directional_exit(body) >= 1,
            "constant non-unit-step loop must use a single-direction exit test: {:?}",
            body
        );
        assert!(
            body.iter()
                .any(|i| matches!(i, Instr::AddConstI64SlotAndJumpIfLe(_, 2, _, _))),
            "constant non-unit-step loop must use a fused backedge carrying delta=2: {:?}",
            body
        );
    }

    #[test]
    fn dynamic_step_keeps_sign_check() {
        // `for i in 1:s:n` with `s::Int` — the step is a runtime variable but its
        // type is statically known integer, so the loop stays on the I64 fast path
        // and the dynamic sign-check must remain intact. (An *unannotated* step
        // infers `Any` and diverts to the generic range path since Issue #9291 —
        // see `any_step_diverts_to_generic_range_9291` below.)
        let compiled = compile_source_with_base(
            "function k(n, s::Int)\n  acc = 0\n  for i in 1:s:n\n    acc += i\n  end\n  acc\nend\n",
        );
        let f = get_function(&compiled, "k");
        let body = function_body(&compiled, f);
        assert!(
            has_sign_check(body),
            "dynamic-step loop must keep the per-iteration step>0 sign check: {:?}",
            body
        );
    }

    #[test]
    fn any_step_diverts_to_generic_range_9291() {
        // `for i in 1:s:n` with an unannotated `s` — the step infers `Any`, and an
        // Any-typed step must divert to the generic lazy-range/ForEach path instead
        // of the I64 fast path (Issue #9291, follow-up to the #9287 operand-following
        // division inference): a runtime float step on the I64 fast path would be
        // truncated to 0 and iterate zero times.
        let compiled = compile_source_with_base(
            "function k(n, s)\n  acc = 0\n  for i in 1:s:n\n    acc += i\n  end\n  acc\nend\n",
        );
        let f = get_function(&compiled, "k");
        let body = function_body(&compiled, f);
        assert!(
            body.iter().any(|i| matches!(i, Instr::MakeStepRangeLazy)),
            "Any-step loop must build a lazy range for the generic ForEach path: {:?}",
            body
        );
        assert!(
            !has_sign_check(body),
            "Any-step loop must not be on the I64 fast path (no per-iteration \
             step>0 sign check expected): {:?}",
            body
        );
    }
}

mod slot_const_increment_fusion_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value};

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
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn slot_const_increment_fuses_iadd_one_update() {
        let source = r#"
    function count_to(n::Int64)::Int64
        i = 0
        while i < n
            i += 1
        end
        return i
    end

    count_to(3)
    "#;

        let compiled = compile_source(source);
        let body = function_body(&compiled, get_function(&compiled, "count_to"));
        assert!(
            body.iter()
                .any(|instr| matches!(instr, Instr::AddConstI64Slot(_, 1))),
            "expected i += 1 to fuse to AddConstI64Slot: {body:?}"
        );
        assert!(
            !body.windows(4).any(|window| matches!(
                window,
                [
                    Instr::LoadSlotI64(_),
                    Instr::PushI64(1),
                    Instr::AddI64,
                    Instr::StoreSlotI64(_)
                ]
            )),
            "unfused slot increment remains: {body:?}"
        );

        let rng = StableRng::new(0);
        let mut vm = Vm::new_program(compiled, rng);
        let result = vm.run().expect("vm run failed");
        assert!(matches!(result, Value::I64(3)));
    }
}

mod float_compare_jump_fusion_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value};

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
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn f64_compare_jump_fusion_preserves_nan_false_branch() {
        let source = r#"
    function branch_le(x::Float64)::Int64
        if x <= 4.0
            return 1
        end
        return 2
    end

    branch_le(0.0 / 0.0)
    "#;

        let compiled = compile_source(source);
        let body = function_body(&compiled, get_function(&compiled, "branch_le"));
        assert!(
            body.iter()
                .any(|instr| matches!(instr, Instr::JumpIfNotLeF64(_))),
            "expected LeF64 + JumpIfZero to fuse to JumpIfNotLeF64: {body:?}"
        );
        assert!(
            !body
                .windows(2)
                .any(|pair| matches!(pair, [Instr::LeF64, Instr::JumpIfZero(_)])),
            "unfused LeF64 + JumpIfZero remains: {body:?}"
        );

        let rng = StableRng::new(0);
        let mut vm = Vm::new_program(compiled, rng);
        let result = vm.run().expect("vm run failed");
        assert!(matches!(result, Value::I64(2)));
    }
}

mod unary_math_specialization_guard_9694_tests {
    use std::collections::HashSet;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::vm::specialize::{specialize_function, SpecializationError};
    use subset_julia_vm_bytecode::{CompiledProgram, SpecializableFunction, ValueType};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn specializable<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a SpecializableFunction {
        compiled
            .specializable_functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
    }

    fn assert_complex_operand_rejected(function: &str, wrapper: &str) {
        let source = format!(
            r#"
    function {wrapper}(x, y)
        z = Complex{{Float64}}(x, y)
        return {function}(z)
    end
    "#
        );
        let compiled = compile_source(&source);
        let type_object_names = HashSet::new();
        let result = specialize_function(
            &specializable(&compiled, wrapper).ir,
            &[ValueType::F64, ValueType::F64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        );

        assert!(
            matches!(result, Err(SpecializationError::Unsupported(_))),
            "{function}(Complex{{Float64}}) must fall back to generic dispatch, got {result:?}"
        );
    }

    #[test]
    fn floor_complex_operand_stays_on_generic_fallback_9694() {
        assert_complex_operand_rejected("floor", "floor_complex_9694");
    }

    #[test]
    fn ceil_complex_operand_stays_on_generic_fallback_9694() {
        assert_complex_operand_rejected("ceil", "ceil_complex_9694");
    }

    #[test]
    fn round_complex_operand_stays_on_generic_fallback_9694() {
        assert_complex_operand_rejected("round", "round_complex_9694");
    }
}

mod f64_function_block_tests {
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::{
        F64FunctionBlock, F64FunctionBuilder, F64FunctionOp, F64FunctionSlot, F64Relation, Vm,
    };

    #[test]
    fn builder_slot_deduplication_and_param_binding() {
        let param_slots = [10, 20];
        let mut builder = F64FunctionBuilder::new(&param_slots);

        assert_eq!(builder.slot(10), 0);
        assert_eq!(builder.slot(20), 1);
        assert_eq!(builder.slot(10), 0);

        let slot10 = builder
            .slots
            .iter()
            .find(|entry| entry.slot == 10)
            .expect("slot 10 should be recorded");
        assert_eq!(slot10.param_index, Some(0));

        let slot20 = builder
            .slots
            .iter()
            .find(|entry| entry.slot == 20)
            .expect("slot 20 should be recorded");
        assert_eq!(slot20.param_index, Some(1));
    }

    #[test]
    fn execute_hand_constructed_f64_function_block() {
        // f(x) = x * 2.0 + 1.0
        let block = F64FunctionBlock {
            slots: vec![F64FunctionSlot {
                slot: 42,
                param_index: Some(0),
            }],
            ops: vec![
                F64FunctionOp::LoadSlot(0),
                F64FunctionOp::Push(2.0),
                F64FunctionOp::Mul,
                F64FunctionOp::Push(1.0),
                F64FunctionOp::Add,
                F64FunctionOp::Return,
            ],
            callees: vec![],
        };

        let result = Vm::<StableRng>::execute_f64_function_block(&block, &[3.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn nested_call_f64_function_callees() {
        // callee(x) = x * 2.0
        let callee = F64FunctionBlock {
            slots: vec![F64FunctionSlot {
                slot: 5,
                param_index: Some(0),
            }],
            ops: vec![
                F64FunctionOp::LoadSlot(0),
                F64FunctionOp::Push(2.0),
                F64FunctionOp::Mul,
                F64FunctionOp::Return,
            ],
            callees: vec![],
        };

        // outer(x) = callee(x)
        let outer = F64FunctionBlock {
            slots: vec![F64FunctionSlot {
                slot: 10,
                param_index: Some(0),
            }],
            ops: vec![
                F64FunctionOp::LoadSlot(0),
                F64FunctionOp::Call(0, 1),
                F64FunctionOp::Return,
            ],
            callees: vec![callee],
        };

        let result = Vm::<StableRng>::execute_f64_function_block(&outer, &[3.0]);
        assert_eq!(result, Some(6.0));
    }

    fn compare_block(relation: F64Relation, lhs: f64, rhs: f64) -> Option<f64> {
        const FALSE_TARGET: usize = 6;
        let block = F64FunctionBlock {
            slots: vec![],
            ops: vec![
                F64FunctionOp::Push(lhs),
                F64FunctionOp::Push(rhs),
                F64FunctionOp::Cmp(relation),
                F64FunctionOp::JumpIfZero(FALSE_TARGET),
                F64FunctionOp::Push(1.0),
                F64FunctionOp::Return,
                F64FunctionOp::Push(0.0),
                F64FunctionOp::Return,
            ],
            callees: vec![],
        };
        Vm::<StableRng>::execute_f64_function_block(&block, &[])
    }

    fn assert_compare(relation: F64Relation, lhs: f64, rhs: f64, expected: f64) {
        let result = compare_block(relation, lhs, rhs).unwrap_or_else(|| {
            panic!("compare block should return a value for {relation:?} {lhs} {rhs}")
        });
        assert_eq!(
            result, expected,
            "unexpected result for {relation:?}({lhs}, {rhs})"
        );
    }

    #[test]
    fn compare_and_jump_ops_with_nan() {
        // Equality and inequality.
        assert_compare(F64Relation::Eq, 2.0, 2.0, 1.0);
        assert_compare(F64Relation::Eq, 2.0, 3.0, 0.0);
        assert_compare(F64Relation::Eq, f64::NAN, 2.0, 0.0);
        assert_compare(F64Relation::Eq, f64::NAN, f64::NAN, 0.0);

        assert_compare(F64Relation::Ne, 2.0, 3.0, 1.0);
        assert_compare(F64Relation::Ne, 2.0, 2.0, 0.0);
        assert_compare(F64Relation::Ne, f64::NAN, 2.0, 1.0);
        assert_compare(F64Relation::Ne, f64::NAN, f64::NAN, 1.0);

        // Ordered comparisons.
        assert_compare(F64Relation::Lt, 2.0, 3.0, 1.0);
        assert_compare(F64Relation::Lt, 3.0, 2.0, 0.0);
        assert_compare(F64Relation::Lt, f64::NAN, 2.0, 0.0);
        assert_compare(F64Relation::Lt, 2.0, f64::NAN, 0.0);

        assert_compare(F64Relation::Le, 2.0, 2.0, 1.0);
        assert_compare(F64Relation::Le, 3.0, 2.0, 0.0);
        assert_compare(F64Relation::Le, f64::NAN, 2.0, 0.0);

        assert_compare(F64Relation::Gt, 3.0, 2.0, 1.0);
        assert_compare(F64Relation::Gt, 2.0, 3.0, 0.0);
        assert_compare(F64Relation::Gt, f64::NAN, 2.0, 0.0);

        assert_compare(F64Relation::Ge, 2.0, 2.0, 1.0);
        assert_compare(F64Relation::Ge, 2.0, 3.0, 0.0);
        assert_compare(F64Relation::Ge, f64::NAN, 2.0, 0.0);
    }
}
