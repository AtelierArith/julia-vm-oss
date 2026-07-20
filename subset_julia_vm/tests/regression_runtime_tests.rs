//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod common;

mod array_construction_routing_6649_tests {
    //! Bytecode guards for public Array construction routing (Issue #6649).

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
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    fn is_native_array_carrier_builder(instr: &Instr) -> bool {
        // NOTE: `FinalizeArray`/`FinalizeArrayTyped` are intentionally NOT listed.
        // They were the legacy native-carrier finalize, but Issue #6807 (Slice 4)
        // de-varianted the build buffer onto `Value::Memory`, so they now finalize a
        // `Memory` into the MemoryRef-backed `Array{T,N}` wrapper. Issue #6846 routes
        // public array literals through that native finalize (instead of a
        // per-literal pure-Julia `wrap` call), so a `FinalizeArray` in public
        // construction bytecode is the *wrapper* path, not a native carrier.
        matches!(
            instr,
            Instr::NewArray(_)
                | Instr::PushArrayValue(_)
                | Instr::PushElem
                | Instr::NewArrayTyped(_, _)
                | Instr::PushElemTyped
                | Instr::AllocUndefTyped(_, _)
                | Instr::AllocUndefTypedFromTuple(_)
                | Instr::AllocUndefDynamicTyped(_)
                | Instr::AllocUndefDynamicTypedFromTuple
        )
    }

    #[test]
    fn public_array_literals_do_not_emit_native_array_builders_issue_6649() {
        // Compiling the full Base prelude in-process overflows the default
        // libtest thread stack in debug builds (Issue #10006).
        crate::common::run_with_large_stack(|| {
            let compiled = compile_source_with_base(
                r#"
    function public_array_literal_construction_6649()
        a = [1, 2, 3]
        b = Int64[4, 5]
        c = [i for i in 1:3]
        d = Int64[i for i in 1:3]
        e = Vector{Int64}()
        m = [1 2; 3 4]
        return a[2] + b[1] + c[3] + d[2] + length(e) + m[2, 1]
    end
    "#,
            );
            let function = get_function(&compiled, "public_array_literal_construction_6649");
            let body = function_body(&compiled, function);

            let offenders: Vec<_> = body
                .iter()
                .filter(|instr| is_native_array_carrier_builder(instr))
                .collect();

            assert!(
                offenders.is_empty(),
                "public array literal bytecode must use Memory + Array wrapper construction: {offenders:#?}"
            );

            assert!(
                body.iter()
                    .any(|instr| matches!(instr, Instr::NewMemory(_, _))),
                "expected public array literal bytecode to allocate Memory storage: {body:#?}"
            );
            // Issue #6846: literals finalize the `Memory` into the `Array{T,N}` wrapper
            // natively via `FinalizeArray` instead of a per-literal pure-Julia
            // `wrap(Array, memory, dims)` call (which spun up ~5 Julia frames per
            // literal). The wrapper is still MemoryRef-backed — no native carrier.
            assert!(
                body.iter().any(|instr| matches!(
                    instr,
                    Instr::FinalizeArray(_) | Instr::FinalizeArrayTyped(_)
                )),
                "expected public array literal bytecode to finalize Memory into the Array wrapper natively: {body:#?}"
            );
        });
    }

    #[test]
    fn public_array_materialization_routes_do_not_emit_native_array_carriers_issue_6653() {
        // Compiling the full Base prelude in-process overflows the default
        // libtest thread stack in debug builds (Issue #10006).
        crate::common::run_with_large_stack(|| {
            let compiled = compile_source_with_base(
                r#"
    function public_array_materialization_surface_6653()
        a = [1, 2, 3]
        b = Array{Int64}(undef, 3)
        for i in 1:3
            b[i] = i
        end
        c = Array{Int64}(undef, (2, 2))
        for i in 1:4
            c[i] = i
        end
        d = collect(1:3)
        e = collect((1, 2, 3))
        f = collect(x * 2 for x in a)
        g = [x + 1 for x in a]
        h = map(x -> x + 1, a)
        i = filter(isodd, a)
        j = broadcast(+, a, a)
        k = similar(a)
        l = zeros(Int64, 3)
        m = ones(Int64, 3)
        n = reshape([1, 2, 3, 4], (2, 2))
        return b[2] + c[2, 2] + d[3] + e[1] + f[2] + g[3] +
            h[1] + i[2] + j[3] + length(k) + l[1] + m[2] + n[2, 1]
    end
    "#,
            );
            let function = get_function(&compiled, "public_array_materialization_surface_6653");
            let body = function_body(&compiled, function);

            let offenders: Vec<_> = body
                .iter()
                .filter(|instr| is_native_array_carrier_builder(instr))
                .collect();

            assert!(
                offenders.is_empty(),
                "public array materialization bytecode must not emit native array carrier builders: {offenders:#?}"
            );

            // Issue #6846: literal/comprehension construction finalizes `Memory` into
            // the `Array{T,N}` wrapper natively via `FinalizeArray` (no per-literal
            // pure-Julia `wrap` call); the wrapper stays MemoryRef-backed.
            assert!(
                body.iter().any(|instr| matches!(
                    instr,
                    Instr::FinalizeArray(_) | Instr::FinalizeArrayTyped(_)
                )),
                "expected public array routes to finalize Memory into the Array wrapper natively: {body:#?}"
            );
            assert!(
                body.iter().any(
                    |instr| matches!(instr, Instr::PushFunction(name) if name == "_array_undef_from_dims")
                ),
                "expected Array{{T}}(undef, ...) to route through _array_undef_from_dims: {body:#?}"
            );
        });
    }
}

