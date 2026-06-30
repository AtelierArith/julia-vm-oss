use super::*;
use crate::vm::Value;

/// Run a test closure on a thread with 16 MB stack to avoid stack overflow
/// in debug builds. REPL eval involves deep recursion through parse → lower
/// → compile → VM execute, which exceeds the default 8 MB test-thread stack.
fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let builder = std::thread::Builder::new()
        .name("repl-test".into())
        .stack_size(16 * 1024 * 1024);
    let handler = builder.spawn(f).unwrap();
    if let Err(e) = handler.join() {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_repl_globals() {
    let mut globals = REPLGlobals::new();

    // Set and get integer
    globals.set("x", Value::I64(42));
    assert!(
        matches!(globals.get("x"), Some(Value::I64(42))),
        "Expected Some(I64(42)), got {:?}",
        globals.get("x")
    );

    // Set and get float
    globals.set("y", Value::F64(std::f64::consts::PI));
    assert!(
        matches!(globals.get("y"), Some(Value::F64(v)) if (v - std::f64::consts::PI).abs() < 0.001),
        "Expected Some(F64(~PI)), got {:?}",
        globals.get("y")
    );

    // Overwrite with different type
    globals.set("x", Value::F64(std::f64::consts::E));
    assert!(
        matches!(globals.get("x"), Some(Value::F64(v)) if (v - std::f64::consts::E).abs() < 0.001),
        "Expected Some(F64(~E)), got {:?}",
        globals.get("x")
    );

    // Clear
    globals.clear();
    assert!(globals.get("x").is_none());
}

#[test]
fn test_repl_session_simple() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Evaluate simple expression
        let result = session.eval("1 + 2");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected Some(Value::I64(3)), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_value_display_uses_user_show_7168() {
    run_with_large_stack(|| {
        // A user-type result echoes through its `show` method (matching
        // `string(x)`), not the default struct-field dump (Issue #7168).
        // Also guards Issue #8246: broad method-table return re-inference must
        // not recurse through REPL `show` paths or lose this display behavior.
        let mut s1 = REPLSession::new(7);
        let r1 = s1.eval("using Symbolics; @variables x; x^2 + 2x + 1");
        assert!(r1.success, "eval failed: {:?}", r1.error);
        assert_eq!(r1.value_display.as_deref(), Some("x^2 + 2*x + 1"));

        let mut s2 = REPLSession::new(7);
        let r2 = s2.eval("using Symbolics; @variables x; sin(x)");
        assert!(r2.success, "eval failed: {:?}", r2.error);
        assert_eq!(r2.value_display.as_deref(), Some("sin(x)"));

        // Types with a dedicated Rust formatter (Complex, Rational, LinRange,
        // array wrappers) are intentionally left on that path — `value_display`
        // is `None` so the existing (canonical) formatter is used.
        let mut s3 = REPLSession::new(7);
        let r3 = s3.eval("1.0 + 2.0im");
        assert_eq!(r3.value_display, None);

        let mut s3b = REPLSession::new(7);
        let r3b = s3b.eval("range(-3, stop = 3, length = 100)");
        assert_eq!(r3b.value_display, None);

        // A plain Int has no user `show` → no override; the default formatter is used.
        let mut s4 = REPLSession::new(7);
        let r4 = s4.eval("1 + 2");
        assert_eq!(r4.value_display, None);

        // A user struct WITHOUT a `show` method → no override (default dump path).
        let mut s5 = REPLSession::new(7);
        let r5 = s5.eval("struct Pt7168; a; b; end; Pt7168(1, 2)");
        assert!(r5.success, "eval failed: {:?}", r5.error);
        assert_eq!(r5.value_display, None);
    });
}

#[test]
fn test_repl_empty_array_persists_across_evals_7151() {
    // Regression (Issue #7151): an empty array global (`ps = []`, `Int[]`, ...)
    // was stored in REPLGlobals but never re-injected (value_to_init_expr returns
    // None for empty arrays), so the *next* eval raised `UndefVarError`. Empty
    // arrays of every element type must persist like any other binding.
    run_with_large_stack(|| {
        for init in ["[]", "Any[]", "Int[]", "Float64[]"] {
            let mut s = REPLSession::new(7);
            let r1 = s.eval(&format!("v = {init}"));
            assert!(r1.success, "{init}: define failed: {:?}", r1.error);
            let r2 = s.eval("length(v)");
            assert!(
                r2.success,
                "{init}: empty array global was dropped across evals: {:?}",
                r2.error
            );
            assert!(
                matches!(r2.value, Some(Value::I64(0))),
                "{init}: expected length 0, got {:?}",
                r2.value
            );
        }
    });
}

#[test]
fn test_repl_push_to_empty_array_across_evals_7151() {
    // The `ps = []` then `push!(ps, …)` pattern (what `@gif` lowers to) must work
    // when the two are separate REPL evaluations (Issue #7151).
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);
        assert!(session.eval("ps = []").success);
        let r = session.eval("push!(ps, 1)");
        assert!(
            r.success,
            "push! to persisted empty array failed: {:?}",
            r.error
        );
        let r = session.eval("push!(ps, 2)");
        assert!(r.success, "second push! failed: {:?}", r.error);
        let r = session.eval("length(ps)");
        assert!(
            matches!(r.value, Some(Value::I64(2))),
            "expected length 2 after two pushes, got {:?}",
            r.value
        );
    });
}

#[test]
fn test_repl_gif_with_global_accumulator_7151() {
    // End-to-end of the reported failure: `@gif for … push!(ps, p) end` after a
    // separate `ps = []` eval must not raise `UndefVarError: ps` (Issue #7151).
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);
        assert!(session.eval("using Plots").success);
        assert!(session.eval("ps = []").success);
        let r = session.eval(
            "@gif for t in -3.0:0.1:3.0\n    p = plot([1,2,3], [1,2,3])\n    push!(ps, p)\nend",
        );
        assert!(
            r.success,
            "@gif with global accumulator failed: {:?}",
            r.error
        );
        let r = session.eval("length(ps)");
        assert!(
            matches!(r.value, Some(Value::I64(n)) if n > 0),
            "expected ps to accumulate frames, got {:?}",
            r.value
        );
    });
}

#[test]
fn test_repl_persist_array_of_struct_with_empty_array_field_8086() {
    // Focused, Plots-free regression for the root cause behind the `@gif` failure
    // (Issue #8086 / #8063): a struct field that is an EMPTY array (e.g.
    // `Plot.hlines = Float64[]`) must not drop a REPL global holding an array of
    // such structs across eval boundaries. The cross-eval persistence rebuilds
    // each struct via a positional constructor; before the fix, an empty-array
    // field returned `None` (the top-level "defer to module initializer" rule
    // leaking into nested reconstruction), failing the whole struct → the array
    // global was silently dropped → the next eval raised `UndefVarError`.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);
        assert!(
            session
                .eval("struct Box8086; v::Vector{Float64}; w::Int; end")
                .success
        );
        assert!(session.eval("xs = []").success);
        let r = session.eval("push!(xs, Box8086(Float64[], 7))");
        assert!(r.success, "push! failed: {:?}", r.error);
        // A new eval must still see `xs` with the accumulated struct.
        let r = session.eval("length(xs)");
        assert!(
            matches!(r.value, Some(Value::I64(1))),
            "expected length 1 after persistence, got {:?}",
            r.value
        );
        let r = session.eval("xs[1].w");
        assert!(
            matches!(r.value, Some(Value::I64(7))),
            "expected xs[1].w == 7, got {:?}",
            r.value
        );
        let r = session.eval("length(xs[1].v)");
        assert!(
            matches!(r.value, Some(Value::I64(0))),
            "expected empty-array field to round-trip, got {:?}",
            r.value
        );
    });
}

