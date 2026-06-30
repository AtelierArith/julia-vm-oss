# Rotations.jl subset (Issue #7434)

A pure-Julia, upstream-compatible MVP of [Rotations.jl] for SubsetJuliaVM,
bundled at `subset_julia_vm/packages/Rotations/` and adapted to the sjulia
subset. Reference source: `extern/Rotations.jl/src/`. Numeric output matches
upstream **Rotations 1.7.1** for the MVP fixtures (verified against an oracle
project with `StaticArrays` + `Quaternions` + `Rotations` installed).

Prerequisite: the bundled [StaticArrays](#dependencies) and `Quaternions`
compatibility packages (Issues #7433 / #7472).

## Supported surface

| Area | Types / functions |
|------|-------------------|
| Abstract | `Rotation{N,T}` |
| 2-D | `RotMatrix{2}` (`RotMatrix2`), `Angle2d` |
| 3-D matrix | `RotMatrix{3}` (`RotMatrix3`) |
| Single-axis | `RotX`, `RotY`, `RotZ` |
| Axis-angle | `AngleAxis` |
| 3-param | `RotationVec`, `RodriguesParam`, `MRP` |
| Quaternion | `QuatRotation` (fields `.w/.x/.y/.z`), `slerp` |
| Generators | `RotationGenerator`, `Angle2dGenerator`, `RotationVecGenerator`, `Rotations.skew`, `isrotationgenerator` |
| Interface | `rotation_angle`, `rotation_axis`, `rotation_between` (2-D/3-D), `isrotation`, `Rotations.params`, `Tuple`, `getindex`, `one`, `inv`, `*` (vector rotation + composition), `/`, `\`, `adjoint`/`transpose`, `size` |

Construction, indexing, `Tuple`, vector rotation, composition, inverse,
`rotation_angle`/`rotation_axis`, `params`, identity, and the relevant
conversions all match upstream for the fixtures under
`subset_julia_vm/tests/fixtures/rotations/`.

## Design / sjulia adaptations

The port follows the upstream `extern/Rotations.jl/src/` include layout but
hand-expands constructs that exceed the current subset and works around several
VM limitations. Each adaptation is tied to a tracked Issue:

- **No `StaticMatrix` supertype.** Upstream spells `Rotation{N,T} <:
  StaticMatrix{N,N,T}`. Here `Rotation` (and `RotationGenerator`) subtype
  **nothing**: subtyping `AbstractMatrix`/`StaticMatrix` makes `r * v`
  mis-select the generic `*(::AbstractMatrix,::AbstractVector)` over the
  specific rotation operator (Issue #8103-B). Consequence: `r isa
  AbstractMatrix` is `false` in this MVP.
- **One method per generic function (#7960).** sjulia mis-dispatches between
  sibling concrete methods of a shared generic that differ only in argument
  type. Every operation defined for more than one rotation type is therefore a
  single method on the abstract `Rotation` / `RotationGenerator` with a runtime
  `isa` branch (the same workaround StaticArrays uses for `*`). `rotation_between`
  likewise branches on `length` instead of separate `StaticVector{2}/{3}` methods.
- **Tuple-based matvec (#8090).** A boxed 3×3 `SMatrix` read out of a struct
  field loses its element-type parameter in dispatch, so `r.mat * v` misses the
  StaticArrays matvec. Vector rotation multiplies by the column-major flat
  `Tuple` directly instead.
- **Quaternion family via `_to_quat_wxyz` / `_quat_matrix_tuple`.**
  `QuatRotation`, `RotationVec`, `RodriguesParam` and `MRP` realise their matrix
  through a unit quaternion, exactly as upstream. Composition of quaternion-family
  rotations returns a `RotMatrix` (numerically identical `Tuple`) rather than
  re-deriving the source representation — an MVP simplification.
- **Constructors avoid #8103 / #8121.** No custom inner constructors and no typed
  `T{P...}(...)` outer constructors that transform arguments; the bare outer
  constructor computes the element type and hands already-typed values to the
  synthesized default field constructor.
- **`QuatRotation` stores `.w/.x/.y/.z` scalar fields (#8127).** Upstream exposes
  these through a `Base.getproperty` overload over a single `q::Quaternion`
  field, but sjulia resolves field access at compile time and ignores custom
  `getproperty`. Storing the four scalars makes `.w/.x/.y/.z` native field
  accesses; a `Quaternion` is reconstructed on demand (e.g. for `slerp`).
- **`params` and `skew` are not exported** (matching upstream): use
  `Rotations.params` / `Rotations.skew`.

### Dependencies

The bundled `StaticArrays` package gained several fixes required by Rotations:
column-major storage (Issue #8084) and scalar division / `norm` / `normalize`
on static arrays (Issue #8125). Type-system fixes #8092 (registered struct names
not treated as typevars in `isa`/`<:`) and the #8103 constructor-specificity fix
also landed as part of this milestone.

## Deferred / unsupported

Explicitly out of scope for this MVP (filed or documented for later):

- **`Base.getproperty` overloads (#8127).** The general VM feature is unsupported;
  `QuatRotation` works around it (above).
- **Static-array scalar division on non-square / larger matrices (#8125).**
  Needs a runtime-parameter `SMatrix{M,N}` constructor the VM lacks.
- **`RotMatrixGenerator`** (dense SMatrix-backed generator) and the
  **generator exp/log maps** — only `Angle2dGenerator` / `RotationVecGenerator`
  + `skew` + `isrotationgenerator` are implemented.
- **Exponential / log maps and rotation-error operators**: `expm`, `logm`,
  `Rotations.params`-based `⊖`/`⊕`, `error_maps.jl`, `rotation_error.jl`.
- **Exhaustive Euler-order variants** (`RotXYZ`, `RotZYX`, … the two- and
  three-axis compositions) — only the single-axis `RotX/Y/Z` are implemented.
- **`rotation_between` for N-D** (N≠2,3) — needs an SVD.
- **Random constructors** (`rand(RotMatrix)`, …) — depends on broader `Random`
  distribution parity.
- **`eigen` / decomposition parity**, **`nearest_rotation`**,
  **`principal_value`** beyond the quaternion-construction path.
- **ForwardDiff derivative parity**, **RecipesBase plotting**, **Unitful**
  element types.

## Tests

`subset_julia_vm/tests/fixtures/rotations/` (run with
`cargo nextest run --test fixture_tests rotations`):

| Fixture | Phase / Issue |
|---------|---------------|
| `using_rotations_quaternions_7472` | 1 / #7472 |
| `rotations_2d_basics_7473` | 2 / #7473 |
| `rotations_3d_axis_7474` | 3 / #7474 |
| `rotations_angleaxis_7474` | 3 / #7474 |
| `rotations_param3_7474` | 3 / #7474 |
| `rotations_quatrotation_7475` | 4 / #7475 |
| `rotations_rotation_between_7475` | 4 / #7475 |
| `rotations_generators_7476` | 5 / #7476 |

Parity is verified against upstream Julia with
`bash scripts/fixture_julia_parity.sh <fixture>` (requires the oracle project).

[Rotations.jl]: https://github.com/JuliaGeometry/Rotations.jl
