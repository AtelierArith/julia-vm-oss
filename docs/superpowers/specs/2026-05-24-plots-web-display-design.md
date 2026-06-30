# Design: `using Plots; plot(sin)` SVG Display in Web Playground

**Date:** 2026-05-24  
**Status:** Approved

## Summary

Enable the Web Playground (`/web`) to render SVG plots when Julia code produces a `Plots.Plot` value. The infrastructure (plotting module, SVG generation) already exists in the VM and is used by the iOS/CLI REPLs. This feature closes the gap by wiring the WASM return value and the browser display.

## Background

`subset_julia_vm` has a `plotting` module (`src/plotting/`) that converts `Plot` struct values into SVG strings via `try_value_to_artifact`. The REPL session (`src/repl/session.rs`) calls this automatically and exposes the result as `REPLResult::display_artifact`. The WASM entry point (`subset_julia_vm_web/src/lib.rs`) does **not** call this function; `ExecutionResult` has no SVG field. The web UI has no plot display area.

## Goals

- `using Plots; plot(sin)` executed in the Web Playground renders the sine-curve graph in the output area.
- Works for `plot(x, y)`, `scatter(x, y)`, and other forms already supported by the SVG renderer.
- Text output (`println` etc.) still works normally when no plot is produced.
- When a new Run produces no plot, any previous graph is automatically cleared.

## Non-Goals

- Interactive plots (zoom, pan, hover) — static SVG only.
- Changing the SVG renderer quality/style — uses the existing renderer.
- Supporting `plot!()` (mutation) across multiple REPL evaluations — single-shot only.
- Changing iOS or CLI REPL behavior.

## Architecture

### Layer 1 — Rust/WASM (`subset_julia_vm_web/src/lib.rs`)

**Change `ExecutionResult`:**

```rust
#[derive(Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub value: f64,
    pub output: String,
    pub error_message: Option<String>,
    pub svg_artifact: Option<String>,   // NEW: SVG string, present when result is a Plot
}
```

**Change `ExecutionResult::success()`:**

```rust
fn success(value: f64, output: String, svg: Option<String>) -> Self {
    Self { success: true, value, output, error_message: None, svg_artifact: svg }
}
```

(Or add a separate builder method — implementation detail.)

**Change `run_from_source_internal`:**

After `vm.run()` succeeds, call:
```rust
use subset_julia_vm::plotting::try_value_to_artifact;
let svg = try_value_to_artifact(&value, vm.get_struct_heap()).map(|a| a.data);
ExecutionResult::success(f64_value, output, svg)
```

`run_ir_internal` (used by `run_ir_json`) is **not** changed — it remains a pure numeric runner.

### Layer 2 — Web HTML (`web/index.html`)

Add inside `.output-container`, before `<pre id="output">`:

```html
<div id="plot-output" class="hidden"></div>
```

### Layer 3 — Web CSS (`web/styles.css`)

```css
#plot-output {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 1rem;
    background: var(--bg-primary);
}
#plot-output svg {
    max-width: 100%;
    height: auto;
}
#plot-output.hidden {
    display: none;
}
```

### Layer 4 — Web JS (`web/app.js`)

Add `plotOutput` element reference:
```js
const plotOutput = document.getElementById('plot-output');
```

Update `displayResult`:
```js
function displayResult(execResult) {
    // Always reset plot area on each run
    plotOutput.innerHTML = '';
    plotOutput.classList.add('hidden');

    if (execResult.svg_artifact) {
        // Show SVG, hide text output
        plotOutput.innerHTML = execResult.svg_artifact;
        plotOutput.classList.remove('hidden');
        output.classList.add('hidden');
    } else {
        // No plot: show text output as before
        output.classList.remove('hidden');
        if (execResult.success) {
            if (execResult.output) output.textContent += execResult.output;
            if (execResult.value !== 0 && !isNaN(execResult.value)) {
                result.textContent = `Result: ${execResult.value}`;
            } else if (!execResult.output) {
                result.textContent = 'Completed';
            }
        } else {
            if (execResult.output) output.textContent += execResult.output;
            showError(execResult.error_message || 'Execution failed');
        }
    }
}
```

Also update the **Clear button** handler to reset `#plot-output`:
```js
clearOutputBtn.addEventListener('click', () => {
    output.textContent = '';
    output.classList.remove('hidden');
    plotOutput.innerHTML = '';
    plotOutput.classList.add('hidden');
    result.textContent = '';
    hideError();
});
```

### Layer 5 — Samples (`web/samples_ir.js`)

Add a new sample entry for `using Plots; plot(sin)` (and optionally `scatter`).

### Build

After Rust changes:
```bash
cd subset_julia_vm_web
wasm-pack build --target web --out-dir ../web/pkg
```

Bump the cache-busting version on the `app.js` script tag in `index.html` (`?v=51`).

## Data Flow

```
Julia source
    │
    ▼
parse_and_lower()
    │
    ▼
compile_core_program()
    │
    ▼
Vm::run() → Value (may be Plot struct)
    │
    ├── get_output()          → output: String
    ├── get_struct_heap()     → &[StructInstance]
    └── try_value_to_artifact(value, heap) → Option<DisplayArtifact>
                                                │
                                                ▼
                                          ExecutionResult {
                                            svg_artifact: Some("<svg>...")
                                          }
                                                │
                                    serde_wasm_bindgen::to_value()
                                                │
                                                ▼
                                          JavaScript
                                           (app.js)
                                                │
                                     execResult.svg_artifact
                                                │
                                     plotOutput.innerHTML = svg
```

## Testing

- Add a WASM unit test in `subset_julia_vm_web/src/lib.rs`:
  ```rust
  #[test]
  fn test_run_from_source_plot_returns_svg() {
      let source = "using Plots\nplot(sin)\n";
      let result = run_from_source_internal(source, 42);
      assert!(result.success);
      let svg = result.svg_artifact.expect("expected SVG artifact");
      assert!(svg.contains("<svg"), "SVG should start with <svg");
      assert!(svg.contains("<path"), "SVG should contain a path element");
  }
  ```
- Manual browser test: load playground, run `using Plots; plot(sin)`, verify graph appears.
- Run `timeout 1800 cargo nextest run --release` to ensure no regressions.

## Files Changed

| File | Change |
|------|--------|
| `subset_julia_vm_web/src/lib.rs` | Add `svg_artifact` to `ExecutionResult`, call `try_value_to_artifact` |
| `web/index.html` | Add `<div id="plot-output">` |
| `web/styles.css` | Style `#plot-output` |
| `web/app.js` | Update `displayResult`, Clear handler, add `plotOutput` ref |
| `web/samples_ir.js` | Add Plots sample |