mod array_element_structref_5234_tests {
    //! Issue #5234: bare stdout `print` / `println` of an array whose ELEMENTS are
    //! heap-allocated structs (Pair, Complex, user struct) must resolve each
    //! `Value::StructRef(idx)` through the element's show form, not leak the Rust
    //! debug `StructRef(heap_idx=N)` repr.
    //!
    //! These tests capture the VM's stdout (`get_output()`) so they exercise the
    //! exact entry points the bug reproduced on — `BuiltinId::Print` /
    //! `BuiltinId::Println` and the `PrintAny` / `PrintAnyNoNewline` instructions —
    //! which the `string` / `repr` fixture path does NOT reach (those go through
    //! `ToString` / `Repr`, which already deep-resolved). The fixture
    //! `io::array_element_structref_5234` covers the string/repr/sprint side; this
    //! Rust test closes the remaining bare-stdout gap that the #4766 matrix
    //! fixtures (which only used `print(buf, x)` / `sprint`) never hit.

    use crate::common::compile_and_run_str_with_output;

    /// No display path may leak the Rust debug `StructRef(...)` / `heap_idx=`
    /// tokens into user-visible output.
    fn assert_no_structref_leak(output: &str, ctx: &str) {
        assert!(
            !output.contains("StructRef") && !output.contains("heap_idx"),
            "{ctx}: leaked StructRef debug repr into stdout output: {output:?}"
        );
    }

    #[test]
    fn println_array_of_pair_literal_no_structref_leak() {
        let output = compile_and_run_str_with_output("println([1 => 1, 2 => 4])\n0\n", 0);
        assert_no_structref_leak(&output, "println([Pair...])");
        assert!(
            output.contains("[1 => 1, 2 => 4]"),
            "expected `[1 => 1, 2 => 4]`, got: {output:?}"
        );
    }

    #[test]
    fn print_array_of_pair_literal_no_structref_leak() {
        let output = compile_and_run_str_with_output("print([1 => 1, 2 => 4])\n0\n", 0);
        assert_no_structref_leak(&output, "print([Pair...])");
        assert!(
            output.contains("[1 => 1, 2 => 4]"),
            "expected `[1 => 1, 2 => 4]`, got: {output:?}"
        );
    }

    #[test]
    fn println_array_of_complex_literal_no_structref_leak() {
        let output =
            compile_and_run_str_with_output("println([complex(1, 1), complex(2, 2)])\n0\n", 0);
        assert_no_structref_leak(&output, "println([Complex...])");
        // Complex integer literal eltype parity is tracked by #9743; this test is
        // scoped to the #5234 stdout StructRef leak.
        let has_integer_complex = output.contains("1 + 1im") && output.contains("2 + 2im");
        let has_f64_complex = output.contains("1.0 + 1.0im") && output.contains("2.0 + 2.0im");
        assert!(
            has_integer_complex || has_f64_complex,
            "expected complex elements without StructRef leakage, got: {output:?}"
        );
    }

    #[test]
    fn println_comprehension_of_complex_no_structref_leak() {
        let output =
            compile_and_run_str_with_output("println([complex(x, x) for x in [1, 2]])\n0\n", 0);
        assert_no_structref_leak(&output, "println([complex comprehension])");
        assert!(
            output.contains("1 + 1im") && output.contains("2 + 2im"),
            "expected complex elements `1 + 1im` / `2 + 2im`, got: {output:?}"
        );
    }

    #[test]
    fn println_array_of_user_struct_no_structref_leak() {
        let src = "\
    struct Foo5234
        x::Int
    end
    println([Foo5234(1), Foo5234(2)])
    0
    ";
        let output = compile_and_run_str_with_output(src, 0);
        assert_no_structref_leak(&output, "println([Foo5234...])");
        assert!(
            output.contains("Foo5234(1)") && output.contains("Foo5234(2)"),
            "expected struct elements `Foo5234(1)` / `Foo5234(2)`, got: {output:?}"
        );
    }

    #[test]
    fn println_comprehension_of_user_struct_no_structref_leak() {
        let src = "\
    struct Bar5234
        x::Int
    end
    println([Bar5234(i) for i in 1:3])
    0
    ";
        let output = compile_and_run_str_with_output(src, 0);
        assert_no_structref_leak(&output, "println([Bar5234 comprehension])");
        assert!(
            output.contains("Bar5234(1)") && output.contains("Bar5234(3)"),
            "expected struct elements `Bar5234(1)` / `Bar5234(3)`, got: {output:?}"
        );
    }

