# Design: Monaco Editor for Quarto reveal.js sjulia slides

**Date:** 2026-07-02  
**Issue:** #8770 (follow-up to the initial slide runtime implementation)  
**Status:** Approved for implementation

## 1. Goal

Replace the plain `<textarea>` code editor in each `sjulia-executor` on Quarto reveal.js slides with a Monaco Editor instance. The editor should provide Julia syntax highlighting, a Monokai-inspired dark theme, and the same code-completion features used in the `./web` Playground, including Unicode LaTeX completion powered by the WASM runtime.

## 2. Background

The `./slide` Quarto project currently uses a `<textarea>` for code input. It works but offers no syntax highlighting, autocompletion, or Unicode input assistance. The `./web` Playground already solves this with Monaco Editor loaded from a CDN plus a custom `julia-language.js` module that registers:

- Julia tokenizer and Monokai theme,
- Bracket/comment/indentation rules,
- Unicode completion provider (`\alpha` → `α`) via `wasmModule.unicode_completions()`,
- Keyword/builtin/variable/function completion provider.

We reuse that work instead of re-implementing a language server.

## 3. Scope

### In scope

- Load Monaco Editor from the same CDN version used by `./web` (`0.45.0`).
- Port/adapt `./web/julia-language.js` into `./slide/assets/julia-language.js`.
- Modify `slide/assets/slide-executor.js` to instantiate Monaco per `.sjulia-executor`.
- Preserve existing behavior: Run button, `Ctrl/Cmd+Enter`, text output, Plotly output, 5 s timeout.
- Graceful fallback to `<textarea>` if Monaco fails to load.
- Re-layout Monaco when the slide becomes visible or the window resizes.

### Out of scope (this iteration)

- Bundling Monaco locally for fully offline use (keep CDN to match `./web`).
- JSXGraph rendering in slides.
- Minimap, line numbers, or other Monaco features beyond what `./web` enables.
- Persistent editor state across slide navigation.

## 4. Architecture

```text
Quarto render
     │
     ▼
slide/_site/index.html
     │
     ├── include-after-body: assets/after-body.html
     │       │
     │       ├── <script src="assets/plotly.min.js">
     │       ├── <script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs/loader.js">
     │       ├── <script type="module" src="sjulia-runtime.js">   (sets window.sjulia)
     │       └── <script type="module" src="assets/slide-executor.js">
     │
     └── per-slide: assets/slide-executor.css
             └── .sjulia-executor
                     ├── .sjulia-code      ← Monaco container (was <textarea>)
                     ├── .sjulia-controls  ← Run button
                     ├── .sjulia-output    ← text output
                     └── .sjulia-plot      ← Plotly container
```

`assets/slide-executor.js` bootstraps Monaco once and creates one editor per executor. It also imports `registerJuliaLanguage` and `setWasmModule` from `assets/julia-language.js`.

## 5. Directory layout changes

```text
slide/
├── assets/
│   ├── after-body.html          # add Monaco loader script
│   ├── julia-language.js        # NEW: adapted from ./web/julia-language.js
│   ├── slide-executor.js        # UPDATE: create Monaco editors
│   └── slide-executor.css       # UPDATE: style Monaco container
└── _quarto.yml                  # unchanged
```

## 6. Component details

### 6.1 `assets/after-body.html`

Insert the Monaco loader before the existing ES modules:

```html
<!-- Plotly must load before any AMD-style loader defines `define.amd` -->
<script src="assets/plotly.min.js"></script>
<!-- Monaco Editor loader -->
<script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs/loader.js"></script>
<!-- sjulia WASM runtime and per-slide executor -->
<script type="module" src="sjulia-runtime.js"></script>
<script type="module" src="assets/slide-executor.js"></script>
```

### 6.2 `assets/julia-language.js`

Adapt `./web/julia-language.js`:

- Keep the tokenizer, theme, language configuration, and completion providers.
- Export `registerJuliaLanguage(monaco)` and `setWasmModule(wasm)` as named exports so `slide-executor.js` can use them.
- Remove or weaken Playground-specific assumptions (e.g. module path differences are fine because both use ES modules).

The module signature becomes:

