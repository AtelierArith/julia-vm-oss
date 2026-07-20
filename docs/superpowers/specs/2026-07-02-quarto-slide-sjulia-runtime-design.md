# Design: Quarto reveal.js slides with embedded sjulia runtime

**Date:** 2026-07-02  
**Issue:** #8770  
**Status:** Approved for implementation

## 1. Goal

Create a `./slide` directory containing a Quarto reveal.js presentation that embeds the SubsetJuliaVM (sjulia) WASM runtime. During a presentation, the speaker can edit and execute ~5-line Julia code samples on individual slides and visualize the results, including Plotly-based graphs from `using Plots; plot(...)`.

## 2. Background

The existing `./web` Playground already demonstrates the full pipeline:

- `subset_julia_vm_web` (wasm-bindgen) exposes `run_from_source(source, seed)`.
- `scripts/wasm_build_with_cache.sh` builds a WASM artifact with embedded Base bytecode and prelude Program caches.
- `./web/app.js` calls `run_from_source()`, then renders text output or a Plotly JSON artifact.

We reuse that pipeline instead of creating a new runtime binding.

## 3. Scope

### In scope

- Self-contained Quarto reveal.js project under `./slide`.
- Global WASM runtime loaded once per presentation.
- Per-slide executor UI:
  - editable `<textarea>` for Julia code,
  - Run button + Ctrl/Cmd+Enter shortcut,
  - text output area,
  - Plotly graph output area.
- Offline-capable asset bundle (WASM, JS, CSS, Plotly).
- Sample slides demonstrating `println` and `using Plots; plot(...)`.
- README with build/serve instructions.

### Out of scope (first version)

- Monaco editor per slide.
- JSXGraph rendering.
- Slide-transition synchronized execution.
- Persistent history or sharing URLs.

## 4. Architecture

```text
Quarto render
     │
     ▼
slide/_site/index.html  ← reveal.js slides
     │
     ├── include-after-body: assets/sjulia-runtime.js
     │       │
     │       ├── imports pkg/subset_julia_vm_web.js
     │       ├── initializes WASM
     │       └── warms up with `using Plots; plot(sin)`
     │
     └── per-slide: assets/slide-executor.js + .css
             │
             ├── finds `.sjulia-executor` containers
             ├── wires Run button / Ctrl+Enter
             ├── calls global `window.sjulia.run_from_source(code, seed)`
             └── renders text / Plotly output
```

## 5. Directory layout

```text
slide/
├── _quarto.yml                   # revealjs project + include-after-body
├── index.qmd                     # title/agenda slide
├── slides/
│   └── demo.qmd                  # executable sample slides
├── assets/
│   ├── sjulia-runtime.js         # WASM loader & warmup
│   ├── slide-executor.js         # per-slide UI logic
│   ├── slide-executor.css        # executor card styling
│   └── plotly.min.js             # copied from ./web/plotly.min.js
├── pkg/                          # wasm-pack output (copied or built here)
│   ├── subset_julia_vm_web.js
│   ├── subset_julia_vm_web_bg.wasm
│   └── ...
└── README.md
```

## 6. Component details

### 6.1 `assets/sjulia-runtime.js`

Responsibilities:

1. Import `./pkg/subset_julia_vm_web.js`.
2. Call the default export to initialize the WASM module.
3. Expose the module as `window.sjulia`.
4. Run `window.sjulia.run_from_source('using Plots\nplot(sin)\n', 42n)` as a warmup.
5. Dispatch a custom event `sjulia:ready` when the runtime is usable.

```javascript
import init, * as sjulia from './pkg/subset_julia_vm_web.js';

async function boot() {
  await init();
  window.sjulia = sjulia;
  try {
    sjulia.run_from_source('using Plots\nplot(sin)\n', 42n);
  } catch (e) {
    console.warn('warmup failed', e);
  }
  window.dispatchEvent(new CustomEvent('sjulia:ready'));
}

boot();
```

### 6.2 `assets/slide-executor.js`

Responsibilities:

1. Listen for `sjulia:ready`.
2. Find all elements with `data-sjulia-executor`.
3. For each executor:
   - populate textarea with the code from `data-code` or the element's text,
   - enable the Run button,
   - bind Run click and Ctrl/Cmd+Enter,
   - on run: call `window.sjulia.run_from_source(code, 42n)`,
   - route the `ExecutionResult` to text or Plotly output.

Rendering rules (mirroring `./web/app.js`):

- `success === false`: show `error_message` in red.
- `artifact_mime === "application/vnd.plotly+json"`: parse `artifact_data` and call `Plotly.newPlot(container, traces, layout, {responsive: true})`.
- otherwise: show `output` and numeric `value` if non-zero.

### 6.3 `assets/slide-executor.css`

Style the executor card to fit a reveal.js slide:

- fixed maximum height (~60% of slide),
- two-column layout on wide slides (code left, output right),
- stacked layout on narrow slides / mobile,
- dark-friendly colors matching the default reveal.js dark theme.

### 6.4 `slides/demo.qmd`

Use raw HTML blocks to inject executors:

```markdown
## Hello, sjulia

```{=html}
<div class="sjulia-executor" data-code="println(&quot;Hello, World!&quot;)"></div>
```

## Plot example

```{=html}
<div class="sjulia-executor" data-code="using Plots\nx = 0:0.1:2π\ny = sin.(x)\nplot(x, y)"></div>
```
```

A lightweight Quarto shortcode (`julia-live`) could be added later to make authoring cleaner, but raw HTML blocks are sufficient for the first version.

### 6.5 `_quarto.yml`

```yaml
project:
  type: revealjs
  output-dir: _site

revealjs:
  theme: dark
  slide-number: true
  preview-links: auto
  include-after-body:
    - assets/plotly.min.js
    - assets/sjulia-runtime.js
    - assets/slide-executor.js
```

`plotly.min.js` must load before the executor script so that `window.Plotly` is available.

## 7. Build process

1. Build or copy WASM artifacts into `./slide/pkg`.

   ```bash
   # Option A: copy existing web/pkg
   mkdir -p slide/pkg
   cp -R web/pkg/* slide/pkg/

   # Option B: build directly into slide/pkg
   scripts/wasm_build_with_cache.sh --out-dir ./slide/pkg
   ```

2. Copy Plotly bundle.

   ```bash
   cp web/plotly.min.js slide/assets/plotly.min.js
   ```

3. Render slides.

   ```bash
   cd slide
   quarto render
   ```

4. Serve locally.

   ```bash
   cd slide
   quarto preview
   ```

## 8. Error handling

| Scenario | Behavior |
|----------|----------|
| WASM still loading | Run button disabled; label shows "Loading sjulia…" |
| Warmup fails | Log warning; enable Run anyway so the user can see runtime errors |
| Execution error | Display `error_message` in the executor's output area |
| Plotly not loaded | Show text fallback: `[Plotly.js not loaded]` |
| Timeout | Cap execution at 5 s and show timeout message |

## 9. Testing plan

1. Build `./slide/pkg` with `scripts/wasm_build_with_cache.sh --out-dir ./slide/pkg`.
2. Run `quarto render slide/`.
3. Open `slide/_site/index.html` via a local server.
4. Verify:
   - "Hello, World!" slide prints output.
   - Plot slide renders a sine curve.
   - Syntax error slide shows a red error message.

## 10. Acceptance criteria

- [ ] `quarto render slide/` succeeds.
- [ ] Rendered slides load the WASM runtime and reach `sjulia:ready`.
- [ ] Clicking Run on a `println` sample shows the printed text.
- [ ] Clicking Run on a `using Plots; plot(...)` sample renders a Plotly graph.
- [ ] `slide/README.md` documents build and preview commands.

## 11. Related work

- Issue #8770
- `./web/app.js` — executor rendering logic to mirror.
- `./subset_julia_vm_web/src/lib.rs` — `run_from_source` WASM API.
- `./scripts/wasm_build_with_cache.sh` — cached WASM build helper.