#[test]
fn test_repl_session_variable_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a variable
        let result = session.eval("x = 10");
        assert!(result.success);

        // Use the variable in a subsequent evaluation
        let result = session.eval("x + 5");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(15))),
            "Expected Some(Value::I64(15)), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_abstractalgebra_polynomial_generator_persists_across_evals() {
    // Regression for Issue #8322: iOS REPL evaluates pasted input as separate
    // session evals, so the polynomial generator must survive across evals.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using AbstractAlgebra");
        assert!(
            result.success,
            "using AbstractAlgebra failed: {:?}",
            result.error
        );

        let result = session.eval("R, x = polynomial_ring(ZZ, :x)");
        assert!(
            result.success,
            "polynomial_ring assignment failed: {:?}",
            result.error
        );

        let result = session.eval("p = (x + 1)^3");
        assert!(
            result.success,
            "persisted polynomial generator failed: {:?}",
            result.error
        );
    });
}

#[test]
fn test_repl_abstractalgebra_residue_ring_parent_remains_callable_8496() {
    // Regression for Issue #8496: REPL global injection kept the integer
    // residue ring parent value visible, but the next eval compiled `Z7(10)` as
    // a direct global function call and errored "Unknown function: Z7".
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using AbstractAlgebra");
        assert!(
            result.success,
            "using AbstractAlgebra failed: {:?}",
            result.error
        );

        let result = session.eval("Z7 = residue_ring(ZZ, 7)[1]");
        assert!(
            result.success,
            "residue ring assignment failed: {:?}",
            result.error
        );

        let result = session.eval("a = Z7(10)");
        assert!(
            result.success,
            "persisted residue ring parent call failed: {:?}",
            result.error
        );

        let result = session.eval("data(a) == big(3)");
        assert!(
            matches!(result.value, Some(Value::Bool(true))),
            "Expected residue data to equal big(3), got value={:?}, error={:?}",
            result.value,
            result.error
        );
    });
}

#[test]
fn test_repl_session_range_variable_persistence() {
    // Regression: a `Range` global (`t = 0:0.01:2π`) was silently dropped by
    // inject_globals (no Literal form), so the next eval raised
    // `UndefVarError: t not defined`. Ranges must persist like any other binding.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Float range bound to a global.
        let result = session.eval("t = 0:0.01:2π");
        assert!(
            result.success,
            "range assignment failed: {:?}",
            result.error
        );

        // Referencing it in a later eval must not raise UndefVarError.
        let result = session.eval("length(t)");
        assert!(
            result.success,
            "range global not persisted: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(629))),
            "Expected length 629 for 0:0.01:2π, got {:?}",
            result.value
        );

        // Integer non-unit range round-trips through reconstruction too.
        let result = session.eval("u = 2:3:11");
        assert!(
            result.success,
            "int range assignment failed: {:?}",
            result.error
        );
        let result = session.eval("sum(u)");
        assert!(
            result.success,
            "int range global not persisted: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(26))), // 2+5+8+11
            "Expected sum 26 for 2:3:11, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_import_only_inputs_are_silent_6000() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        for src in ["using LinearAlgebra", "using Plots"] {
            let result = session.eval(src);
            assert!(result.success, "`{src}` failed: {:?}", result.error);
            assert!(
                result.value.is_none(),
                "`{src}` should not display package-load internals in the REPL"
            );
            assert!(
                result.display_artifact.is_none(),
                "`{src}` should not attach a display artifact"
            );
        }

        let result = session.eval("norm([3.0, 4.0])");
        assert!(result.success, "LinearAlgebra import did not persist");
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 5.0).abs() < 1e-12),
            "Expected norm result 5.0 after import, got {:?}",
            result.value
        );

        let result = session.eval("plot(sin)");
        assert!(result.success, "Plots import did not persist");
        assert!(
            result.display_artifact.is_some(),
            "Plots import should still enable plot artifacts"
        );
    });
}

#[test]
fn test_repl_plotbang_appends_across_evals() {
    // Regression (Issue #5296): in the REPL, each input line is a separate
    // `eval`. The `Plots` module's mutable state `const _CURRENT_SERIES = Any[]`
    // was re-initialized to empty on every eval (module bodies re-run before
    // main), so `plot!(cos)` after `plot(sin)` saw an empty current plot and
    // replaced it with a cos-only plot instead of appending. Module-level
    // mutable state must persist across evaluations like user globals do.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let r = session.eval("using Plots");
        assert!(r.success, "using Plots failed: {:?}", r.error);

        let r = session.eval("plot(sin)");
        assert!(r.success, "plot(sin) failed: {:?}", r.error);

        // A fresh eval: plot!(cos) must append to the persisted current plot,
        // yielding two series (sin + cos), not replace it with cos alone. The
        // Plotly artifact the host renders must therefore carry two line traces.
        let r = session.eval("plot!(cos)");
        assert!(r.success, "plot!(cos) failed: {:?}", r.error);
        let artifact = r
            .display_artifact
            .as_ref()
            .expect("plot!(cos) should produce a Plotly display artifact");
        let line_traces = artifact.data.matches(r#""mode":"lines""#).count();
        assert_eq!(
            line_traces, 2,
            "plot!(cos) should render sin + cos (2 line traces), got {} in {}",
            line_traces, artifact.data
        );

        // The series count of the returned plot also reflects the append.
        let r = session.eval("length(plot!(cos).series)");
        assert!(r.success, "plot!(cos) failed: {:?}", r.error);
        assert!(
            matches!(r.value, Some(Value::I64(3))),
            "second plot!(cos) should append again (sin + cos + cos = 3 series), got {:?}",
            r.value
        );

        // scatter! keeps appending across yet another eval (now 4 series).
        let r = session.eval("length(scatter!([0.0], [0.0]).series)");
        assert!(r.success, "scatter! failed: {:?}", r.error);
        assert!(
            matches!(r.value, Some(Value::I64(4))),
            "scatter! should append (4th series), got {:?}",
            r.value
        );

        // A non-mutating `plot` in a fresh eval replaces the current plot, so the
        // series count resets to one (the persisted state must not make plot append).
        let r = session.eval("length(plot(tan).series)");
        assert!(r.success, "plot(tan) failed: {:?}", r.error);
        assert!(
            matches!(r.value, Some(Value::I64(1))),
            "plot(tan) should reset to a single series, got {:?}",
            r.value
        );
    });
}

#[test]
fn test_repl_plotbang_3d_appends_across_evals() {
    // Regression (Issue #5296): the 3D mutating variants (`plot!(x,y,z)` /
    // `scatter!(x,y,z)`) share the same `_CURRENT_SERIES` module state, so they
    // suffered the identical cross-eval reset — a 3D `scatter!` replaced the
    // current 3D plot instead of appending to it.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let r = session.eval("using Plots");
        assert!(r.success, "using Plots failed: {:?}", r.error);

        let r = session.eval("t = 0:0.5:2π");
        assert!(r.success, "range assignment failed: {:?}", r.error);

        // First 3D command (a `path3d` series).
        let r = session.eval("length(plot!(cos.(t), sin.(t), t).series)");
        assert!(r.success, "3D plot! failed: {:?}", r.error);
        assert!(
            matches!(r.value, Some(Value::I64(1))),
            "first 3D plot! should create one path3d series, got {:?}",
            r.value
        );

        // A fresh eval: 3D scatter! must append (curve + points), not replace.
        let r = session.eval("scatter!(cos.(t), sin.(t), t)");
        assert!(r.success, "3D scatter! failed: {:?}", r.error);
        let artifact = r
            .display_artifact
            .as_ref()
            .expect("3D scatter! should produce a Plotly display artifact");
        let traces_3d = artifact.data.matches(r#""type":"scatter3d""#).count();
        assert_eq!(
            traces_3d, 2,
            "3D scatter! should render the path3d curve + scatter3d points (2 traces), got {} in {}",
            traces_3d, artifact.data
        );
    });
}