    #[test]
    fn println_matrix_of_pair_no_structref_leak() {
        let output =
            compile_and_run_str_with_output("println([1 => 2 3 => 4; 5 => 6 7 => 8])\n0\n", 0);
        assert_no_structref_leak(&output, "println(matrix of Pair)");
        assert!(
            output.contains("1 => 2") && output.contains("7 => 8"),
            "expected matrix Pair elements, got: {output:?}"
        );
    }
}

mod dict_native_demotion_6621_tests {
    //! Bytecode guards for demoting native Dict carriers to boundary/cache roles.

    use subset_julia_vm::builtins::BuiltinId;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr};

    fn compile_source_with_standard_base(source: &str) -> CompiledProgram {
        // Let the production merge path attach canonical Base ownership metadata;
        // hand-prepending Base loses `base_function_count` provenance (Issue #11445).
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
        // Compiling the full Base prelude in-process overflows the default
        // libtest thread stack in debug builds (Issue #10006).
        crate::common::run_with_large_stack(|| {
            let compiled = compile_source_with_standard_base(
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
        });
    }
}

mod complex_loop_allocation_baseline_9198_tests {
    //! Per-iteration heap-allocation baseline for typed `Complex{Float64}` loops
    //! (Issue #9198, slice 1 — the S2 acceptance baseline).
    //!
    //! Issue #9198 makes "isbits immutable struct unboxing" a register-VM design
    //! requirement. Its acceptance criterion for S2 is:
    //!
    //! > typed loop 内 `Complex{Float64}` スカラー演算がヒープ確保ゼロになる
    //! > (allocation カウンタで検証)
    //!
    //! This module is that allocation counter. sjulia already exposes VM *memory
    //! stats* (`REPLSession::last_vm_memory_stats` → `struct_heap_len`,
    //! dispatch/specialization cache entry counts; see
    //! `session_boundedness_8625_tests.rs`), but those measure **steady-state
    //! resident cache/heap entry counts per eval**, not the number of transient
    //! heap allocations issued *while a loop runs*. A boxed `Complex{Float64}`
    //! (`Value::Struct(StructInstance{ values: Vec<Value> })`) is a transient whose
    //! backing `Vec` is allocated and dropped inside the loop body — it never lands
    //! in `struct_heap` (only mutable / `StructRef` values do), so the existing
    //! stats cannot see it. We therefore install a **test-only counting global
    //! allocator** (justified: no existing counter observes transient per-iteration
    //! allocations) and measure the allocation delta across a windowed `Vm::run()`.
    //!
    //! ## Method (difference across iteration counts)
    //!
    //! For two otherwise-identical programs differing only in loop bound (`N_LO`
    //! vs `N_HI`), every allocation that is *not* proportional to the iteration
    //! count (parse/lower/compile, Base method specialization on first call, VM
    //! setup) is identical and cancels in the difference:
    //!
    //! ```text
    //! allocs_per_iter = (allocs(N_HI) - allocs(N_LO)) / (N_HI - N_LO)
    //! ```
    //!
    //! A scalar-`Float64` control loop (the "real-decomposed" mandelbrot form) is
    //! measured the same way; it is already fully slotized, so its per-iteration
    //! allocation count is ~0 and is the target the boxed-Complex loop must reach.
    //!
    //! ## S2 landed (Issue #9198, slice 2 — `compile::complex_sroa`)
    //!
    //! The Complex{Float64} slot-pair SROA pass now unboxes a proven-ComplexF64 loop
    //! local into two `f64` re/im slots, so the typed `z = z*z + c` loop compiles to
    //! the same fused typed-`f64` slot ops as the real-decomposed control and issues
    //! **zero per-iteration heap allocations from the struct representation**. This
    //! test therefore no longer records a boxed *baseline* (21 : 1) but asserts the
    //! *landed* result: the Complex loop's per-iteration allocation count has dropped
    //! to the control's interpreted-loop floor (both ~1/iter; the residual is the
    //! interpreter's per-iteration bookkeeping, not struct boxing). Numbers are
    //! provisional/local (NS-4); see `docs/vm/REGISTER_VM.md` §"Multi-Slot Scalar
    //! (isbits Immutable Struct) Unboxing".

    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::CompiledProgram;

    /// Counting allocator: forwards every request to the system allocator and, when
    /// the *current thread* has counting enabled, counts each fresh allocation
    /// (`alloc` / `alloc_zeroed`). `realloc` and `dealloc` are not counted — we want
    /// the number of *new* allocation events (each boxed Complex is one
    /// `vec![re, im]` = one `alloc`), not resizes or frees.
    ///
    /// Counting state is **thread-local**, not process-global (Issue #10201).
    /// A process-global `ACTIVE`/`ALLOC_COUNT` pair also counted allocations issued
    /// by concurrently running *sibling* tests: under libtest's default parallel
    /// in-process threads those land inside the measurement window and do not cancel
    /// in the N_LO/N_HI difference, so the allocation-free assertions were flaky
    /// under plain `cargo test` (debug) while passing under `--test-threads=1` and
    /// nextest (process-per-test). The VM is single-threaded by design
    /// (`docs/vm/SINGLE_THREADED_VM.md`), so every allocation of the measured
    /// `Vm::run()` happens on the calling test thread; thread-confined counting
    /// therefore measures exactly what an isolated run would, immune to siblings.
    ///
    /// Both cells are `const`-initialized (no lazy heap init inside the allocator
    /// hooks) and read via `try_with`, so a foreign thread allocating during its own
    /// TLS teardown cannot panic — it simply is not counted.
    struct CountingAllocator;