```javascript
export function setWasmModule(wasm) { ... }
export function registerJuliaLanguage(monaco) { ... }
```

### 6.3 `assets/slide-executor.js`

Initialization flow:

1. Wait for `sjulia:ready`.
2. Load Monaco via the global `require` configured to the CDN path.
3. Call `registerJuliaLanguage(monaco)` and `setWasmModule(window.sjulia)`.
4. For each `.sjulia-executor` container:
   - Create a `<div class="sjulia-code">`.
   - Create `monaco.editor.create(container, { language: 'julia', theme: 'julia-monokai', ... })`.
   - Bind `Ctrl/Cmd+Enter` to run.
   - Store the editor instance on the container for later access.
5. Listen to Reveal.js `slidechanged` event and call `editor.layout()` for editors on the newly visible slide.

Fallback:

- If Monaco loader or `require` is missing, create a `<textarea class="sjulia-code">` and keep the current behavior.

Run flow:

- Read code with `editor.getValue()` instead of `textarea.value`.
- Everything else (Run button state, `run_from_source`, output/plot rendering) stays the same.

### 6.4 `assets/slide-executor.css`

- Make `.sjulia-code` a block-level container with `height: 100%` and `min-height: 6rem`.
- Ensure the grid layout still allocates the left column to code and the right column to controls/output.
- Add `overflow: hidden` to the Monaco container so the editor's internal scrollbars behave.

Example additions:

```css
.sjulia-code {
  width: 100%;
  min-height: 6rem;
  height: 100%;
  background: #272822;
  border: 1px solid #4a4a40;
  border-radius: 4px;
  overflow: hidden;
}
```

## 7. Reveal.js transform considerations

Reveal.js scales slides with CSS `transform`. Monaco Editor inside transformed ancestors can misalign mouse/cursor coordinates. Mitigations:

1. Call `editor.layout()` on `slidechanged` and `resize`.
2. If cursor offset persists, experiment with Reveal's `disableLayout: true` or pin `width`/`height` in `_quarto.yml` so the transform is closer to identity.
3. Keep the textarea fallback so a broken Monaco does not block presentations.

## 8. Error handling

| Scenario | Behavior |
|----------|----------|
| Monaco CDN unreachable | Fall back to `<textarea>`; Run still works |
| Monaco loader loaded but `require` fails | Log error and fall back to `<textarea>` |
| WASM not ready | Run button disabled until `sjulia:ready` |
| Editor layout broken after slide change | `slidechanged` handler calls `editor.layout()` |

## 9. Testing plan

1. `cd slide && quarto render` succeeds.
2. Open `slide/_site/index.html` via a local server.
3. On the "Hello, sjulia" slide:
   - Confirm code is rendered by Monaco (Julia syntax colors, line numbers if enabled).
   - Type `\alpha` and verify Unicode completion works.
   - Press `Ctrl/Cmd+Enter` and verify output appears.
4. On the plot slide:
   - Confirm the 5-line sample is readable in the wider code column.
   - Run and confirm the Plotly graph renders.
5. Simulate CDN failure (block `cdn.jsdelivr.net`) and confirm fallback textarea works.
6. Navigate back and forth between slides and confirm editors remain usable.

## 10. Acceptance criteria

- [ ] `slide/assets/julia-language.js` is created and exports `registerJuliaLanguage` / `setWasmModule`.
- [ ] `slide/assets/after-body.html` loads Monaco loader before the executor.
- [ ] Each `.sjulia-executor` renders a Monaco editor with Julia highlighting and the Monokai theme.
- [ ] `Ctrl/Cmd+Enter` runs the current editor's code.
- [ ] Unicode completion (`\alpha` → `α`) works when WASM is ready.
- [ ] If Monaco fails to load, the executor falls back to a working `<textarea>`.
- [ ] `cd slide && node test-render.js` still passes.

## 11. Related work

- Issue #8770
- `./web/app.js` — Monaco initialization pattern to mirror.
- `./web/julia-language.js` — language definition and completion providers to reuse.
- `docs/superpowers/specs/2026-07-02-quarto-slide-sjulia-runtime-design.md` — base slide runtime design that intentionally left Monaco out of scope.
