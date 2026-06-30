# Bug: Base exports include identifiers not in upstream Julia

## Summary
`subset_julia_vm/src/julia/base/exports.jl` exports several identifiers that do not appear in `julia/base/exports.jl` (upstream Julia in this repo). Base exports should match upstream; these are likely unintended and should not be exported from Base.

## Expected
`subset_julia_vm/src/julia/base/exports.jl` exports only identifiers present in `julia/base/exports.jl`.

## Actual
The following identifiers are exported in SubsetJuliaVM Base but are not exported in upstream Julia Base:

```
@printf
@sprintf
Algorithm
ArithmeticRounds
ArithmeticStyle
ArithmeticUnknown
ArithmeticWraps
IOError
OrderStyle
Ordered
ParseError
RangeStepIrregular
RangeStepRegular
RangeStepStyle
Unordered
WrappedException
catalan
countfrom
cycle
displays
drop
e
eulergamma
flatten
fliplr
flipud
golden
isalnum
isexported
isgreater
ispublic
issubset_proper
issuperset
issuperset_proper
nth
partition
peel
product
repeated
rest
take
γ
φ
```

## Steps to reproduce
1. Compare `subset_julia_vm/src/julia/base/exports.jl` against `julia/base/exports.jl`.
2. Compute set difference: `subset_exports - julia_exports`.

## Evidence
- SubsetJuliaVM exports: `subset_julia_vm/src/julia/base/exports.jl`
- Upstream exports: `julia/base/exports.jl`

## Notes
- Some entries appear to belong in stdlib modules (e.g., Iterators or Printf) rather than Base exports.
- Please reconcile Base exports to match upstream Julia.