    thread_local! {
        static THREAD_ALLOC_COUNT: Cell<u64> = const { Cell::new(0) };
        static THREAD_COUNTING: Cell<bool> = const { Cell::new(false) };
    }

    /// Count one allocation event iff the current thread has counting enabled.
    fn count_alloc_event_on_this_thread() {
        let counting = THREAD_COUNTING.try_with(Cell::get).unwrap_or(false);
        if counting {
            let _ = THREAD_ALLOC_COUNT.try_with(|n| n.set(n.get() + 1));
        }
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count_alloc_event_on_this_thread();
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            count_alloc_event_on_this_thread();
            System.alloc_zeroed(layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            System.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    fn compile(src: &str) -> CompiledProgram {
        let mut parser = Parser::new().unwrap();
        let outcome = parser.parse(src).unwrap();
        let mut lowering = Lowering::new(src);
        let program = lowering.lower(outcome).unwrap();
        compile_with_cache(&program).unwrap()
    }

    /// Number of `alloc`/`alloc_zeroed` events issued during a single `Vm::run()`
    /// of `compiled` (setup/compile excluded — only the run window is counted).
    fn allocs_during_run(compiled: &CompiledProgram) -> u64 {
        let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
        THREAD_ALLOC_COUNT.with(|n| n.set(0));
        THREAD_COUNTING.with(|c| c.set(true));
        let result = vm.run();
        THREAD_COUNTING.with(|c| c.set(false));
        result.expect("program must run without error");
        THREAD_ALLOC_COUNT.with(Cell::get)
    }

    const N_LO: usize = 2_000;
    const N_HI: usize = 4_000;

    /// A typed `Complex{Float64}` accumulation loop: `z = z * z + c`, the exact
    /// mandelbrot-inner-loop shape Issue #9198 names. Each iteration runs the Pure
    /// Julia `*(::Complex, ::Complex)` and `+(::Complex, ::Complex)` methods, each
    /// constructing a boxed `Complex{Float64}` result. Loop bound is templated so
    /// two programs differ only in `N`.
    fn complex_loop_src(n: usize) -> String {
        format!(
            r#"
    function complex_accum(n::Int64)::Float64
        z = Complex{{Float64}}(0.1, 0.2)
        c = Complex{{Float64}}(0.1, 0.2)
        i = 0
        while i < n
            z = z * z + c
            i = i + 1
        end
        real(z) + imag(z)
    end
    println(complex_accum({n}))
    "#
        )
    }

    /// The **im-literal** spelling of the same loop (Issue #9198 S3): `z`/`c` are
    /// initialized with `im`-based literals whose coefficients are provably `Float64`
    /// (`0.0 + 0.0im`, `0.1 + 0.2im`) rather than explicit `Complex{Float64}(…)`
    /// constructors. S2 deliberately bailed on this form; S3 recognizes it, so this
    /// loop must now be allocation-free just like `complex_loop_src`.
    fn complex_im_literal_loop_src(n: usize) -> String {
        format!(
            r#"
    function complex_im_accum(n::Int64)::Float64
        z = 0.0 + 0.0im
        c = 0.1 + 0.2im
        i = 0
        while i < n
            z = z * z + c
            i = i + 1
        end
        real(z) + imag(z)
    end
    println(complex_im_accum({n}))
    "#
        )
    }

    /// Real-decomposed control: the same recurrence carried in two `Float64` slots
    /// (`zr`, `zi`) with no `Complex` value. This is what S2 lowers the boxed loop
    /// *into*; it is already fully slotized and should allocate ~0 per iteration.
    fn float_control_src(n: usize) -> String {
        format!(
            r#"
    function real_accum(n::Int64)::Float64
        zr = 0.1
        zi = 0.2
        cr = 0.1
        ci = 0.2
        i = 0
        while i < n
            nzr = zr * zr - zi * zi + cr
            nzi = 2.0 * zr * zi + ci
            zr = nzr
            zi = nzi
            i = i + 1
        end
        zr + zi
    end
    println(real_accum({n}))
    "#
        )
    }

    /// Median-of-3 differenced per-iteration allocation count for a templated
    /// program, robust to a stray one-off allocation in any single measurement.
    fn allocs_per_iter(src_fn: impl Fn(usize) -> String) -> f64 {
        let lo = compile(&src_fn(N_LO));
        let hi = compile(&src_fn(N_HI));
        // Warm up both so any process-global lazy init is already paid before the
        // measured windows (it is identical for both programs and cancels anyway).
        let _ = allocs_during_run(&lo);
        let _ = allocs_during_run(&hi);

        let mut deltas = Vec::new();
        for _ in 0..3 {
            let a_lo = allocs_during_run(&lo);
            let a_hi = allocs_during_run(&hi);
            deltas.push(a_hi.saturating_sub(a_lo));
        }
        deltas.sort_unstable();
        let median_delta = deltas[1] as f64;
        median_delta / (N_HI - N_LO) as f64
    }

    /// Sanity: the counting allocator actually observes heap allocations.
    #[test]
    fn counting_allocator_observes_allocations_issue_9198() {
        THREAD_ALLOC_COUNT.with(|n| n.set(0));
        THREAD_COUNTING.with(|c| c.set(true));
        let v: Vec<u64> = (0..128).collect();
        std::hint::black_box(&v);
        THREAD_COUNTING.with(|c| c.set(false));
        assert!(
            THREAD_ALLOC_COUNT.with(Cell::get) >= 1,
            "counting allocator failed to observe a Vec allocation"
        );
    }

    /// Asserts the **S2 landed** result (Issue #9198): the typed `Complex{Float64}`
    /// `z = z*z + c` loop, after slot-pair SROA (`compile::complex_sroa`), issues no
    /// per-iteration heap allocations *from the struct representation* — its
    /// allocation count has dropped to the real-decomposed control's interpreted-loop
    /// floor. Before S2 this loop paid ~21 allocs/iter (the boxed-`StructInstance`
    /// field `Vec` on each `*`/`+` result); it now matches the fully-slotized control
    /// (both ~1/iter). See `docs/vm/REGISTER_VM.md` §"Multi-Slot Scalar (isbits
    /// Immutable Struct) Unboxing".
    #[test]
    fn typed_complex_loop_is_allocation_free_after_slot_pair_sroa_issue_9198() {
        let complex_per_iter = allocs_per_iter(complex_loop_src);
        let float_per_iter = allocs_per_iter(float_control_src);

        // Emit the numbers for the design record (visible with `--nocapture`).
        println!(
            "[issue-9198 S2] typed Complex{{Float64}} z=z*z+c loop: \
             {complex_per_iter:.3} heap allocs/iter (was ~21 before slot-pair SROA)"
        );
        println!(
            "[issue-9198 S2] real-decomposed Float64 control loop: \
             {float_per_iter:.3} heap allocs/iter"
        );

        // The float control is already slotized: essentially zero allocs/iter (the
        // ~1/iter residual is the interpreter's per-iteration bookkeeping floor).
        assert!(
            float_per_iter <= 1.0,
            "real-decomposed Float64 control should be ~allocation-free per iter, \
             measured {float_per_iter:.3}"
        );

        // S2 acceptance: the SROA'd Complex loop is now allocation-free from the
        // struct representation — it must sit at (or below) the control's floor, no
        // longer paying the boxed-StructInstance `Vec` on each `*`/`+` result.
        // Generous upper bound (mechanism + direction, not an exact pin) so the
        // assertion is not flaky under the full suite's cache/HashMap-seed variation;
        // the pre-S2 value was ~21, so anything ≤ 2 proves the boxing is gone.
        assert!(
            complex_per_iter <= float_per_iter + 1.0,
            "typed Complex loop expected to be allocation-free after slot-pair SROA \
             (≈ the real-decomposed control {float_per_iter:.3}), measured \
             {complex_per_iter:.3} — struct boxing appears to have returned"
        );
        assert!(
            complex_per_iter <= 2.0,
            "typed Complex loop per-iteration allocations unexpectedly high \
             ({complex_per_iter:.3}); slot-pair SROA (Issue #9198 S2) may have \
             regressed to the boxed representation"
        );
    }

    /// Asserts the **S3 landed** result (Issue #9198): the `im`-literal spelling of
    /// the typed `z = z*z + c` loop (`z = 0.0 + 0.0im`, `c = 0.1 + 0.2im`) — which S2
    /// deliberately left boxed — now also unboxes to the real-decomposed control's
    /// interpreted-loop floor. This guards the new S3 im-literal qualifying form: if
    /// the provably-Float64 im-literal init regressed to boxed, this loop would climb
    /// back toward ~21 allocs/iter.
    #[test]
    fn typed_complex_im_literal_loop_is_allocation_free_issue_9198_s3() {
        let im_per_iter = allocs_per_iter(complex_im_literal_loop_src);
        let float_per_iter = allocs_per_iter(float_control_src);

        println!(
            "[issue-9198 S3] im-literal (0.0+0.0im init) z=z*z+c loop: \
             {im_per_iter:.3} heap allocs/iter (S2 left this form boxed at ~21)"
        );

        assert!(
            float_per_iter <= 1.0,
            "real-decomposed Float64 control should be ~allocation-free per iter, \
             measured {float_per_iter:.3}"
        );
        // Same bound as the S2 assertion: anything ≤ 2 proves the im-literal form is
        // unboxed (the pre-S3 value for this spelling was ~21).
        assert!(
            im_per_iter <= float_per_iter + 1.0,
            "im-literal Complex loop expected to be allocation-free after S3 SROA \
             (≈ the real-decomposed control {float_per_iter:.3}), measured \
             {im_per_iter:.3} — the im-literal init appears to have stayed boxed"
        );
        assert!(
            im_per_iter <= 2.0,
            "im-literal Complex loop per-iteration allocations unexpectedly high \
             ({im_per_iter:.3}); the Issue #9198 S3 im-literal SROA form may have \
             regressed to the boxed representation"
        );
    }
}

mod abstract_numeric_param_slot_soundness_9724_tests {
    //! Abstract-numeric parameters (`x::Integer`/`x::Real`/`x::Signed`/...) must
    //! compile to a GENERIC slot, never a machine `I64`/`F64` slot — their runtime
    //! value can be a `BigInt`/`BigFloat` that a `LoadSlotI64`/`LoadSlotF64` would
    //! reject with `InternalError: LoadSlotI64: expected numeric in x, got
    //! BigInt(...)`. A CONCRETE parameter (`x::Int64`/`x::Float64`) keeps its typed
    //! slot. This pins the pipeline wiring that feeds
    //! `build_slot_info_with_generic_params` (Issue #9724). The isolated slotizer
    //! mechanism is covered by
    //! `subset_julia_vm_bytecode::slot::tests::abstract_numeric_param_slot_stays_generic_issue_9724`.

    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, VarTypeTag};

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

    fn param_slot_tag(f: &FunctionInfo, param_name: &str) -> Option<VarTypeTag> {
        let slot = f
            .slot_names
            .iter()
            .position(|n| n == param_name)
            .unwrap_or_else(|| panic!("param '{}' has no slot in {}", param_name, f.name));
        f.slot_types.get(slot).copied().flatten()
    }

    #[test]
    fn abstract_numeric_param_slots_are_generic_issue_9724() {
        let compiled = compile_source_with_base(
            r#"
    function abs_integer_probe_9724(x::Integer)
        return x + x
    end

    function abs_real_probe_9724(x::Real)
        return x + x
    end

    function abs_number_probe_9724(x::Number)
        return x + x
    end

    function abs_signed_probe_9724(x::Signed)
        return x + x
    end

    function abs_where_probe_9724(x::T) where {T<:Integer}
        return x + x
    end

    function concrete_int_probe_9724(x::Int64)
        return x + x
    end

    function concrete_float_probe_9724(x::Float64)
        return x + x
    end

    nothing
    "#,
        );

        // Abstract-numeric params (and `where`-bounded type vars) accept BigInt /
        // BigFloat, so their slot MUST stay generic (`None`).
        for name in [
            "abs_integer_probe_9724",
            "abs_real_probe_9724",
            "abs_number_probe_9724",
            "abs_signed_probe_9724",
            "abs_where_probe_9724",
        ] {
            let f = get_function(&compiled, name);
            assert_eq!(
                param_slot_tag(f, "x"),
                None,
                "{name}: abstract-numeric param `x` must compile to a generic slot, \
                 not a machine slot the slotizer could turn into LoadSlotI64/F64"
            );
        }

        // Concrete parameters keep their fast typed slot — the fix must not
        // de-optimize genuinely machine-width annotations.
        assert_eq!(
            param_slot_tag(get_function(&compiled, "concrete_int_probe_9724"), "x"),
            Some(VarTypeTag::I64),
            "x::Int64 must keep its typed I64 slot"
        );
        assert_eq!(
            param_slot_tag(get_function(&compiled, "concrete_float_probe_9724"), "x"),
            Some(VarTypeTag::F64),
            "x::Float64 must keep its typed F64 slot"
        );
    }
}

mod covariant_shorthand_undef_bound_10373_tests {
    //! Issue #10373: a BARE top-level assignment whose RHS is an anonymous
    //! covariant/contravariant bound shorthand naming an undefined identifier
    //! (`x = Vector{<:SomeUndefinedName}`) was registered as a static string
    //! type alias by `extract_type_alias_from_binding`, bypassing runtime
    //! name resolution entirely. This shape cannot be pinned by a fixture
    //! (fixtures must end with `true`; an uncaught top-level statement error
    //! aborts the file), so the exact MWE lives here. The expression-position
    //! and function-body shapes are covered by
    //! `tests/fixtures/types/covariant_shorthand_undef_bound_10373.jl`.

