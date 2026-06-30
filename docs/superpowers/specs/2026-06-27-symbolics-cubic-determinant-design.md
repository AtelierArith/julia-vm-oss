# Design: Add x³+y³+z³-3xyz Determinant Example to iOS Symbolics.jl + LinearAlgebra.jl Sample

## Summary

Add the identity

```
x³ + y³ + z³ - 3xyz = det([x y z; z x y; y z x])
```

to the existing iOS sample `symbolics_linear_algebra.jl` (and its Swift fallback), demonstrating Symbolics.jl + LinearAlgebra.jl working with a 3×3 circulant matrix and verifying the identity with `expand` and a numeric `substitute` check.

## Background

The existing iOS sample `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl` shows:

- `@variables x y` declaration
- 2×2 symbolic matrix construction
- Matrix-vector/matrix-matrix products
- `det`, `inv`, and `A \ b`

It does not yet show a 3×3 determinant or the use of `expand`/`substitute` to verify a symbolic identity. The cubic identity is a compact, well-known example that naturally extends the determinant topic and introduces symbolic expansion and numeric substitution.

## Placement

Append the new example to the **end** of the existing `symbolics_linear_algebra.jl` sample, after the existing `inv` and `A \ b` demonstrations. Keep the existing `@variables x y` and reuse it by adding `z`.

## Files to Modify

| File | Change |
|------|--------|
| `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl` | Append the cubic determinant identity example |
| `SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSamples+Advanced.swift` | Update the Swift fallback string to match the `.jl` file exactly |
| `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` | No change required; the existing description already mentions determinant |

## Code Change

Append the following to `symbolics_linear_algebra.jl`:

```julia
# x^3 + y^3 + z^3 - 3xyz as a determinant of a 3x3 circulant matrix.
@variables z
M = [x y z; z x y; y z x]
p = x^3 + y^3 + z^3 - 3x*y*z
println("M                  = ", M)
println("det(M)             = ", det(M))
println("expand(det(M))     = ", expand(det(M)))
println("x^3+y^3+z^3-3xyz   = ", p)
println("numeric check      = ", substitute(expand(det(M) - p), Dict(x => 1, y => 2, z => 3)))
```

## Verification

1. Run the sample with upstream Julia:
   ```bash
   julia SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl
   ```
   Expected: `expand(det(M))` and `p` print the same expanded polynomial; `numeric check` prints `0`.

2. Run the sample with sjulia:
   ```bash
   cargo run -p subset_julia_vm --bin sjulia --features repl -- \
     SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl
   ```
   Expected: program completes without error; `numeric check` prints `0`.

3. Build the iOS app / Swift fallback to confirm the string copy compiles:
   ```bash
   xcodebuild -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
     -scheme SubsetJuliaVMApp -sdk iphonesimulator build
   ```

## Risks and Considerations

- `expand(det(M))` and `p` may have different internal term orderings between upstream Julia and sjulia, so the sample uses `substitute` on the expanded difference for a robust numeric zero check.
- The Swift fallback and `.jl` file must remain byte-for-byte identical in the code section to avoid drift between bundled and fallback samples.
- `@variables z` is added after existing `x y` use; ensure it does not shadow or break earlier examples.

## Decisions

- **Approach**: B (identity verification via `expand` and numeric `substitute`) — adds educational value without making the sample too long, and works reliably in both upstream Julia and sjulia.
- **Placement**: Append to existing sample rather than create a new one — keeps the change minimal and thematically grouped.