#[test]
fn test_repl_plotbang_3d_appends_across_evals_with_t_redefined() {
    // Reproduction of the on-device screenshot (Issue #5296 follow-up): the user
    // redefines `t` between the 3D `plot!` and the 3D `scatter!`:
    //   plot!(cos.(t), sin.(t), t)   # t = 0:0.1:2π
    //   t = 0:0.5:2π
    //   scatter!(cos.(t), sin.(t), t)
    // The scatter! must still append (curve + points), not replace.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let r = session.eval("using Plots");
        assert!(r.success, "using Plots failed: {:?}", r.error);

        let r = session.eval("t = 0:0.1:2π");
        assert!(r.success, "first range assignment failed: {:?}", r.error);

        let r = session.eval("plot!(cos.(t), sin.(t), t)");
        assert!(r.success, "3D plot! failed: {:?}", r.error);

        // Redefine t between the two mutating calls (as in the screenshot).
        let r = session.eval("t = 0:0.5:2π");
        assert!(r.success, "second range assignment failed: {:?}", r.error);

        let r = session.eval("scatter!(cos.(t), sin.(t), t)");
        assert!(r.success, "3D scatter! failed: {:?}", r.error);
        let artifact = r
            .display_artifact
            .as_ref()
            .expect("3D scatter! should produce a Plotly display artifact");
        let traces_3d = artifact.data.matches(r#""type":"scatter3d""#).count();
        assert_eq!(
            traces_3d, 2,
            "3D scatter! should render the path3d curve + scatter3d points (2 traces), got {} in {}",
            traces_3d, artifact.data
        );
    });
}

#[test]
fn test_repl_session_ans() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation
        session.eval("42");

        // ans should be available
        let result = session.eval("ans * 2");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(84))),
            "Expected Some(Value::I64(84)), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_session_ans_after_assignment() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Assignment should also set ans (Julia behavior: assignment returns value)
        let result = session.eval("x = 5");
        assert!(result.success, "x = 5 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "x = 5 should return I64(5), got {:?}",
            result.value
        );

        // Check if ans is available
        let result = session.eval("ans");
        assert!(
            result.success,
            "ans should be available: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "ans should be I64(5), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_eval_initializes_compile_cache() {
    run_with_large_stack(|| {
        crate::compile::cache::clear_cache();
        assert!(
            !crate::compile::cache::is_cache_initialized(),
            "cache should start empty for this test"
        );

        let mut session = REPLSession::new(42);
        let result = session.eval("1 + 1");
        assert!(result.success, "eval should succeed: {:?}", result.error);
        assert!(
            crate::compile::cache::is_cache_initialized(),
            "REPL eval should initialize compile cache to avoid full Base recompilation"
        );

        crate::compile::cache::clear_cache();
    });
}

#[test]
fn test_repl_session_function_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a function
        let result = session.eval("function double(x)\n  x * 2\nend");
        assert!(result.success);
        assert!(
            session.function_names().contains(&"double".to_string()),
            "function_names should include persisted REPL functions"
        );

        // Use the function
        let result = session.eval("double(21)");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_session_imported_module_names_include_packages() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using Primes");
        assert!(result.success, "using Primes failed: {:?}", result.error);
        assert!(
            session
                .imported_module_names()
                .contains(&"Primes".to_string()),
            "imported_module_names should include Primes"
        );
    });
}

#[test]
fn test_repl_session_field_completion_sources() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("struct CompletionPoint\n  x::Int64\n  y::Int64\nend");
        assert!(
            result.success,
            "struct definition failed: {:?}",
            result.error
        );

        let result = session.eval("point = CompletionPoint(1, 2)");
        assert!(
            result.success,
            "struct construction failed: {:?}",
            result.error
        );

        let fields = session.field_names_by_object();
        assert!(
            fields
                .iter()
                .any(|(name, names)| name == "point"
                    && names == &vec!["x".to_string(), "y".to_string()]),
            "field_names_by_object should include point fields, got {fields:?}"
        );
    });
}

#[test]
fn test_repl_session_reset() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a variable
        let result = session.eval("x = 100");
        assert!(result.success);

        // Verify the variable is set
        let result = session.eval("x + 1");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(101))),
            "Expected Some(Value::I64(101)), got {:?}",
            result.value
        );

        // Reset
        session.reset();

        // After reset, defining a new variable should work independently
        // and the old value should not persist
        let result = session.eval("y = 5");
        assert!(result.success);

        // Verify session variables are cleared (no 'x' in variable_names)
        let names = session.variable_names();
        assert!(
            !names.contains(&"x".to_string()),
            "x should not be in variable names after reset"
        );
        assert!(
            names.contains(&"y".to_string()),
            "y should be in variable names"
        );
    });
}

#[test]
fn test_split_expressions() {
    let session = REPLSession::new(42);

    let src = r#"function fizzbuzz(n)
    for i in 1:n
        println(i)
    end
end

fizzbuzz(10)"#;

    let exprs = session.split_expressions(src).unwrap();

    assert_eq!(exprs.len(), 2, "Should have 2 top-level expressions");

    // First expression: function definition
    assert!(
        exprs[0].2.starts_with("function fizzbuzz"),
        "First should be function"
    );
    assert!(exprs[0].2.ends_with("end"), "First should end with 'end'");

    // Second expression: function call
    assert_eq!(exprs[1].2.trim(), "fizzbuzz(10)", "Second should be call");
}