    use crate::common::{compile_and_run_str_with_output, run_core_pipeline, run_with_large_stack};

    fn assert_undefvarerror(src: &str, name: &str) {
        let err = run_core_pipeline(src, 0)
            .expect_err("undefined anonymous bound name must raise UndefVarError");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
        assert!(
            err.contains(name),
            "error must name the undefined bound `{name}`, got: {err}"
        );
    }

    #[test]
    fn top_level_covariant_undef_bound_assignment_raises_undefvarerror() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "x = Vector{<:SomeUndefinedNameABC10373}\nstring(x)",
                "SomeUndefinedNameABC10373",
            );
        });
    }

    #[test]
    fn top_level_contravariant_undef_bound_assignment_raises_undefvarerror() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "x = Vector{>:SomeUndefinedNameABC10373}\nstring(x)",
                "SomeUndefinedNameABC10373",
            );
        });
    }

    #[test]
    fn top_level_nested_undef_bound_assignment_raises_undefvarerror() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "x = Dict{String, <:SomeUndefinedNameABC10373}\nstring(x)",
                "SomeUndefinedNameABC10373",
            );
        });
    }

    #[test]
    fn top_level_defined_struct_bound_assignment_still_resolves() {
        run_with_large_stack(|| {
            // The non-error half of the same alias-branch rejection: a bound
            // naming a REAL user struct must resolve via runtime global
            // lookup and produce the identical display.
            let output = compile_and_run_str_with_output(
                "struct FooCovX10373 end\nx = Vector{<:FooCovX10373}\nprintln(string(x))",
                0,
            );
            assert!(
                output.contains("Vector{<:FooCovX10373}"),
                "expected `Vector{{<:FooCovX10373}}` in output, got: {output}"
            );
        });
    }
}

