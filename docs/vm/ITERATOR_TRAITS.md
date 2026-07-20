# Iterator Traits and Consumer Architecture

Status: implemented and ratcheted (Issue #10050)

SubsetJuliaVM exposes the upstream Julia iterator model at the Julia surface:
iterators are driven by `iterate`, wrapper traits compose from their wrapped
iterator, and public consumers must not assume `getindex`. The VM may retain a
native generator fast path underneath that surface when it is observationally
equivalent. The measured keep decision and performance evidence live in
`GENERATOR_REPRESENTATION.md`.

## Generator representation and callable identity

Source generators lower to the upstream-shaped `Base.Generator{I,F}` /
`Iterators.Filter` / `Iterators.Flatten` / `product` graph. The callable field is
a normal closure/function value at the Julia surface. The compiler may collapse
that graph to `MakeGenerator` / `GeneratorCallable`, but the collapsed value must
retain callable capture and dispatch semantics; consumers cannot inspect a
rendered function name to recover identity.

## Wrapper trait algebra

`IteratorSize` and `IteratorEltype` are type-level traits mirrored in
`subset_julia_vm/src/julia/base/generator.jl`:

| Wrapper | `IteratorSize` | `IteratorEltype` |
|---|---|---|
| `Generator{I,F}` | delegates to `I` | `EltypeUnknown` unless proven by the upstream-shaped rule |
| `Filter{F,I}` | `SizeUnknown` | delegates only where upstream does |
| `Flatten{I}` | wrapper-specific, never inferred from public indexing | `EltypeUnknown` until the flattened element type is established |
| `zip` / `enumerate` / product | composed from all wrapped iterators | derived from their element tuples and callable body |
| `Array{T,N}` / ranges | `HasShape{N}` / `HasShape{1}` | `HasEltype` |

Value-level fast paths must unwrap Pure Julia array wrappers and produce the same
answer as type-level dispatch. New wrappers belong in the differential matrix,
not in isolated consumer-specific conditionals.

## Consumer contract

Public `collect`, `sum`, `foldl`, `map`, `first`, `length`, comprehensions, and
explicit `for` loops use the iterator protocol. Public `getindex(::Generator)`
must remain a catchable `MethodError`. An internal native fast path is allowed
only when its eligibility is wrapper-aware and its value, shape, eltype,
dispatch, and error behavior match the iterate path. The guardrails are:

- `CHECKLISTS.md` — Generator Consumer / Public Indexing and Trait Fast-Path checklists;
- `scripts/check_array_public_data_access.sh` — rejects generator indexing/materialization fallback;
- `generator_trait_matrix_9566` — upstream differential coverage across traits and consumers.

Generator body typing is consumer-independent: dynamic bodies dispatch per
element, while collection result typing accumulates the observed/inferred body
types rather than freezing on the first element. Empty-result eltype reuse is
allowed only when body and predicate provenance are transparent.

## Broadcast and range boundaries

Broadcast shape is selected through the `BroadcastStyle` hierarchy in
`base/broadcast.jl`; tuple style materializes tuples, array style preserves array
shape, and scalar/style combination follows the shared style rules. Generator
iteration does not make a generator publicly indexable for broadcast.

Range traits and parameters are orthogonal to consumer mechanics:

- `UnitRange{T}` has element and step type `T`;
- `StepRange{T,S}` separates element type `T` from step type `S`;
- floating `StepRangeLen{T,R,S}` keeps visible element type `T` while `R` and
  `S` may use `TwicePrecision`/wider arithmetic;
- `collect`, indexing, `first`/`last`, and `step` must agree on those parameters.

`RangeValue` and `twiceprecision.rs` are the runtime oracle for the latter
contract. Add differential cases for every new numeric width instead of
hard-coding a consumer-specific range exception.

## Acceptance and change protocol

`scripts/check_generator_trait_matrix.sh` regenerates the 152-cell upstream
oracle and requires a zero-row skiplist. The matrix covers map, filter, flatten,
product, zip, and enumerate over typed arrays, ranges, and mixed `Any` bases;
`IteratorSize`, `IteratorEltype`, `collect`, `sum`, `foldl`, `first`, `length`,
and explicit `for` consumption. Any new divergence must be filed first and may
not silently weaken the zero-residual ratchet.

Run after iterator/consumer changes:

```bash
julia --startup-file=no scripts/gen_generator_trait_matrix_fixture.jl
bash scripts/check_generator_trait_matrix.sh
bash scripts/check_array_public_data_access.sh
timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests generator
```