#[test]
fn test_split_expressions_sequential_eval() {
    run_with_large_stack(|| {
        let src = r#"function double(x)
    x * 2
end

double(21)"#;

        let mut session = REPLSession::new(42);
        let exprs = session.split_expressions(src).unwrap();

        assert_eq!(exprs.len(), 2);

        // First: define function
        let result = session.eval(&exprs[0].2);
        assert!(result.success, "Function definition should succeed");

        // Second: call function
        let result = session.eval(&exprs[1].2);
        assert!(result.success, "Function call should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_fizzbuzz_split() {
    run_with_large_stack(|| {
        let src = r#"function fizzbuzz(n)
    for i in 1:n
        if i % 15 == 0
            println("FizzBuzz")
        elseif i % 3 == 0
            println("Fizz")
        elseif i % 5 == 0
            println("Buzz")
        else
            println(i)
        end
    end
end

fizzbuzz(15)"#;

        let mut session = REPLSession::new(42);

        // Test split detection
        let exprs = session.split_expressions(src);
        assert!(exprs.is_some(), "Should detect multiple expressions");
        let exprs = exprs.unwrap();
        assert_eq!(exprs.len(), 2, "Should have 2 expressions");

        // First expression: function definition
        assert!(
            exprs[0].2.starts_with("function fizzbuzz"),
            "First should be function"
        );

        // Second expression: function call
        assert_eq!(exprs[1].2.trim(), "fizzbuzz(15)", "Second should be call");

        // Evaluate sequentially
        let result1 = session.eval(&exprs[0].2);
        assert!(result1.success, "Function definition should succeed");

        let result2 = session.eval(&exprs[1].2);
        assert!(result2.success, "Function call should succeed");

        // Check output contains expected values
        assert!(result2.output.contains("1"), "Should output 1");
        assert!(result2.output.contains("Fizz"), "Should output Fizz");
        assert!(result2.output.contains("Buzz"), "Should output Buzz");
        assert!(
            result2.output.contains("FizzBuzz"),
            "Should output FizzBuzz"
        );
    });
}

#[test]
fn test_repl_array_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define an array
        let result = session.eval("v1 = [1, 2, 3, 4, 5]");
        assert!(
            result.success,
            "Array definition should succeed: {:?}",
            result.error
        );

        // Use the array in a subsequent evaluation
        let result = session.eval("length(v1)");
        assert!(
            result.success,
            "Using array should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected length to be 5, got {:?}",
            result.value
        );

        // Define another array and use both
        let result = session.eval("v2 = [5, 4, 3, 2, 1]");
        assert!(result.success, "Second array definition should succeed");

        // Define a function that uses arrays
        let result = session.eval(
            r#"function dot_product(a, b)
                sum = 0.0
                for i in 1:length(a)
                    sum += a[i] * b[i]
                end
                sum
            end"#,
        );
        assert!(
            result.success,
            "Function definition should succeed: {:?}",
            result.error
        );

        // Call the function with persisted arrays
        let result = session.eval("dot_product(v1, v2)");
        assert!(
            result.success,
            "Function call should succeed: {:?}",
            result.error
        );
        // v1 · v2 = 1*5 + 2*4 + 3*3 + 4*2 + 5*1 = 5 + 8 + 9 + 8 + 5 = 35
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 35.0).abs() < 0.001),
            "Pattern match failed, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_reshaped_array_persistence_reads_logical_storage() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval(
            r#"
            base = [1, 2, 3, 4]
            mat = reshape(base, 2, 2)
            base[4] = 40
            mat
            "#,
        );
        assert!(
            result.success,
            "Reshaped array setup should succeed: {:?}",
            result.error
        );

        let result = session.eval("mat[2, 2]");
        assert!(
            result.success,
            "Persisted reshaped array access should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(40))),
            "Expected persisted logical value 40, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_matrix_comprehension_global_persistence_5995() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let setup = session.eval("using LinearAlgebra\nusing Plots");
        assert!(
            setup.success,
            "module setup should succeed: {:?}",
            setup.error
        );

        let ranges = session.eval("x = y = range(-3, stop = 3, length = 4)");
        assert!(
            ranges.success,
            "range globals should persist: {:?}",
            ranges.error
        );

        let matrix = session.eval("z = [sinc(norm([xi, yi])) for yi in y, xi in x]");
        assert!(
            matrix.success,
            "matrix comprehension assignment should succeed: {:?}",
            matrix.error
        );
        assert!(
            session.variable_names().contains(&"z".to_string()),
            "matrix comprehension global should be stored after assignment; names={:?}",
            session.variable_names()
        );

        let readback = session.eval("z[2, 3]");
        assert!(
            readback.success,
            "matrix comprehension global should be re-injected: {:?}",
            readback.error
        );
        assert!(
            matches!(readback.value, Some(Value::F64(_))),
            "expected matrix element readback as Float64, got {:?}",
            readback.value
        );

        let surface = session.eval("surface(x, y, z)");
        assert!(
            surface.success,
            "surface should see persisted matrix z global: {:?}",
            surface.error
        );
    });
}