mod filtered_generator_empty_provenance_10621_tests {
    //! Issue #10621: empty filtered-generator eltype inference is only sound
    //! when both the body and predicate provenance are transparent. These
    //! source-level guards pin the compiler/runtime boundary that prevents a
    //! user-call predicate from reusing a body-only `result_element_type`.

    fn section_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start_idx = source
            .find(start)
            .unwrap_or_else(|| panic!("missing section start: {start}"));
        let rest = &source[start_idx..];
        let end_idx = rest
            .find(end)
            .unwrap_or_else(|| panic!("missing section end after {start}: {end}"));
        &rest[..end_idx]
    }

    fn compact(source: &str) -> String {
        source.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn compiler_rejects_body_eltype_when_filter_predicate_is_nontransparent() {
        let source = include_str!("../../subset_julia_vm_compile/src/compile/expr/collection.rs");
        let function = section_between(
            source,
            "fn filtered_generator_result_element_type(",
            "fn empty_collection_body_element_type(",
        );
        let compact = compact(function);

        assert!(
            compact.contains("ifself.expr_has_nontransparent_filter_call(filter_expr,0){returnNone;}"),
            "filtered generator eltype inference must reject nontransparent predicates before trusting body-only provenance:\n{function}"
        );
    }

    #[test]
    fn collect_empty_filtered_function_path_erases_inlined_predicate_eltype() {
        let source = include_str!("../../subset_julia_vm_vm/src/vm/type_ops/iteration.rs");
        let arm = section_between(
            source,
            "GeneratorCallable::FilteredFunctionIndex",
            "self.start_hof_filter_map_values_with_array_result(",
        );
        let compact = compact(arm);

        assert!(
            compact.contains("letpredicate_uses_inlined_call=")
                && compact.contains("__sjulia_inline_arg_")
                && compact.contains(
                    "letelement_type=ifpredicate_uses_inlined_call{ArrayElementType::UnionOf(Vec::new())}else{result_element_type.unwrap_or_else(||ArrayElementType::UnionOf(Vec::new()))}"
                ),
            "empty collect over a filtered generator must erase result_element_type when the predicate used an inlined/user call:\n{arm}"
        );
    }

    #[test]
    fn hof_filter_map_finalize_erases_inlined_predicate_eltype() {
        let source = include_str!("../../subset_julia_vm_vm/src/vm/hof_exec/value_mode.rs");
        let function = section_between(
            source,
            "fn filter_map_finalize(&mut self)",
            "pub(crate) fn start_hof_runtime_call_values_with_array_result(",
        );
        let compact = compact(function);

        assert!(
            compact.contains("letpredicate_uses_inlined_call=")
                && compact.contains("__sjulia_inline_arg_")
                && compact.contains(
                    "letelement_type=ifpredicate_uses_inlined_call{ArrayElementType::UnionOf(Vec::new())}else{result_element_type.unwrap_or_else(||ArrayElementType::UnionOf(Vec::new()))}"
                ),
            "HOF filter-map finalization must erase result_element_type when the predicate used an inlined/user call:\n{function}"
        );
    }
}

