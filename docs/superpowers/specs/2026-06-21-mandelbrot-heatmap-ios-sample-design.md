# iOS Application Mandelbrot Heatmap Sample Design

Date: 2026-06-21

## Objective

Add a new iOS sample that visualizes the Mandelbrot set using `Plots.jl`'s `heatmap`, based on the existing ASCII Mandelbrot sample.

## Background

- The existing `advanced/mandelbrot_set.jl` computes an escape-time grid and prints ASCII art.
- `Plots.jl` is a bundled package and exports `heatmap` / `heatmap!`.
- A fixture test (`subset_julia_vm/tests/fixtures/packages/plots_heatmap_6360.jl`) confirms `heatmap` works in sjulia.

## Design Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| Sample ID | `mandelbrot_heatmap` | Clear, descriptive identifier. |
| Display name | `Mandelbrot Heatmap` | Distinguishes it from the ASCII `Mandelbrot Set`. |
| Difficulty | `Intermediate` | Other `Plots.jl` samples (`plotting_2d`, `sinc_surface`, `barnsley_fern`) are intermediate. |
| Category | `Visualization` | The sample is about plotting with Plots.jl. |
| Folder | `intermediate` | Matches difficulty and existing Plots samples. |
| Swift fallback | `CodeSamples+Intermediate.swift` | Same pattern as other intermediate samples. |

## Proposed Approaches (Rejected)

1. **Combined ASCII + heatmap in one sample** — Show both ASCII art and the heatmap. Rejected because it duplicates the existing ASCII sample and mixes two output styles in one example.
2. **Animated heatmap with `@gif`** — Render the Mandelbrot set at increasing iteration counts. Rejected because it adds animation complexity and the user asked for a heatmap sample, not an animation.

## Selected Approach

**Standalone heatmap visualization**: Reuse the escape-time grid computation from `mandelbrot_set.jl` and visualize the result with a single `Plots.heatmap` call. This keeps the sample focused on the heatmap visualization concept.

## Sample Code Outline

```julia
# Mandelbrot set visualized as a heatmap with Plots.jl.
using Plots

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute a 2D escape-time grid via broadcasting.
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymin, ymax; length=height)

    # xs' is a row vector, ys is a column vector → broadcasting builds a complex grid
    C = xs' .+ im .* ys

    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# Render the escape-time grid as a heatmap.
@time grid = mandelbrot_grid(200, 150, 80)
heatmap(range(-2.0, 1.0; length=200), range(-1.2, 1.2; length=150), grid;
        title="Mandelbrot Set", aspect_ratio=:equal)
```

## Files to Modify

1. `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/mandelbrot_heatmap.jl` — new sample source.
2. `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` — add JSON metadata entry.
3. `SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSamples+Intermediate.swift` — add Swift fallback entry.
4. `SubsetJuliaVMApp/SubsetJuliaVMAppTests/SampleCodeTests.swift` — add `testMandelbrotHeatmap()` individual test.

## Verification

- Run the `.jl` file with upstream Julia to confirm output.
- Run the `.jl` file with `target/release/sjulia` to confirm sjulia compatibility.
- Run iOS sample tests via `make test-ios-samples`.

## Assumptions

- Auto-permission mode is active; clarifying questions were skipped and reasonable defaults were selected.
- The sample stays within the currently supported `Plots.jl` subset (`heatmap`, `aspect_ratio`, `title`).