#[test]
fn test_repl_memory_persistence_reads_storage_without_array_bridge() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval(
            r#"
            m = Memory{Int64}(undef, 3)
            m[1] = 10
            m[2] = 20
            m[3] = 30
            m
            "#,
        );
        assert!(
            result.success,
            "Memory setup should succeed: {:?}",
            result.error
        );

        let result = session.eval("length(m)");
        assert!(
            result.success,
            "Persisted Memory length should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected persisted Memory length 3, got {:?}",
            result.value
        );

        let result = session.eval("m[2]");
        assert!(
            result.success,
            "Persisted Memory indexing should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(20))),
            "Expected persisted Memory element 20, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_memory_global_round_trip_preserves_type_identity() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval(
            r#"
            m = Memory{Int64}(undef, 3)
            m[1] = 10
            m[2] = 20
            m[3] = 30
            m
            "#,
        );
        assert!(
            result.success,
            "Memory setup should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Memory(_))),
            "Initial REPL result should be Memory, got {:?}",
            result.value
        );

        let result = session.eval("m");
        assert!(
            result.success,
            "Persisted Memory read should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Memory(_))),
            "Persisted global should remain Memory, got {:?}",
            result.value
        );

        let result = session.eval("typeof(m) == Memory{Int64}");
        assert!(
            result.success,
            "Persisted Memory typeof check should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Bool(true))),
            "Expected typeof(m) == Memory{{Int64}}, got {:?}",
            result.value
        );

        let result = session.eval("m[2]");
        assert!(
            result.success,
            "Persisted Memory indexing should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(20))),
            "Expected persisted Memory element 20, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_memory_ans_round_trip_preserves_type_identity() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval(
            r#"
            m = Memory{Int64}(undef, 2)
            m[1] = 7
            m[2] = 9
            m
            "#,
        );
        assert!(
            result.success,
            "Memory ans setup should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(session.get_ans(), Some(Value::Memory(_))),
            "ans should store Memory, got {:?}",
            session.get_ans()
        );

        let result = session.eval("ans");
        assert!(
            result.success,
            "Persisted ans Memory read should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Memory(_))),
            "ans should round-trip as Memory, got {:?}",
            result.value
        );

        let result = session.eval("saved_ans = ans");
        assert!(
            result.success,
            "Persisted ans alias should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Memory(_))),
            "saved_ans assignment should return Memory, got {:?}",
            result.value
        );

        let result = session.eval("typeof(saved_ans) == Memory{Int64}");
        assert!(
            result.success,
            "Persisted ans Memory typeof check should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Bool(true))),
            "Expected typeof(saved_ans) == Memory{{Int64}}, got {:?}",
            result.value
        );

        let result = session.eval("saved_ans[2]");
        assert!(
            result.success,
            "Persisted ans Memory indexing should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(9))),
            "Expected ans[2] == 9, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_semicolon_separated_statements() {
    run_with_large_stack(|| {
        // Test that semicolon-separated statements can be evaluated in REPL
        let mut session = REPLSession::new(42);

        // Evaluate statements separately
        let result = session.eval("x = 3");
        assert!(result.success, "x = 3 should succeed");

        let result = session.eval("y = 2");
        assert!(result.success, "y = 2 should succeed");

        let result = session.eval("z = x + y");
        assert!(result.success, "z = x + y should succeed");

        // z should now be 5
        let result = session.eval("z");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected z to be 5, got {:?}",
            result.value
        );

        let result = session.eval("f = +; f(1, 2)");
        assert!(
            result.success,
            "semicolon-separated bare operator assignment should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected f(1, 2) to be 3, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_time_macro_variable_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Test that variables assigned inside @time persist
        let result = session.eval("@time x = 42");
        assert!(
            result.success,
            "@time assignment should succeed: {:?}",
            result.error
        );

        // x should be available in the next evaluation
        let result = session.eval("x + 1");
        assert!(result.success, "Using x should succeed: {:?}", result.error);
        assert!(
            matches!(result.value, Some(Value::I64(43))),
            "Expected x + 1 to be 43, got {:?}",
            result.value
        );

        // Test with array
        let result = session.eval("@time grid = [1, 2, 3, 4, 5]");
        assert!(
            result.success,
            "@time array assignment should succeed: {:?}",
            result.error
        );

        // grid should be available
        let result = session.eval("length(grid)");
        assert!(
            result.success,
            "Using grid should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected length(grid) to be 5, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_using_returns_nothing() {
    // Issue #4357: `using <Module>` was returning the asyncmap docstring
    // because Base sources end with `"""doc""" function asyncmap ... end`.
    // The lowering treated the docstring as a standalone main statement, so
    // when the user's input added nothing to main (e.g. `using LinearAlgebra`),
    // the merged main's last value was the leftover docstring literal.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using LinearAlgebra");
        assert!(
            result.success,
            "using LinearAlgebra should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Nothing) | None),
            "using LinearAlgebra should not return a value, got {:?}",
            result.value
        );

        // Confirm a bare struct definition (also leaves user main empty) does
        // not leak a Base docstring either.
        let result = session.eval("struct DocstringLeakProbe end");
        assert!(
            result.success,
            "struct definition should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Nothing) | None),
            "struct definition should not return a value, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_using_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: using Statistics
        let result = session.eval("using Statistics");
        assert!(
            result.success,
            "using Statistics should succeed: {:?}",
            result.error
        );

        // Second evaluation: mean([1,2,3]) - should work because using persists
        let result = session.eval("mean([1, 2, 3])");
        assert!(
            result.success,
            "mean after using should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 2.0).abs() < 0.001),
            "Pattern match failed, got {:?}",
            result.value
        );

        // Third evaluation: std - should also work
        let result = session.eval("std([1.0, 2.0, 3.0])");
        assert!(
            result.success,
            "std after using should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 1.0).abs() < 0.001),
            "Pattern match failed, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_using_test_macro_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using Test");
        assert!(
            result.success,
            "using Test should succeed: {:?}",
            result.error
        );

        let result = session.eval("@test 1 + 1 == 2");
        assert!(
            result.success,
            "@test after using Test should succeed: {:?}",
            result.error
        );
    });
}

#[test]
fn test_repl_namedtuple_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: create a NamedTuple
        let result = session.eval("F = (U = [1.0, 2.0], S = [3.0, 4.0], V = [5.0, 6.0])");
        assert!(
            result.success,
            "NamedTuple creation should succeed: {:?}",
            result.error
        );

        // Second evaluation: destructure the NamedTuple - F should persist
        let result = session.eval("U, S, V = F");
        assert!(
            result.success,
            "Destructuring NamedTuple should succeed: {:?}",
            result.error
        );

        // Third evaluation: use the destructured variables
        let result = session.eval("U[1]");
        assert!(
            result.success,
            "Accessing U should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 1.0).abs() < 0.001),
            "Pattern match failed, got {:?}",
            result.value
        );

        let result = session.eval("S[2]");
        assert!(
            result.success,
            "Accessing S should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 4.0).abs() < 0.001),
            "Pattern match failed, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_module_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: define a module with functions
        let result = session.eval(
            r#"
module MyModule

export double, triple

function double(x)
    return x * 2
end

function triple(x)
    return x * 3
end

end
"#,
        );
        assert!(
            result.success,
            "Module definition should succeed: {:?}",
            result.error
        );

        // Second evaluation: use the module (relative import)
        let result = session.eval("using .MyModule");
        assert!(
            result.success,
            "using .MyModule should succeed: {:?}",
            result.error
        );

        // Third evaluation: use the exported function - should work because module persists
        let result = session.eval("double(21)");
        assert!(
            result.success,
            "double(21) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected double(21) to be 42, got {:?}",
            result.value
        );

        // Fourth evaluation: use another exported function
        let result = session.eval("triple(10)");
        assert!(
            result.success,
            "triple(10) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(30))),
            "Expected triple(10) to be 30, got {:?}",
            result.value
        );

        // Fifth evaluation: run completely unrelated code - should NOT try to load MyModule from LOAD_PATH
        // This is the key test for the bug fix - previously it would fail with "module 'MyModule' not found in LOAD_PATH"
        let result = session.eval("x = 1 + 2");
        assert!(
            result.success,
            "Simple expression after module use should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected Some(Value::I64(3)), got {:?}",
            result.value
        );

        // Sixth evaluation: use the module function again after unrelated code
        let result = session.eval("double(100)");
        assert!(
            result.success,
            "double(100) after unrelated code should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(200))),
            "Expected double(100) to be 200, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_split_simple_statements() {
    // Test that simple statements (no block structures) are split correctly
    let session = REPLSession::new(42);

    let src = r#"x = 42
pi_approx = 3.14159
println("x = $(x)")
y = 10
println(y)"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split simple statements");

    let exprs = exprs.unwrap();
    assert_eq!(
        exprs.len(),
        5,
        "Should have 5 top-level expressions, got {:?}",
        exprs
    );

    assert_eq!(exprs[0].2, "x = 42");
    assert_eq!(exprs[1].2, "pi_approx = 3.14159");
    assert_eq!(exprs[2].2, "println(\"x = $(x)\")");
    assert_eq!(exprs[3].2, "y = 10");
    assert_eq!(exprs[4].2, "println(y)");
}

#[test]
fn test_split_with_comments() {
    // Test that comments are handled correctly
    let session = REPLSession::new(42);

    let src = r#"# This is a comment
x = 42
# Another comment
y = 10"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split statements with comments");

    let exprs = exprs.unwrap();
    // Comments on their own lines should be skipped, leaving just the assignments
    assert_eq!(
        exprs.len(),
        2,
        "Should have 2 expressions (comments excluded), got {:?}",
        exprs
    );
    assert_eq!(exprs[0].2, "x = 42");
    assert_eq!(exprs[1].2, "y = 10");
}

#[test]
fn test_split_with_block_comments() {
    // Test that block comments (#= ... =#) are handled correctly
    let session = REPLSession::new(42);

    // Test 1: Simple block comment spanning multiple lines
    let src = r#"#=
==========================================
Welcome message
==========================================
=#
println("Hello, World!")"#;

    let exprs = session.split_expressions(src);
    // Should either return None (single expression) or have 1 expression
    // Since there's only one actual statement, it might return None
    if let Some(exprs) = exprs {
        assert_eq!(
            exprs.len(),
            1,
            "Should have 1 expression after block comment, got {:?}",
            exprs
        );
        assert_eq!(exprs[0].2.trim(), "println(\"Hello, World!\")");
    }

    // Test 2: Multiple expressions with block comment
    let src2 = r#"x = 42
#= This is a block comment
spanning multiple lines =#
y = 10"#;

    let exprs2 = session.split_expressions(src2);
    assert!(
        exprs2.is_some(),
        "Should split statements with block comments"
    );
    let exprs2 = exprs2.unwrap();
    assert_eq!(
        exprs2.len(),
        2,
        "Should have 2 expressions, got {:?}",
        exprs2
    );
    assert_eq!(exprs2[0].2, "x = 42");
    assert_eq!(exprs2[1].2, "y = 10");
}

#[test]
fn test_split_with_nested_block_comments() {
    // Test that nested block comments are handled correctly
    let session = REPLSession::new(42);

    let src = r#"x = 1
#= outer comment
#= nested comment =#
still in outer =#
y = 2"#;

    let exprs = session.split_expressions(src);
    assert!(
        exprs.is_some(),
        "Should split statements with nested block comments"
    );
    let exprs = exprs.unwrap();
    assert_eq!(exprs.len(), 2, "Should have 2 expressions, got {:?}", exprs);
    assert_eq!(exprs[0].2, "x = 1");
    assert_eq!(exprs[1].2, "y = 2");
}

#[test]
fn test_block_comment_eval() {
    run_with_large_stack(|| {
        // Test that code with block comments can be evaluated in REPL
        let mut session = REPLSession::new(42);

        let src = r#"#=
==========================================
Welcome to SubsetJuliaVM!
==========================================
=#

println("Hello, World!")"#;

        let result = session.eval(src);
        assert!(
            result.success,
            "Should successfully evaluate code with block comment: {:?}",
            result.error
        );
        assert!(
            result.output.contains("Hello, World!"),
            "Should output Hello, World!"
        );
    });
}

#[test]
fn test_split_multiline_array() {
    // Test that multi-line array literals are NOT split
    let session = REPLSession::new(42);

    let src = r#"x = [1, 2,
     3, 4]
println(x)"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split after array");

    let exprs = exprs.unwrap();
    assert_eq!(
        exprs.len(),
        2,
        "Should have 2 expressions (array + println)"
    );
    assert!(
        exprs[0].2.contains("[1, 2,"),
        "First should be array literal"
    );
    assert_eq!(exprs[1].2, "println(x)");
}

