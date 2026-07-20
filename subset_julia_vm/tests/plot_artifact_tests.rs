//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod display_plot_artifact_9262 {
    //! Issue #9262: `display(plot(cos))` must render the plot on a graphical host
    //! (iOS/web REPL/Editor) instead of printing the raw `Plot(...)` struct text.
    //!
    //! Upstream shape: `display(x)` walks the display stack and renders `x` on the
    //! first backend that can. SubsetJuliaVM cannot keep a mutable `displays` array
    //! reachable from a prelude function, so the host's rich "graphical display"
    //! lives on the VM: a graphical host calls `Vm::enable_graphical_display()`
    //! before `run()`, and pure-Julia `display(x)` calls the `_display_artifact(x)`
    //! builtin, which — when a graphical display is active — renders `x` through the
    //! SAME `try_value_to_artifact` structural path as the trailing-expression
    //! render, buffers it in the VM display sink, and returns `nothing`. Without a
    //! graphical display (plain CLI script / terminal REPL) `display(x)` falls back
    //! to text, matching a headless Julia session whose display stack holds only a
    //! `TextDisplay`.

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::plotting::try_value_to_artifact;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::Value;

    type RunResult = (
        Value,
        Vec<subset_julia_vm::plotting::DisplayArtifact>,
        String,
        Vec<subset_julia_vm_bytecode::value::StructInstance>,
    );

    /// Run `src`, optionally under a graphical host, returning the result value, the
    /// artifacts emitted by `display(x)` during the run, the captured stdout, and
    /// the VM struct heap (needed to resolve any `StructRef` result value).
    fn run(src: &str, graphical: bool) -> RunResult {
        let program = parse_and_lower(src).expect("parse_and_lower");
        let compiled = compile_with_cache(&program).expect("compile");
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        if graphical {
            vm.enable_graphical_display();
        }
        let value = vm.run().expect("run");
        let output = vm.get_output().to_string();
        let heap = vm.get_struct_heap().to_vec();
        (value, vm.take_display_artifacts(), output, heap)
    }

    /// Under a graphical host, `display(plot(cos))` buffers a Plotly artifact and the
    /// expression evaluates to `nothing` (so no giant struct echo).
    #[test]
    fn display_plot_emits_artifact_under_graphical_host_9262() {
        let (value, artifacts, output, _heap) = run("using Plots; display(plot(cos))", true);

        assert!(
            matches!(value, Value::Nothing),
            "display(plot(cos)) must evaluate to nothing, got {value:?}"
        );
        assert_eq!(
            artifacts.len(),
            1,
            "display(plot(cos)) should emit exactly one display artifact"
        );
        assert_eq!(artifacts[0].mime, "application/vnd.plotly+json");
        assert!(
            artifacts[0].data.contains("\"traces\""),
            "artifact should be a Plotly figure with traces, got: {}",
            &artifacts[0].data[..artifacts[0].data.len().min(200)]
        );
        // The plot is rendered graphically, so it is NOT echoed as text.
        assert!(
            !output.contains("Series"),
            "the plot struct must not be printed as text under a graphical host, got: {output}"
        );
    }

    /// Without a graphical host (CLI script / terminal REPL), `display(plot(cos))`
    /// emits NO artifact and falls back to text output — matching a headless Julia
    /// session. The expression still evaluates to `nothing`.
    #[test]
    fn display_plot_falls_back_to_text_without_graphical_host_9262() {
        let (value, artifacts, output, _heap) = run("using Plots; display(plot(cos))", false);

        assert!(
            matches!(value, Value::Nothing),
            "display(plot(cos)) must evaluate to nothing, got {value:?}"
        );
        assert!(
            artifacts.is_empty(),
            "no display artifact should be emitted without a graphical host"
        );
        assert!(
            output.contains("Series") || output.contains("Plot"),
            "display should print the plot struct text without a graphical host, got: {output}"
        );
    }

    /// A non-renderable value (`display(42)`) under a graphical host is NOT captured
    /// as an artifact and falls back to text (`42`) — the display-stack text path.
    #[test]
    fn display_non_renderable_value_falls_back_to_text_9262() {
        let (value, artifacts, output, _heap) = run("display(42)", true);

        assert!(matches!(value, Value::Nothing));
        assert!(
            artifacts.is_empty(),
            "display(42) must not produce a display artifact"
        );
        assert!(
            output.contains("42"),
            "display(42) should print 42, got: {output}"
        );
    }

    /// Regression guard: a *trailing* `plot(cos)` (no explicit `display`) does NOT
    /// route through the display sink; it stays a renderable result value the host
    /// renders via `try_value_to_artifact`. This keeps the existing trailing-plot
    /// rendering behavior intact (no `display` call, empty sink).
    #[test]
    fn trailing_plot_does_not_use_display_sink_9262() {
        let (value, artifacts, _output, heap) = run("using Plots; plot(cos)", true);

        assert!(
            artifacts.is_empty(),
            "a trailing plot must not populate the display sink"
        );
        // The trailing plot value is itself renderable via the normal path (resolved
        // against the SAME run's struct heap).
        assert!(
            try_value_to_artifact(&value, &heap).is_some(),
            "trailing plot(cos) result should be renderable via try_value_to_artifact"
        );
    }
}

