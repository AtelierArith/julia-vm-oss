# Web Playground Redesign: Match iOS Samples, Mobile-First, Drop Tutorial

**Date:** 2026-06-21
**Scope:** `./web` static playground for GitHub Pages deployment

## Goal

Redesign the SubsetJuliaVM web playground (`./web`) so that:

1. Its sample library is identical to the iOS app's sample library (`SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/`).
2. The interactive tutorial flow is removed entirely.
3. The UI is designed for smartphone screens first while remaining usable on desktop.
4. The site continues to deploy as a static site on GitHub Pages.

## Current State

- `./web/index.html` + `app.js` + `styles.css` + `samples_ir.js`.
- `samples_ir.js` contains ~62 samples including 5 tutorial lessons.
- UI is a side-by-side editor/output split with a top sample dropdown and a tutorial panel.
- Monaco editor is loaded from CDN; Plotly is bundled locally.
- Execution is via `wasm.run_from_source(code, seed)` from `web/pkg/` (wasm-pack output).

## Target State

### Samples

Use the iOS sample set defined in `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` (27 samples) plus the `.jl` files in `beginner/`, `intermediate/`, and `advanced/`.

Sample metadata fields:

- `id`
- `name`
- `category`
- `description`
- `difficulty`
- `tags`
- `folder`

Web representation will keep the same fields and load `code` from the matching `.jl` file.

### Web-Incompatible Samples

Some iOS samples depend on packages or features not present in the web WASM build:

- `jsxgraph_demo`
- `apollonian_gasket`
- `barnsley_fern` (uses Distributions.jl)
- `distributions_package`
- `symbolics_package`
- `primes_package`

These samples are still listed so the web catalog matches iOS, but selecting one shows a friendly fallback message instead of attempting execution.

## Design: Tabbed Mobile Editor

### Layout Philosophy

Phone screens are too small for a permanent split view and a Monaco-based code editor with toolbars. The new UI uses a tabbed pattern on narrow screens and degrades gracefully to a split desktop layout.

### Mobile (< 768px)

- **Header:** App title + sample picker button (opens sample drawer/modal).
- **Tab bar:** `Edit` | `Output`.
- **Edit tab:** Lightweight code editor (textarea with Julia-like styling) + Run/Share/Copy action bar at the bottom.
- **Output tab:** Text output, Plotly plot area, and result/error display.
- **Sample drawer:** Full-screen or bottom-sheet modal with:
  - Search input.
  - Category filter chips.
  - Sample cards showing name, category, difficulty.

### Desktop (>= 768px)

- **Three-pane layout:** Sample list sidebar | Code editor | Output panel.
- Code editor uses Monaco on desktop for syntax highlighting and completion.
- Output panel shows text, plots, and errors.
- Action bar lives inside the editor pane.

### Editor Strategy

- **Mobile:** plain `<textarea>` to avoid Monaco's mobile awkwardness, heavy JS load, and touch issues. Keep Julia language coloring minimal via CSS classes.
- **Desktop:** Monaco editor as today, but initialized only when the screen is wide enough or only when the Edit tab is active on desktop.
- Share URL, copy source, and clear output remain.

### Sample Picker

- Replace the `<select>` dropdown with a button that opens a searchable, category-filtered card list.
- Categories derived from `samples.json`.
- Default selection is the first sample (`hello_world`).
- URL hash restoration still works and clears the sample selection.

### Removed Elements

- Tutorial panel HTML, CSS, and all `tutorial` object handling in `app.js`.
- `samples_ir.js` tutorial entries.
- Tutorial check evaluation functions.
- `tutorialSampleIndexes`.

### GitHub Pages Constraints

- No backend; everything remains static files.
- Keep relative asset paths (`./pkg/`, `./plotly.min.js`, etc.).
- No server-side routing; SPA behavior uses hash fragments for shared code.
- Do not introduce a build step that GitHub Pages cannot run (keep vanilla JS).

## Files to Change

| File | Change |
|------|--------|
| `web/index.html` | Remove tutorial panel. Restructure layout for tabs/mobile. Add sample drawer markup. |
| `web/styles.css` | Add mobile-first styles, tab bar, drawer, sample cards, bottom action bar. Remove tutorial styles. |
| `web/app.js` | Remove tutorial logic. Add tab switching, sample drawer, category filter, mobile/desktop layout switching. Load samples from new module. |
| `web/samples_ir.js` | Replace contents with iOS-derived samples. Keep filename for import compatibility. |
| `web/README.md` | Update sample documentation and remove tutorial references. |

## Data Flow

1. `samples.js` exports an array built from iOS `samples.json` + `.jl` files.
2. `app.js` renders the sample list and default selection on init.
3. User picks a sample → code loaded into editor state.
4. User runs → `wasm.run_from_source(code, seed)`.
5. Result rendered in Output tab / panel.
6. Share URL encodes editor contents into URL hash.

## Error Handling

- WASM not loaded: show existing build instructions.
- Web-incompatible sample selected: show a non-blocking info banner in the output area and do not execute.
- Execution errors: render in output area.

## Testing Checklist

- [ ] All 27 iOS samples appear in the web picker.
- [ ] Tutorial UI and code removed.
- [ ] Phone portrait layout is usable (tabs, bottom bar, sample drawer).
- [ ] Desktop layout shows editor + output side by side.
- [ ] `plot(sin)` still renders a Plotly plot.
- [ ] Share URL still encodes/decodes code.
- [ ] GitHub Pages deploy works (no 404s for relative assets).
- [ ] Web-incompatible samples show a fallback message.

## Decisions

- Keep the filename `samples_ir.js` to avoid updating imports in `app.js` and `test.html`; only its contents change.
- Breakpoint for switching between tabbed mobile and split desktop layout is **768px**.
- The exported array remains named `samplesIR` for existing consumers.