#[test]
fn test_split_multiline_call_keeps_continuation_lines_together() {
    let session = REPLSession::new(42);

    let src = r#"tableau = YoungTableau([4, 3, 1])
println("tableau[1], tableau[2], tableau[4], tableau[6] = ",
        tableau[1], ", ", tableau[2], ", ", tableau[4], ", ", tableau[6])
println("done")"#;

    let exprs = session.split_expressions(src).unwrap();

    assert_eq!(
        exprs.len(),
        3,
        "should not split a parenthesized call at its continuation newline: {exprs:?}"
    );
    assert_eq!(exprs[0].2, "tableau = YoungTableau([4, 3, 1])");
    assert_eq!(
        exprs[1].2,
        "println(\"tableau[1], tableau[2], tableau[4], tableau[6] = \",\n        tableau[1], \", \", tableau[2], \", \", tableau[4], \", \", tableau[6])"
    );
    assert_eq!(exprs[2].2, "println(\"done\")");
}

#[test]
fn test_split_simple_statements_sequential_eval() {
    run_with_large_stack(|| {
        // Test that simple statements can be evaluated sequentially
        let src = r#"x = 42
y = x + 10
z = x * y"#;

        let mut session = REPLSession::new(42);
        let exprs = session.split_expressions(src).unwrap();

        assert_eq!(exprs.len(), 3);

        // First: x = 42
        let result = session.eval(&exprs[0].2);
        assert!(result.success, "x = 42 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}",
            result.value
        );

        // Second: y = x + 10 (x persists)
        let result = session.eval(&exprs[1].2);
        assert!(result.success, "y = x + 10 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(52))),
            "Expected Some(Value::I64(52)), got {:?}",
            result.value
        );

        // Third: z = x * y (both x and y persist)
        let result = session.eval(&exprs[2].2);
        assert!(result.success, "z = x * y should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(2184))),
            "Expected Some(I64(2184)) (42 * 52), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_plot_returns_plotly_artifact() {
    // Regression for Issue #4361: `using Plots; plot(sin)` returned a bare
    // <struct ref> in the iOS REPL because the plot renderer only matched the
    // unqualified struct names "Plot"/"Series". After `using Plots` the VM
    // stores the qualified names "Plots.Plot"/"Plots.Series", so the
    // auto-display path skipped rendering.
    // Issue #5283: 2D plots now render through Plotly (line → scatter/lines).
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let setup = session.eval("using Plots");
        assert!(setup.success, "`using Plots` failed: {:?}", setup.error);

        let result = session.eval("plot(sin)");
        assert!(result.success, "`plot(sin)` failed: {:?}", result.error);

        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly display artifact for plot(sin)");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains(r#""type":"scatter""#),
            "2D line plot should emit a scatter trace, got: {}",
            &artifact.data[..artifact.data.len().min(120)]
        );
        assert!(
            artifact.data.contains(r#""mode":"lines""#),
            "a line plot should render with mode lines"
        );
    });
}

#[test]
fn test_repl_plot_renders_2d_axes_layout() {
    // Issue #4437 (continued under #5283): default 2D Plots output should carry
    // a flat x/y axis layout — not a 3D `scene` — so Plotly draws planar axes.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let setup = session.eval("using Plots");
        assert!(setup.success, "`using Plots` failed: {:?}", setup.error);

        let result = session.eval("plot([1, 2, 3], [3.0, 2.0, 1.0])");
        assert!(result.success, "`plot(x, y)` failed: {:?}", result.error);

        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly display artifact for plot(x, y)");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains("\"xaxis\"") && artifact.data.contains("\"yaxis\""),
            "2D plot layout should include x/y axes, got: {}",
            &artifact.data[..artifact.data.len().min(240)]
        );
        assert!(
            !artifact.data.contains("\"scene\""),
            "a 2D plot must not use a 3D scene layout"
        );
    });
}

#[test]
fn test_repl_scatter_with_range_x() {
    // Issue: `scatter(1:10, rand(10))` returned a Plot without an artifact
    // because the extractor only handled native arrays, not ranges. Verify
    // Range axes materialize into a Plotly marker scatter (Issue #5283).
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);
        let setup = session.eval("using Plots");
        assert!(setup.success, "`using Plots` failed: {:?}", setup.error);

        let result =
            session.eval("scatter(1:10, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0])");
        assert!(
            result.success,
            "scatter with Range x failed: {:?}",
            result.error
        );

        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly artifact for scatter with Range x");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains(r#""mode":"markers""#),
            "scatter should render with marker mode when x is a Range, got: {}",
            &artifact.data[..artifact.data.len().min(160)]
        );
    });
}

#[test]
fn test_repl_scatter_renders_markers() {
    // Issue #4367 (under #5283): scatter(...) renders discrete markers rather
    // than a connected line.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);
        let setup = session.eval("using Plots");
        assert!(setup.success, "`using Plots` failed: {:?}", setup.error);

        let result = session.eval("scatter(sin)");
        assert!(result.success, "`scatter(sin)` failed: {:?}", result.error);

        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly display artifact for scatter(sin)");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains(r#""mode":"markers""#),
            "scatter should render with marker mode, got: {}",
            &artifact.data[..artifact.data.len().min(120)]
        );
        assert!(
            !artifact.data.contains(r#""mode":"lines""#),
            "scatter should not draw a connected line"
        );
    });
}