mod plot_artifact_mime_tests {
    //! End-to-end tests that a real Plots program produces the correct display
    //! artifact MIME through `plotting::try_value_to_artifact` — the same path the
    //! C FFI (`compile_and_run_detailed`) feeds to the iOS/Web hosts.
    //!
    //! Regression for Issue #5271 / #5283: every plot — 2D and 3D — must emit
    //! `application/vnd.plotly+json` so the host renders it via Plotly.js. The bug
    //! these guard against was iOS-side, but proving the artifact MIME here pins the
    //! contract the host relies on.
    //!
    //! This mirrors `ffi::compile_and_run_detailed` exactly: `parse_and_lower`
    //! (which resolves `using Plots`) → `compile_with_cache` → run → artifact.

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::plotting::try_value_to_artifact;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::Value;

    fn run_and_get_artifact(source: &str) -> (String, String) {
        let program = parse_and_lower(source).expect("parse_and_lower failed");
        let compiled = compile_with_cache(&program).expect("compile failed");
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result: Value = vm.run().expect("vm run failed");
        let artifact = try_value_to_artifact(&result, vm.get_struct_heap())
            .expect("expected a display artifact for a plot result");
        (artifact.mime, artifact.data)
    }

    fn run_and_get_mime(source: &str) -> String {
        run_and_get_artifact(source).0
    }

    #[test]
    fn test_plot_xyz_3d_line_emits_plotly_mime() {
        let src = r#"
    using Plots
    x = [0.0, 1.0, 2.0]
    y = [0.0, 1.0, 0.0]
    z = [0.0, 1.0, 2.0]
    plot(x, y, z)
    "#;
        assert_eq!(run_and_get_mime(src), "application/vnd.plotly+json");
    }

    #[test]
    fn test_scatter_xyz_3d_emits_plotly_mime() {
        let src = r#"
    using Plots
    x = [0.0, 1.0, 2.0]
    y = [0.0, 1.0, 0.0]
    z = [0.0, 1.0, 2.0]
    scatter(x, y, z)
    "#;
        assert_eq!(run_and_get_mime(src), "application/vnd.plotly+json");
    }