mod signature_definition_order_forward_refs_11114_11118_tests {
    //! Issues #11114 / #11118: two remaining holes in #11025's definition-time
    //! forward-reference detection.
    //!
    //! #11118's actual root cause (found empirically; the issue's own
    //! hypothesis of un-stamped nested functions / abstract types turned out
    //! to already be handled by #11117's byte-position fallback in
    //! `TypeDefinitionPosition::is_before`): a top-level program whose ENTIRE
    //! user body is declarations -- no other executable statement at all, the
    //! exact shape `g(x::T) = 1; abstract type T end` with nothing else in the
    //! file -- never enters the `!user_main_stmts.is_empty()` branch of
    //! `compile_main`, so the per-statement flush loop that emits
    //! `emit_signature_definition_probes` for queued top-level function
    //! activations never runs even once, and the probe is silently dropped.
    //! This is orthogonal to struct vs. abstract type: the same bug reproduces
    //! for a bare top-level STRUCT forward reference with no trailing
    //! statement (confirmed against upstream `julia`, which raises
    //! `UndefVarError` in both cases). This shape cannot be pinned by a
    //! fixture (fixtures must end with `true`, i.e. a trailing executable
    //! statement -- which is exactly what was masking the bug, since it also
    //! provides a later position that triggers the flush).
    //!
    //! #11114: the deliberately order-independent type-alias pre-scan (Issue
    //! #5055) registers every `const Name = T` binding regardless of source
    //! position, so a signature annotation naming an alias defined AFTER the
    //! method used to resolve silently instead of raising `UndefVarError` like
    //! upstream. Empirically this is already fixed on current `main` (verified
    //! against `julia 1.12.6`); the tests below pin it as a regression fixture
    //! now that the same probe surface is being touched for #11118.
    //!
    //! Verified against `julia 1.12.6`.