#[test]
fn test_repl_surface_matrix_range_globals_returns_plotly_artifact_5987() {
    // Issue #5987: matrix-backed `surface(x, y, z)` in the persistent REPL
    // returned a textual `Plots.Plot(...)` value without the Plotly artifact.
    // The user flow binds x/y/z in prior evals, then returns the Plot.
    // Also guards Issue #5995: matrix comprehension globals must persist across
    // those eval boundaries so `z` is available to the final surface call.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        for setup in ["using LinearAlgebra", "using Plots"] {
            let result = session.eval(setup);
            assert!(result.success, "`{setup}` failed: {:?}", result.error);
        }

        let result = session.eval("x = y = range(-3, stop = 3, length = 4)");
        assert!(
            result.success,
            "chained range assignment failed: {:?}",
            result.error
        );

        let result = session.eval("length(y)");
        assert!(
            result.success,
            "rhs of chained assignment did not persist: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(4))),
            "expected y to persist as a 4-point range, got {:?}",
            result.value
        );

        let result = session.eval("z = [sinc(norm([xi, yi])) for yi in y, xi in x]");
        assert!(
            result.success,
            "surface matrix construction failed: {:?}",
            result.error
        );

        let result = session.eval("surface(x, y, z)");
        assert!(
            result.success,
            "`surface(x, y, z)` failed: {:?}",
            result.error
        );
        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly artifact for REPL surface");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains(r#""type":"surface""#),
            "surface should emit a Plotly surface trace, got: {}",
            artifact.data
        );
        assert!(
            artifact.data.contains("\"scene\""),
            "surface should use a 3D scene layout, got: {}",
            artifact.data
        );
    });
}

#[test]
fn test_repl_surface_inline_lambda_returns_plotly_artifact_6122() {
    // Issue #6122: the inline anonymous function in `surface(..., (x, y) -> ...)`
    // lowers to an internal `__lambda_*` function. The REPL must not treat that
    // compiler-generated function as a user function definition result.
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        for setup in ["using LinearAlgebra", "using Plots"] {
            let result = session.eval(setup);
            assert!(result.success, "`{setup}` failed: {:?}", result.error);
        }

        let result = session.eval("x = y = range(-3, stop = 3, length = 4)");
        assert!(result.success, "range setup failed: {:?}", result.error);

        let result = session.eval("surface(x, y, (x, y) -> sinc(norm([x, y])))");
        assert!(
            result.success,
            "inline-lambda surface failed: {:?}",
            result.error
        );
        assert!(
            !matches!(result.value, Some(Value::Function(ref f)) if f.name.starts_with("__lambda_")),
            "REPL leaked internal lambda value: {:?}",
            result.value
        );
        let artifact = result
            .display_artifact
            .as_ref()
            .expect("expected Plotly artifact for inline-lambda surface");
        assert_eq!(artifact.mime, "application/vnd.plotly+json");
        assert!(
            artifact.data.contains(r#""type":"surface""#),
            "surface should emit a Plotly surface trace, got: {}",
            artifact.data
        );
    });
}

// Issue #5163: Complex globals must persist across REPL evaluations through the
// same generic struct-persistence path Rational uses (StructRef -> struct_instances
// -> struct_instance_to_literal), NOT a Complex-specific (f64, f64) slot. These
// round-trip tests define a Complex global in one eval and read it back in the next.