    #[test]
    fn test_plot_xy_2d_emits_plotly_mime() {
        // Issue #5283: 2D plots now also render via Plotly (line → scatter/lines).
        let src = r#"
    using Plots
    x = [0.0, 1.0, 2.0]
    y = [0.0, 1.0, 0.0]
    plot(x, y)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"lines""#),
            "2D line plot should be a scatter/lines trace; got: {data}"
        );
        assert!(
            !data.contains("\"scene\""),
            "2D plot must not use a 3D scene layout; got: {data}"
        );
    }

    #[test]
    fn test_ordinarydiffeq_plot_solution_emits_plotly_mime_7364() {
        let src = r#"
    using OrdinaryDiffEq
    using Plots
    f(u, p, t) = 1.01 * u
    prob = ODEProblem(f, 0.5, (0.0, 1.0))
    sol = solve(prob, Tsit5(); dt=0.1)
    plot(sol)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"lines""#),
            "plot(sol) should render as a 2D line trace; got: {data}"
        );
    }

    #[test]
    fn test_ordinarydiffeq_plot_solution_idxs_3d_emits_plotly_mime_7364() {
        let src = r#"
    using OrdinaryDiffEq
    using Plots
    function lorenz!(du, u, p, t)
        du[1] = 10.0 * (u[2] - u[1])
        du[2] = u[1] * (28.0 - u[3]) - u[2]
        du[3] = u[1] * u[2] - (8 / 3) * u[3]
    end
    prob = ODEProblem(lorenz!, [1.0, 0.0, 0.0], (0.0, 0.2))
    sol = solve(prob, Tsit5(); dt=0.01)
    plot(sol, idxs=(1, 2, 3))
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"scatter3d""#) && data.contains("\"scene\""),
            "plot(sol, idxs=(1,2,3)) should render as a 3D Plotly path; got: {data}"
        );
    }

    #[test]
    fn test_push_based_3d_animation_frames_are_3d_and_nonempty_8214() {
        // Issue #8214 follow-up: a `plot3d(1)` + `push!` + `@animate` 3D animation
        // (the Aizawa / Lorenz attractor iOS samples) rendered as an EMPTY 2D figure
        // in the REPL. `@animate` snapshots `frame(_anim) == frame(_anim, current())`,
        // but `push!(plt, x, y, z)` mutated `plt` without updating the global
        // `current()` holder, so every captured frame was the empty pre-push plot:
        // the artifact came back with `"traces":[]`, no `"scatter3d"`, and a 2D
        // xaxis/yaxis layout instead of a 3D `"scene"`. `push!` now re-syncs
        // `current()` to the mutated plot, so the frames carry the accumulated 3D path.
        let src = r#"
    using Plots
    plt = plot3d(1, title="Spiral")
    anim = @animate for i in 1:6
        push!(plt, cos(Float64(i)), sin(Float64(i)), Float64(i))
    end every 2
    gif(anim)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        // The growing-path animation now ships the compact schema (Issue #9206); a
        // non-prefix animation would fall back to full `frames`. Accept either.
        assert!(
            data.contains("\"framesCompact\"") || data.contains("\"frames\""),
            "gif(anim) must emit a Plotly animation (compact or full); got: {data}"
        );
        // The bug rendered a 2D, empty animation: assert it is genuinely 3D...
        assert!(
            data.contains(r#""type":"scatter3d""#) && data.contains("\"scene\""),
            "a push!-based plot3d animation must render as 3D scatter3d traces in a \
             scene layout, not an empty 2D figure (Issue #8214); got: {data}"
        );
        // ...and that no frame is empty (initial trace, full arrays, and per-frame data
        // must carry the path — the regression produced `"traces":[]` / `"data":[]` /
        // empty coordinate arrays / an empty compact `full`).
        assert!(
            !data.contains(r#""traces":[]"#)
                && !data.contains(r#""data":[]"#)
                && !data.contains(r#""x":[]"#)
                && !data.contains(r#""full":[]"#),
            "every animation frame must carry the accumulated 3D path, not empty data \
             (Issue #8214); got: {data}"
        );
    }

    #[test]
    fn test_growing_path_animation_uses_compact_schema_9206() {
        // Issue #9206: a `plot3d(1)` + `push!` + `@animate` animation snapshots the
        // *cumulative* path in every frame. The full per-frame `frames` encoding is
        // O(frames²) in size (~61 MB for the 9000-step Aizawa sample → iOS OOM). Since
        // every frame is an exact prefix of the final path, the artifact instead carries
        // the full arrays once (`framesCompact.full`) plus per-frame point `counts`; the
        // host expands them back to identical Plotly frames.
        let src = r#"
    using Plots
    plt = plot3d(1, title="Spiral")
    anim = @animate for i in 1:20
        push!(plt, cos(Float64(i)), sin(Float64(i)), Float64(i))
    end every 5
    gif(anim)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains("\"framesCompact\"")
                && data.contains("\"full\"")
                && data.contains("\"counts\""),
            "a growing-path animation must use the compact framesCompact schema; got: {data}"
        );
        assert!(
            !data.contains("\"frames\""),
            "the compact schema must NOT also embed the O(frames²) full `frames`; got: {data}"
        );
        assert!(
            data.contains(r#""type":"scatter3d""#) && data.contains("\"scene\""),
            "the compact 3D animation must still be scatter3d in a scene layout; got: {data}"
        );
    }

    #[test]
    fn test_nonprefix_animation_falls_back_to_full_frames_9206() {
        // Issue #9206: an animation whose frames are NOT growing prefixes (each frame
        // rebuilds a fresh 2D plot with different data) cannot use the compact encoding,
        // so it falls back to the full per-frame `frames` schema.
        let src = r#"
    using Plots
    anim = @animate for i in 1:3
        plot([0.0, Float64(i)], [Float64(i), 0.0])
    end
    gif(anim)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains("\"frames\""),
            "a non-prefix animation must use the full frames schema; got: {data}"
        );
        assert!(
            !data.contains("\"framesCompact\""),
            "a non-prefix animation must not be compacted; got: {data}"
        );
    }

    #[test]
    fn test_histogram_emits_plotly_bar_trace() {
        let src = r#"
    using Plots
    histogram([1, 2, 1, 1, 4, 3, 8], bins=0:8)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"bar""#),
            "histogram should render as a Plotly bar trace; got: {data}"
        );
        assert!(
            data.contains(r#""y":[0,3,1,1,1,0,0,1]"#),
            "histogram bin counts were not preserved in Plotly JSON; got: {data}"
        );
    }

    #[test]
    fn test_bar_emits_plotly_bar_trace_6358() {
        let src = r#"
    using Plots
    bar([1, 2, 3], [4, 5, 6], fillcolor=[:red, :green, :blue], fillalpha=[0.2, 0.4, 0.6])
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"bar""#),
            "bar should render as a Plotly bar trace; got: {data}"
        );
        assert!(
            data.contains(r#""x":[1,2,3]"#) && data.contains(r#""y":[4,5,6]"#),
            "bar x/y data were not preserved in Plotly JSON; got: {data}"
        );
    }

    #[test]
    fn test_histogram_weights_wrapper_emits_plotly_bar_trace_6451() {
        let src = r#"
    using Plots
    histogram([1, 2, 1, 1, 4, 3, 8], bins=0:8, weights=weights([4, 7, 3, 9, 12, 2, 6]))
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"bar""#),
            "weighted histogram should render as a Plotly bar trace; got: {data}"
        );
        assert!(
            data.contains(r#""y":[0,16,7,2,12,0,0,6]"#),
            "weighted histogram bin counts were not preserved in Plotly JSON; got: {data}"
        );
    }

    // --- Issue #5271 follow-up: broadcast / range-backed arrays were dropped ---
    // `cos.(0:0.01:π)` produces the Pure-Julia `Array{T}` wrapper struct (Memory
    // buffer), not the `NativeArray` carrier that `rand(10)` yields. The plotting
    // extractors previously only handled `NativeArray`/`Range`, so these plots came
    // out blank for both 2D and 3D. These guard the fix.

    #[test]
    fn test_3d_plot_with_broadcast_and_range_coords_emits_plotly() {
        // The exact user-reported case: x,y are broadcast results, z is a Range.
        let src = r#"
    using Plots
    t = 0:0.01:π
    x = cos.(t)
    y = sin.(t)
    z = t
    plot(x, y, z)
    "#;
        assert_eq!(run_and_get_mime(src), "application/vnd.plotly+json");
    }

    #[test]
    fn test_2d_plot_with_broadcast_coords_emits_plotly() {
        let src = r#"
    using Plots
    t = 0:0.01:π
    plot(cos.(t), sin.(t))
    "#;
        assert_eq!(run_and_get_mime(src), "application/vnd.plotly+json");
    }

    #[test]
    fn test_2d_plot_aspect_ratio_equal_emits_plotly_axis_lock_6353() {
        let src = r#"
    using Plots
    plot(sin, aspect_ratio=:equal)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""scaleanchor":"x""#) && data.contains(r#""scaleratio":1"#),
            "aspect_ratio=:equal should lock 2D Plotly axis scaling; got: {data}"
        );
    }

    #[test]
    fn test_2d_plot_aspect_ratio_alias_emits_plotly_axis_lock_6353() {
        let src = r#"
    using Plots
    plot([0.0, 1.0], [0.0, 2.0], ratio=2)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""scaleanchor":"x""#) && data.contains(r#""scaleratio":2"#),
            "ratio=2 should map to aspect_ratio and set Plotly scaleratio; got: {data}"
        );
    }

    #[test]
    fn test_surface_matrix_z_orientation_is_row_major_by_y() {
        // Plots.jl convention: size(z) == (length(y), length(x)), i.e. z[iy, ix].
        // VM stores column-major, so the extractor must transpose-on-read. With
        // x=[10,20], y=[1,2,3], z[iy,ix] = y*100 + x, Plotly's row-major z must be
        // [[110,120],[210,220],[310,320]] (each inner row is a fixed y across x).
        let src = r#"
    using Plots
    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    Z = [y * 100 + x for y in ys, x in xs]
    surface(xs, ys, Z)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains("[[110,120],[210,220],[310,320]]"),
            "surface z orientation wrong; got: {data}"
        );
    }

    #[test]
    fn test_surface_function_z_samples_grid_5986() {
        let src = r#"
    using Plots
    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    surface(xs, ys, (x, y) -> y * 100 + x)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains("[[110,120],[210,220],[310,320]]"),
            "function-valued surface z should be sampled row-major by y; got: {data}"
        );
    }

    #[test]
    fn test_jsxgraph_board_emits_jsxgraph_json_mime_6357() {
        let src = r#"
    using JSXGraph
    b = board("box"; xlim=(-5, 5), ylim=(-5, 5))
    A = point(1, 2; name="A")
    B = point(-3, -1; name="B")
    l = line(A, B)
    push!(b, A, B, l)
    html(b)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.jsxgraph+json");
        assert!(
            data.contains(r#""type":"point""#),
            "JSXGraph JSON should contain a point element; got: {data}"
        );
        assert!(
            data.contains(r#""type":"line""#),
            "JSXGraph JSON should contain a line element; got: {data}"
        );
        assert!(
            data.contains(r#"{"ref":"#),
            "line parents should reference point by id; got: {data}"
        );
        assert!(
            data.contains(r#""boundingbox":[-5.0,5.0,5.0,-5.0]"#),
            "board options should include boundingbox; got: {data}"
        );
    }

    #[test]
    fn test_jsxgraph_circle_with_coordinate_center_emits_array_parent() {
        // The Apollonian-gasket sample passes each circle's center as a tuple of
        // coordinates (not a point element), so the JSON `parents` for a circle must
        // be `[[x, y], r]`. Guards the coordinate-array center path through
        // `value_to_jsx_parent` (Value::Tuple -> JSON array).
        let src = r#"
    using JSXGraph
    struct Circ
        bend::Float64
        bz::Complex{Float64}
    end
    ccenter(c::Circ) = c.bz / c.bend
    cradius(c::Circ) = 1.0 / abs(c.bend)
    function partner(c1::Circ, c2::Circ, c3::Circ, c4::Circ)
        Circ(2.0 * (c1.bend + c2.bend + c3.bend) - c4.bend,
             2.0 * (c1.bz + c2.bz + c3.bz) - c4.bz)
    end
    c0 = Circ(-1.0, Complex(0.0, 0.0))
    c1 = Circ(2.0, Complex(-1.0, 0.0))
    c2 = Circ(2.0, Complex(1.0, 0.0))
    c3 = Circ(3.0, Complex(0.0, 2.0))
    b = board(; xlim=(-1.05, 1.05), ylim=(-1.05, 1.05), axis=false, grid=false)
    for c in (c0, c1, c2, c3)
        z = ccenter(c)
        push!(b, circle((real(z), imag(z)), cradius(c)))
    end
    html(b)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.jsxgraph+json");
        assert!(
            data.contains(r#""type":"circle""#),
            "JSXGraph JSON should contain a circle element; got: {data}"
        );
        // Center passed as a tuple must serialize to a nested coordinate array,
        // e.g. the radius-1/2 circle at (-1/2, 0): parents == [[-0.5, 0.0], 0.5].
        assert!(
            data.contains(r#""parents":[[-0.5,0.0],0.5]"#),
            "circle center tuple should serialize as [[x,y],r]; got: {data}"
        );
    }

    #[test]
    fn test_jsxgraph_view3d_emits_nested_elements_and_jsfunc_7374() {
        let src = r#"
    using JSXGraph
    b = board(; xlim=(-5, 5), ylim=(-5, 5)) do board_ref
        v = view3d([-4.0, -3.0], [8.0, 8.0],
                   Any[Any[-2.0, 2.0], Any[-2.0, 2.0], Any[-2.0, 2.0]])
        c = curve3d("2*Math.sin(3*t)", "2*Math.sin(4*t)", "2*Math.sin(5*t)",
                    [0.0, 6.283185307179586])
        push!(v, c)
        push!(board_ref, v)
    end
    html(b)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.jsxgraph+json");

        let json: serde_json::Value =
            serde_json::from_str(&data).expect("JSXGraph artifact should be valid JSON");
        let elements = json["elements"]
            .as_array()
            .expect("board elements should be an array");
        assert_eq!(elements.len(), 1, "expected one top-level view3d: {data}");
        assert_eq!(elements[0]["type"], "view3d");

        let nested = elements[0]["elements"]
            .as_array()
            .expect("view3d should carry nested elements");
        assert_eq!(nested.len(), 1, "expected one nested curve3d: {data}");
        assert_eq!(nested[0]["type"], "curve3d");

        let parents = nested[0]["parents"]
            .as_array()
            .expect("curve3d parents should be an array");
        assert_eq!(parents[0]["jsfunc"], "2*Math.sin(3*t)");
        assert_eq!(parents[0]["var"], "t");
        assert!(
            parents[0].as_str().is_none(),
            "JSFunction must be structured JSON, not a string literal: {data}"
        );
    }

    #[test]
    fn test_plots_torus_wireframe_rings_emit_plotly_3d() {
        // The "Torus (Plots.jl)" sample: many plot3d! rings accumulate into one
        // figure, and the program ends with `current()` so the final value is the
        // Plot (a trailing `for` loop returns `nothing` and would emit no artifact).
        // Ranges are bound to variables before the for-heads because an inline
        // integer-start, float-step range iterates zero times (Issue #7800).
        let src = r#"
    using Plots
    R = 2.0
    r = 0.7
    us = 0:(2π/24):2π
    vs = 0:(2π/24):2π
    uring = 0:(2π/12):2π
    vring = 0:(2π/12):2π
    first = true
    for u in uring
        x = (R .+ r .* cos.(vs)) .* cos(u)
        y = (R .+ r .* cos.(vs)) .* sin(u)
        z = r .* sin.(vs)
        if first
            plot3d(x, y, z)
            global first = false
        else
            plot3d!(x, y, z)
        end
    end
    for v in vring
        x = (R + r * cos(v)) .* cos.(us)
        y = (R + r * cos(v)) .* sin.(us)
        z = fill(r * sin(v), length(us))
        plot3d!(x, y, z)
    end
    current()
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        // A 3D figure must use a scene layout, and every ring is a scatter3d trace.
        assert!(
            data.contains("\"scene\""),
            "torus rings should render in a 3D scene layout; got: {data}"
        );
        // 13 meridian + 13 longitude rings = 26 path3d traces.
        let trace_count = data.matches(r#""type":"scatter3d""#).count();
        assert_eq!(
            trace_count, 26,
            "expected one scatter3d trace per ring (26 total); got {trace_count} in: {data}"
        );
    }

    #[test]
    fn test_jsxgraph_parametricsurface3d_emits_two_arg_jsfuncs() {
        // A torus surface (the JSXGraph parametricsurface3d example): the three
        // coordinate maps are FX(u,v), FY(u,v), FZ(u,v), so each JSFunction parent
        // must serialize with a two-element `vars` array `["u","v"]` (not the
        // single-arg `var` used by curve3d), followed by the u and v ranges.
        let src = r#"
    using JSXGraph
    b = board(; xlim=(-5, 5), ylim=(-5, 5)) do board_ref
        v = view3d([-4.0, -3.0], [8.0, 8.0],
                   Any[Any[-4.0, 4.0], Any[-4.0, 4.0], Any[-2.0, 2.0]])
        s = parametricsurface3d(
            "(2.5 + Math.cos(v)) * Math.cos(u)",
            "(2.5 + Math.cos(v)) * Math.sin(u)",
            "Math.sin(v)",
            [0.0, 6.283185307179586],
            [0.0, 6.283185307179586])
        push!(v, s)
        push!(board_ref, v)
    end
    html(b)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.jsxgraph+json");

        let json: serde_json::Value =
            serde_json::from_str(&data).expect("JSXGraph artifact should be valid JSON");
        let nested = json["elements"][0]["elements"]
            .as_array()
            .expect("view3d should carry nested elements");
        assert_eq!(nested[0]["type"], "parametricsurface3d");

        let parents = nested[0]["parents"]
            .as_array()
            .expect("parametricsurface3d parents should be an array");
        assert_eq!(parents[0]["jsfunc"], "(2.5 + Math.cos(v)) * Math.cos(u)");
        assert_eq!(
            parents[0]["vars"],
            serde_json::json!(["u", "v"]),
            "coordinate maps must be two-argument (u, v) functions; got: {data}"
        );
        assert!(
            parents[0].get("var").is_none(),
            "a multi-argument JSFunction must use `vars`, not the single-arg `var`: {data}"
        );
        // The two parameter ranges follow the three coordinate maps.
        assert_eq!(parents[3], serde_json::json!([0.0, std::f64::consts::TAU]));
        assert_eq!(parents[4], serde_json::json!([0.0, std::f64::consts::TAU]));
    }

    // --- Issue #7592: a board carrying a 3D view must rotate (not pan) on a
    // single-finger touch drag. JSXGraph defaults the board to single-finger
    // origin-move (pan.needTwoFingers=false); that sets BOARD_MODE_MOVE_ORIGIN on
    // pointerdown and blocks the View3D rotation handler, which only fires while
    // board.mode === BOARD_MODE_NONE. The emitter must therefore mark the board's
    // pan as two-finger so the single-finger drag is free to rotate the view.

    #[test]
    fn test_jsxgraph_view3d_board_requires_two_finger_pan_for_rotation_7592() {
        // The exact MWE from Issue #7592.
        let src = r#"
    using JSXGraph
    b = board("lissajous3d", xlim=(-8, 8), ylim=(-8, 8), axis=false) do b
        v = view3d([-6, -3], [8, 8], [[-3, 3], [-3, 3], [-3, 3]]) do v
            push!(v, curve3d(
                "2*Math.sin(3*t)", "2*Math.sin(2*t)", "2*Math.sin(5*t)",
                [0.0, 6.283185307179586];
                strokeColor="crimson", strokeWidth=2,
            ))
        end
        push!(b, v)
    end
    b
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.jsxgraph+json");

        let json: serde_json::Value =
            serde_json::from_str(&data).expect("JSXGraph artifact should be valid JSON");
        assert_eq!(
            json["options"]["pan"]["needTwoFingers"],
            serde_json::Value::Bool(true),
            "board with a view3d must require two-finger pan so a single-finger \
             drag rotates the 3D view: {data}"
        );
    }

    #[test]
    fn test_jsxgraph_2d_board_keeps_default_single_finger_pan_7592() {
        // A 2D board has no view3d, so single-finger pan must stay the default
        // (the emitter must not inject `pan`).
        let src = r#"
    using JSXGraph
    b = board(; xlim=(-5, 5), ylim=(-5, 5))
    b
    "#;
        let (_mime, data) = run_and_get_artifact(src);
        let json: serde_json::Value =
            serde_json::from_str(&data).expect("JSXGraph artifact should be valid JSON");
        assert!(
            json["options"].get("pan").is_none(),
            "2D board must not have pan injected (single-finger pan stays): {data}"
        );
    }

    // --- Issue #6355: @animate / @gif produce a Plotly frames animation ---
    // The `AnimatedGif` value must render to the same `application/vnd.plotly+json`
    // MIME, but with a `frames` array (one entry per loop iteration) plus Play/Pause
    // `updatemenus` and a `slider`, so the host auto-plays the animation. The static
    // trace path is unchanged; the host branches on the presence of `frames`.

    #[test]
    fn test_animate_gif_emits_plotly_frames_animation_6355() {
        let src = r#"
    using Plots
    p = plot(1)
    @gif for x = 0:0.1:5
        push!(p, 1, sin(x))
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        // This is a growing 2D line (`push!` extends one series), so it now ships the
        // compact schema (Issue #9206) rather than full per-frame `frames`.
        assert!(
            data.contains(r#""framesCompact""#) && !data.contains(r#""frames":["#),
            "@gif growing animation must carry a compact Plotly animation; got: {data}"
        );
        assert!(
            data.contains(r#""updatemenus""#) && data.contains(r#""sliders""#),
            "animation should expose Play/Pause buttons and a frame slider; got: {data}"
        );
        assert!(
            data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"lines""#),
            "each frame should render the line series as a scatter/lines trace; got: {data}"
        );
        // 0:0.1:5 has 51 elements → 51 frames; the slider emits one `"args":[["i"]`
        // step per frame (Play uses `[null`, Pause uses `[[null`, so neither matches
        // the trailing quote). Escaped literal so the closing `"` is part of the needle.
        let frame_count = data.matches("\"args\":[[\"").count();
        assert_eq!(
            frame_count, 51,
            "expected 51 animation frames (one slider step per iteration); got {frame_count} in: {data}"
        );
        assert!(
            !data.contains("\"scene\""),
            "a 2D animation must not use a 3D scene layout; got: {data}"
        );
    }

    #[test]
    fn test_gif_per_frame_title_7030() {
        // Issue #7030: `plot(...; title=...)` inside `@gif` produces a per-frame title
        // (each frame's layout carries its own title), so animations can label frames.
        let src = r#"
    using Plots
    x = [1.0, 2.0, 3.0]
    @gif for t in 1:3
        plot(x, x .+ t, title="t=$t")
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""frames":["#),
            "title animation must carry a frames array; got: {data}"
        );
        assert!(
            data.contains(r#""title":{"text":"t=1""#),
            "frame 1 should carry title t=1; got: {data}"
        );
        assert!(
            data.contains(r#""title":{"text":"t=2""#),
            "frame 2 should carry title t=2; got: {data}"
        );
        assert!(
            data.contains(r#""title":{"text":"t=3""#),
            "frame 3 should carry title t=3; got: {data}"
        );
    }

    #[test]
    fn test_gif_untitled_frame_clears_prior_title_7030() {
        // PR #7132 review: when an animation mixes titled and untitled frames, the
        // untitled frame must emit an explicit empty title so Plotly clears the prior
        // frame's title instead of retaining it.
        let src = r#"
    using Plots
    @gif for t in 1:2
        if t == 1
            plot([1.0, 2.0], [1.0, 2.0], title="first")
        else
            plot([1.0, 2.0], [2.0, 3.0])
        end
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""title":{"text":"first""#),
            "first frame should carry its title; got: {data}"
        );
        assert!(
            data.contains(r#""title":{"text":""#) && data.contains(r#""text":"""#),
            "the untitled frame must emit an explicit empty title to clear the prior one; got: {data}"
        );
    }

    #[test]
    fn test_plot_title_static_7030() {
        // Issue #7030: a static `plot(...; title=...)` carries the title in its layout.
        let src = r#"
    using Plots
    plot([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], title="hello")
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""title":{"text":"hello""#),
            "static plot title missing; got: {data}"
        );
    }

    // --- Issue #7275: Interact `@manipulate` produces a Plotly dropdown figure ---
    // The MVP evaluates the body once per discrete choice and combines the per-choice
    // Plots into ONE static figure: every choice's traces are emitted up-front (choice 0
    // visible, the rest hidden) plus a `"type":"dropdown"` `updatemenus` whose buttons
    // toggle the `visible` arrays. Same `application/vnd.plotly+json` MIME; the host
    // renders it as a normal static figure with a working dropdown (no reactive runtime).

    #[test]
    fn test_manipulate_emits_plotly_dropdown_7275() {
        // The Issue #7275 example, adapted to the vector-data form sjulia's `scatter`
        // supports (whole-matrix scatter / column slicing are separate Plots gaps).
        let src = r#"
    using Interact, Plots
    datasets = Dict(:some => [1.0, 4.0, 9.0, 16.0], :other => [2.0, 3.0, 5.0, 7.0])
    @manipulate for dataset = [:some, :other]
        scatter(datasets[dataset])
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"dropdown""#),
            "@manipulate must emit a Plotly dropdown updatemenus; got: {data}"
        );
        assert!(
            data.contains(r#""updatemenus""#),
            "@manipulate figure should carry an updatemenus entry; got: {data}"
        );
        // An array control stays a dropdown, not a slider (Issue #7338).
        assert!(
            !data.contains(r#""sliders":["#),
            "an array-driven @manipulate must NOT emit a slider; got: {data}"
        );
        // One dropdown button per choice (each an `update` restyle/relayout).
        let button_count = data.matches(r#""method":"update""#).count();
        assert_eq!(
            button_count, 2,
            "expected one dropdown button per choice; got {button_count} in: {data}"
        );
        // Labels come from `string(choice)`.
        assert!(
            data.contains(r#""label":"some""#) && data.contains(r#""label":"other""#),
            "dropdown buttons should be labelled by each choice; got: {data}"
        );
        // First choice's traces visible, second choice's hidden by default.
        assert!(
            data.contains(r#""visible":true"#) && data.contains(r#""visible":false"#),
            "choice 0 traces visible, others hidden by default; got: {data}"
        );
        // Each choice is a scatter (markers) series.
        assert!(
            data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"markers""#),
            "each choice should render its scatter series; got: {data}"
        );
        // A manipulate figure is static (no animation frames).
        assert!(
            !data.contains(r#""frames":["#),
            "a @manipulate figure must not be an animation; got: {data}"
        );
        assert!(
            !data.contains("\"scene\""),
            "a 2D manipulate figure must not use a 3D scene layout; got: {data}"
        );
    }

    #[test]
    fn test_manipulate_multiple_controls_emit_combined_dropdown_7344() {
        // `@manipulate for a = …, b = …` (Issue #7344): with no reactive runtime, the
        // cartesian product of choices renders as ONE combined dropdown — one button per
        // (a, b) combination, labelled `a=<va>, b=<vb>`.
        let src = r#"
    using Interact, Plots
    @manipulate for a = 1:2, b = 1:3
        plot([1.0, 2.0], [a * 1.0, b * 1.0])
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"dropdown""#),
            "multi-control @manipulate must emit a combined dropdown; got: {data}"
        );
        // One button per cartesian-product combination (2 × 3 = 6).
        let button_count = data.matches(r#""method":"update""#).count();
        assert_eq!(
            button_count, 6,
            "expected one dropdown button per (a,b) combination; got {button_count} in: {data}"
        );
        // Combined labels carry both variables.
        assert!(
            data.contains(r#""label":"a=1, b=1""#) && data.contains(r#""label":"a=2, b=3""#),
            "dropdown buttons should be labelled by the combination; got: {data}"
        );
        // No slider for the combined product, and not an animation.
        assert!(
            !data.contains(r#""sliders":["#),
            "multi-control @manipulate must not emit a slider; got: {data}"
        );
        assert!(
            !data.contains(r#""frames":["#),
            "multi-control @manipulate must not be an animation; got: {data}"
        );
    }

    #[test]
    fn test_manipulate_range_choices_emit_slider_7338() {
        // A range control (`for n = 1:3`) maps to a continuous slider, not a dropdown
        // (upstream `widget()` `AbstractRange → slider`, Issue #7338). Each range element
        // becomes a slider step that toggles its choice's trace group `visible`.
        let src = r#"
    using Interact, Plots
    @manipulate for n = 1:3
        plot([1.0, 2.0, 3.0], [n, 2.0 * n, 3.0 * n])
    end
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        // Range → slider: a `sliders` layout entry, not a dropdown `updatemenus`.
        assert!(
            data.contains(r#""sliders":["#),
            "range-driven @manipulate must emit a Plotly slider; got: {data}"
        );
        assert!(
            !data.contains(r#""type":"dropdown""#),
            "range-driven @manipulate must NOT emit a dropdown; got: {data}"
        );
        assert!(
            data.contains(r#""steps":["#),
            "slider should carry steps; got: {data}"
        );
        // One slider step per range element, each an `update` restyle/relayout.
        let step_count = data.matches(r#""method":"update""#).count();
        assert_eq!(
            step_count, 3,
            "expected one slider step per range element; got {step_count} in: {data}"
        );
        assert!(
            data.contains(r#""label":"1""#) && data.contains(r#""label":"3""#),
            "range choices should be labelled by their value; got: {data}"
        );
        // Still a static figure (no animation frames).
        assert!(
            !data.contains(r#""frames":["#),
            "a @manipulate slider figure must not be an animation; got: {data}"
        );
    }

    #[test]
    fn test_heatmap_matrix_emits_plotly_heatmap_trace_6360() {
        let src = r#"
    using Plots
    heatmap([1.0 2.0; 3.0 4.0], aspect_ratio=:equal)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"heatmap""#),
            "heatmap should render as a Plotly heatmap trace; got: {data}"
        );
        assert!(
            data.contains(r#""x":[1,2]"#) && data.contains(r#""y":[1,2]"#),
            "heatmap(z) should use matrix column/row indices for x/y; got: {data}"
        );
        assert!(
            data.contains("[[1,2],[3,4]]"),
            "heatmap z orientation wrong; got: {data}"
        );
        assert!(
            data.contains(r#""scaleanchor":"x""#) && data.contains(r#""scaleratio":1"#),
            "heatmap should preserve 2D aspect_ratio in Plotly layout; got: {data}"
        );
        assert!(
            !data.contains("\"scene\""),
            "heatmap is a 2D trace and must not use a scene layout; got: {data}"
        );
    }

    #[test]
    fn test_contour_matrix_emits_plotly_contour_trace_9940() {
        let src = r#"
    using Plots
    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    contour(xs, ys, (x, y) -> y * 100 + x; levels=4, aspect_ratio=:equal)
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""type":"contour""#),
            "contour should render as a Plotly contour trace; got: {data}"
        );
        assert!(
            data.contains(r#""x":[10,20]"#) && data.contains(r#""y":[1,2,3]"#),
            "contour should preserve explicit x/y axes; got: {data}"
        );
        assert!(
            data.contains("[[110,120],[210,220],[310,320]]"),
            "contour z orientation wrong; got: {data}"
        );
        assert!(
            data.contains(r#""ncontours":6"#),
            "contour levels=4 should map to Plotly ncontours=6; got: {data}"
        );
        assert!(
            data.contains(r#""scaleanchor":"x""#) && data.contains(r#""scaleratio":1"#),
            "contour should preserve 2D aspect_ratio in Plotly layout; got: {data}"
        );
        assert!(
            !data.contains("\"scene\""),
            "contour is a 2D trace and must not use a scene layout; got: {data}"
        );
    }

    #[test]
    fn test_plot_label_kwarg_emits_name_and_enables_legend_7998() {
        let src = r#"
    using Plots
    x = 0:0.1:1
    plot(x, x .^ 2, label="quadratic")
    plot!(x, x .^ 3, label="cubic")
    "#;
        let (mime, data) = run_and_get_artifact(src);
        assert_eq!(mime, "application/vnd.plotly+json");
        assert!(
            data.contains(r#""name":"quadratic""#),
            "plot label must appear as Plotly trace name; got: {data}"
        );
        assert!(
            data.contains(r#""name":"cubic""#),
            "plot! label must appear as Plotly trace name; got: {data}"
        );
        assert!(
            data.contains(r#""showlegend":true"#),
            "labeled plots must enable Plotly legend; got: {data}"
        );
    }
}