    use crate::common::{compile_and_run_str_with_output, run_core_pipeline, run_with_large_stack};
    use subset_julia_vm::repl::REPLSession;

    fn assert_undefvarerror(src: &str, name: &str) {
        let err = run_core_pipeline(src, 0)
            .expect_err("forward-referenced signature annotation must raise UndefVarError");
        assert!(
            err.contains("UndefVarError"),
            "expected UndefVarError, got: {err}"
        );
        assert!(
            err.contains(name),
            "error must name the undefined/forward-referenced identifier `{name}`, got: {err}"
        );
    }

    // --- Issue #11118: declaration-only top-level program -------------------

    #[test]
    fn abstract_type_forward_reference_as_sole_program_content_raises() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "g11118(x::NotYetDefinedAbstract11118) = 1\n\
                 abstract type NotYetDefinedAbstract11118 end\n",
                "NotYetDefinedAbstract11118",
            );
        });
    }

    #[test]
    fn struct_forward_reference_as_sole_program_content_raises() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "f11118(x::LaterStruct11118) = 1\nstruct LaterStruct11118 end\n",
                "LaterStruct11118",
            );
        });
    }

    #[test]
    fn forward_reference_trailing_last_real_statement_raises() {
        // A function/type declaration trailing the program's last REAL
        // (non-declaration) statement: nothing follows it either, so it hits
        // the exact same dropped-flush gap.
        run_with_large_stack(|| {
            assert_undefvarerror(
                "println(1)\ng11118b(x::NotYetDefinedAbstract11118b) = 1\n\
                 abstract type NotYetDefinedAbstract11118b end\n",
                "NotYetDefinedAbstract11118b",
            );
        });
    }

    #[test]
    fn declaration_only_program_with_already_defined_type_does_not_raise() {
        // Control: the type exists BEFORE the method annotating with it, so
        // even though this is also a declaration-only program, nothing is a
        // forward reference and the probe drain must not misfire.
        run_with_large_stack(|| {
            let value = compile_and_run_str_with_output(
                "abstract type EarlyAbstract11118 end\n\
                 g11118c(x::EarlyAbstract11118) = 1\n",
                0,
            );
            assert_eq!(value, "", "declaration-only valid program must not error");
        });
    }

    // --- Issue #11114: alias definition-position gate -----------------------

    #[test]
    fn builtin_target_alias_forward_reference_raises() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "u11114(x::AL2_11114) = 24\nconst AL2_11114 = Int64\nprintln(u11114(1))\n",
                "AL2_11114",
            );
        });
    }

    #[test]
    fn struct_target_alias_forward_reference_raises() {
        run_with_large_stack(|| {
            assert_undefvarerror(
                "struct Later11114 end\n\
                 u11114b(x::AL11114) = 24\n\
                 const AL11114 = Later11114\n\
                 println(u11114b(Later11114()))\n",
                "AL11114",
            );
        });
    }

    #[test]
    fn alias_defined_before_method_still_dispatches() {
        // Control (Issue #11114): the alias is defined BEFORE the method, so
        // it is not a forward reference and dispatch must succeed exactly as
        // before -- this must not regress.
        run_with_large_stack(|| {
            let output = compile_and_run_str_with_output(
                "const AL3_11114 = Int64\n\
                 u11114c(x::AL3_11114) = 24\n\
                 println(u11114c(1))\n",
                0,
            );
            assert_eq!(output.trim(), "24");
        });
    }

    // --- REPL per-cell lowering path -----------------------------------

    #[test]
    fn repl_cell_forward_reference_within_one_cell_raises() {
        // A declaration-only REPL cell whose function forward-references a
        // type defined LATER IN THE SAME CELL must still raise, matching the
        // file-mode behavior above.
        run_with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let result = session.eval(
                "h11118repl(x::NotYetDefinedRepl11118) = 1\n\
                 abstract type NotYetDefinedRepl11118 end\n",
            );
            assert!(
                !result.success,
                "expected failure, got success with value {:?}",
                result.value
            );
            let err = result.error.unwrap_or_default();
            assert!(err.contains("UndefVarError"), "got: {err}");
            assert!(err.contains("NotYetDefinedRepl11118"), "got: {err}");
        });
    }

    #[test]
    fn repl_cell_referencing_prior_cell_type_does_not_raise() {
        // A type defined in an EARLIER cell must remain visible to a
        // declaration-only later cell (no false-positive forward-reference,
        // Issue #11117's cross-cell concern carried over to the drained
        // activation path).
        run_with_large_stack(|| {
            let mut session = REPLSession::new(0);
            let first = session.eval("abstract type EarlyReplType11118 end\n");
            assert!(
                first.success,
                "abstract type declaration cell should succeed: {:?}",
                first.error
            );
            let second = session.eval("k11118repl(x::EarlyReplType11118) = 1\n");
            assert!(
                second.success,
                "declaration-only cell referencing an earlier cell's type must not raise: {:?}",
                second.error
            );
        });
    }
}