#[test]
fn test_repl_complex_f64_global_persistence_5163() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let result = session.eval("zf = Complex(1.5, -2.5)");
        assert!(
            result.success,
            "Complex{{Float64}} assignment should succeed: {:?}",
            result.error
        );

        // Re-inject the persisted global and read its real part.
        let result = session.eval("real(zf)");
        assert!(
            result.success,
            "real(zf) should succeed after persistence: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 1.5).abs() < 1e-12),
            "Expected real(zf) == 1.5, got {:?}",
            result.value
        );

        let result = session.eval("imag(zf)");
        assert!(
            result.success,
            "imag(zf) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v + 2.5).abs() < 1e-12),
            "Expected imag(zf) == -2.5, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_complex_int_global_persistence_5163() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let result = session.eval("zi = Complex(3, 4)");
        assert!(
            result.success,
            "Complex{{Int64}} assignment should succeed: {:?}",
            result.error
        );

        // Int Complex parts must round-trip as I64, NOT widen to F64
        // (the deleted (f64, f64) slot would have lost the Int element type).
        let result = session.eval("real(zi)");
        assert!(
            result.success,
            "real(zi) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected real(zi) == I64(3) (type preserved), got {:?}",
            result.value
        );

        let result = session.eval("imag(zi)");
        assert!(
            result.success,
            "imag(zi) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(4))),
            "Expected imag(zi) == I64(4) (type preserved), got {:?}",
            result.value
        );

        // typeof must remain Complex{Int64} after re-injection.
        let result = session.eval("string(typeof(zi))");
        assert!(
            result.success,
            "string(typeof(zi)) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::Str(ref s)) if s == "Complex{Int64}"),
            "Expected typeof(zi) == Complex{{Int64}}, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_complex_f32_global_persistence_5163() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let result = session.eval("z32 = Complex{Float32}(1.5f0, 2.5f0)");
        assert!(
            result.success,
            "Complex{{Float32}} assignment should succeed: {:?}",
            result.error
        );

        // Float32 Complex parts must round-trip as F32, NOT widen to F64.
        let result = session.eval("real(z32)");
        assert!(
            result.success,
            "real(z32) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F32(v)) if (v - 1.5_f32).abs() < 1e-6),
            "Expected real(z32) == F32(1.5) (type preserved), got {:?}",
            result.value
        );

        let result = session.eval("string(typeof(z32))");
        assert!(
            result.success,
            "string(typeof(z32)) should succeed: {:?}",
            result.error
        );
        // typeof renders via the ComplexF32 alias like upstream Julia (Issue #5704).
        assert!(
            matches!(result.value, Some(Value::Str(ref s)) if s == "ComplexF32"),
            "Expected typeof(z32) displayed as ComplexF32, got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_complex_global_reset_clears_5163() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let result = session.eval("zc = Complex(3, 4)");
        assert!(
            result.success,
            "Complex assignment should succeed: {:?}",
            result.error
        );
        // Confirm it persisted.
        let result = session.eval("real(zc)");
        assert!(
            result.success,
            "real(zc) should succeed: {:?}",
            result.error
        );

        // reset() clears all globals — including the Complex one (formerly cleared
        // via the dedicated complex_vars slot, now via the generic struct path).
        session.reset();

        let result = session.eval("real(zc)");
        assert!(
            !result.success,
            "real(zc) must fail after reset (Complex global cleared), got value {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_complex_global_reassignment_replaces_element_type_5163() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(0);

        let result = session.eval("z = Complex(3, 4)");
        assert!(
            result.success,
            "first assignment should succeed: {:?}",
            result.error
        );

        // Reassign to a different element type; the persisted slot must be replaced.
        let result = session.eval("z = Complex{Float32}(7.0f0, 8.0f0)");
        assert!(
            result.success,
            "reassignment to Complex{{Float32}} should succeed: {:?}",
            result.error
        );

        let result = session.eval("string(typeof(z))");
        assert!(
            result.success,
            "string(typeof(z)) should succeed: {:?}",
            result.error
        );
        // typeof renders via the ComplexF32 alias like upstream Julia (Issue #5704).
        assert!(
            matches!(result.value, Some(Value::Str(ref s)) if s == "ComplexF32"),
            "Expected typeof(z) displayed as ComplexF32 after reassignment, got {:?}",
            result.value
        );

        let result = session.eval("real(z)");
        assert!(result.success, "real(z) should succeed: {:?}", result.error);
        assert!(
            matches!(result.value, Some(Value::F32(v)) if (v - 7.0_f32).abs() < 1e-6),
            "Expected real(z) == F32(7.0), got {:?}",
            result.value
        );
    });
}

#[test]
fn test_repl_tuple_valued_globals_persist_8243() {
    // Issue #8243: a tuple-valued REPL global (`tspan = (0.0, 60.0)`) was dropped
    // and the next eval raised `UndefVarError: tspan not defined` (the OrdinaryDiffEq
    // samples). `inject_globals` reconstructed Range/NamedTuple/struct globals but had
    // no bare-`Value::Tuple` case, so tuples — and structs carrying a tuple field, e.g.
    // a JSXGraph `board`'s `xlim`/`ylim` (apollonian_gasket sample) — were lost.
    // value_to_init_expr now rebuilds tuples as `TupleLiteral`.
    run_with_large_stack(|| {
        let mut s = REPLSession::new(0);

        assert!(s.eval("tspan = (0.0, 60.0)").success);
        let r = s.eval("tspan");
        assert!(r.success, "tuple global dropped (#8243): {:?}", r.error);
        let sum = s.eval("tspan[1] + tspan[2]");
        assert!(
            sum.success && sum.error.is_none(),
            "tuple element access failed: {:?}",
            sum.error
        );

        // Nested tuple round-trips.
        assert!(s.eval("nt = ((1, 2), (3, 4))").success);
        assert!(s.eval("nt[2][1]").success, "nested tuple global dropped");

        // A struct carrying a tuple field persists (the gasket `board` shape).
        assert!(s.eval("struct Hold8243; v; end").success);
        assert!(s.eval("h = Hold8243((1.0, 2.0))").success);
        let hr = s.eval("h.v[2]");
        assert!(
            hr.success,
            "struct-with-tuple-field global dropped: {:?}",
            hr.error
        );
    });
}

#[test]
fn test_repl_staticarrays_globals_persist_8249() {
    // Issue #8249: `@SVector`/`@SMatrix` globals are stored as `StaticArrayInline`
    // (not tuples/structs), had no reconstruction in `value_to_init_expr`, and so
    // were dropped — `u = @SVector [1.0, 2.0]` then `u` raised `UndefVarError`.
    // Rebuilt now via `SVector(xs...)` / `SMatrix{M,N}(xs...)`.
    run_with_large_stack(|| {
        let mut s = REPLSession::new(0);
        assert!(s.eval("using StaticArrays").success);

        assert!(s.eval("u = @SVector [3.0, 4.0]").success);
        assert!(s.eval("v = @SVector [1.0, 2.0]").success);
        let r = s.eval("u + v");
        assert!(r.success, "SVector global dropped (#8249): {:?}", r.error);
        // values preserved: (3+1, 4+2) = (4, 6)
        let dot = s.eval("u[1] * v[1] + u[2] * v[2]");
        assert!(
            dot.success && dot.error.is_none(),
            "SVector use failed: {:?}",
            dot.error
        );

        assert!(s.eval("A = @SMatrix [1.0 2.0; 3.0 4.0]").success);
        let am = s.eval("A");
        assert!(am.success, "SMatrix global dropped (#8249): {:?}", am.error);
        // column-major + shape preserved: A[2,1] == 3.0, A[1,2] == 2.0
        let a21 = s.eval("A[2, 1]");
        assert!(
            a21.success && a21.error.is_none(),
            "SMatrix shape lost: {:?}",
            a21.error
        );
        let prod = s.eval("A * v"); // matrix·vector still works after round-trip
        assert!(
            prod.success,
            "SMatrix*SVector failed after round-trip: {:?}",
            prod.error
        );
    });
}

#[test]
fn test_repl_function_valued_globals_persist() {
    // A function/closure stored in a global — directly (`g = sin`) or inside a
    // struct field — reconstructs as a FunctionRef so it survives the next eval.
    // (Previously such a global had no init expr and was dropped.)
    run_with_large_stack(|| {
        let mut s = REPLSession::new(0);
        assert!(s.eval("g = sin").success);
        let r = s.eval("g(0.0)");
        assert!(
            r.success && r.error.is_none(),
            "function global dropped: {:?}",
            r.error
        );

        // A struct carrying a function field, with a round-tripping constructor.
        assert!(s.eval("struct FWrap; f; end").success);
        assert!(s.eval("w = FWrap(cos)").success);
        let wr = s.eval("w.f(0.0)");
        assert!(
            wr.success && wr.error.is_none(),
            "struct-with-function-field global dropped: {:?}",
            wr.error
        );
    });
}

#[test]
fn test_repl_value_carried_global_with_pairs_field_persists_8260() {
    // Issue #8260: a global whose value CANNOT be reconstructed as an init
    // expression (a struct carrying a `Base.Pairs` kwargs container — the shape of
    // an OrdinaryDiffEq `ODEProblem`) was silently dropped, so the next eval raised
    // `UndefVarError`. The robust fix carries the actual runtime `Value` across
    // evals (transplanting the prior struct heap) instead of rebuilding it from an
    // init expr. This synthetic struct reproduces the exact blocker without the
    // heavyweight package: `kwargs` is a `Value::Pairs` with no `value_to_init_expr`.
    run_with_large_stack(|| {
        let mut s = REPLSession::new(0);
        assert!(s.eval("struct Holder8260; data; tag; end").success);
        // `; kw...` collects zero keyword args into an empty Base.Pairs.
        assert!(
            s.eval("makeholder8260(; kw...) = Holder8260(kw, 7)")
                .success
        );
        let pr = s.eval("p = makeholder8260()");
        assert!(pr.success, "Holder8260 build failed: {:?}", pr.error);

        // The next eval must still see `p` (previously dropped by the Pairs field).
        let acc = s.eval("p.tag");
        assert!(
            acc.success && acc.error.is_none(),
            "value-carried global dropped (#8260): {:?}",
            acc.error
        );
        // And the carried value is the real one: p.tag == 7.
        assert!(
            matches!(acc.value, Some(Value::I64(7))),
            "carried struct field wrong: {:?}",
            acc.value
        );
    });
}

#[test]
fn test_repl_odeproblem_global_persists_8260() {
    // Issue #8260 (end-to-end): build an `ODEProblem` on one REPL line, then use it
    // on the next. Previously `prob` was dropped (its `kwargs::Base.Pairs` field has
    // no init-expr) and `prob.tspan` / `solve(prob, …)` raised `UndefVarError`.
    run_with_large_stack(|| {
        let mut s = REPLSession::new(0);
        assert!(s.eval("using OrdinaryDiffEq").success);
        assert!(s.eval("f8260(u, p, t) = 1.01 * u").success);
        assert!(s.eval("u0 = 0.5").success);
        assert!(s.eval("tspan = (0.0, 1.0)").success);
        let pr = s.eval("prob = ODEProblem(f8260, u0, tspan)");
        assert!(pr.success, "ODEProblem build failed: {:?}", pr.error);

        // prob must survive into the next eval and its fields stay intact.
        let acc = s.eval("prob.tspan[2]");
        assert!(
            acc.success && acc.error.is_none(),
            "ODEProblem global dropped (#8260): {:?}",
            acc.error
        );
        assert!(
            matches!(acc.value, Some(Value::F64(v)) if (v - 1.0).abs() < 1e-12),
            "carried prob.tspan[2] wrong: {:?}",
            acc.value
        );

        // ...and it is still usable as a real ODEProblem (`solve`).
        let sol = s.eval("sol = solve(prob, Tsit5(); dt=0.1)");
        assert!(
            sol.success && sol.error.is_none(),
            "solve(prob) after round-trip failed (#8260): {:?}",
            sol.error
        );
    });
}
