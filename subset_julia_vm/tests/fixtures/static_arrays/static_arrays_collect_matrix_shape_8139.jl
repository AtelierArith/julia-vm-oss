# Issue #8139: collect(::StaticMatrix) preserves the 2-D shape (Matrix), instead
# of flattening to a column-major Vector.
#
# The generic `collect(itr)` in iterators.jl routes through `IteratorSize(itr)`.
# Inside that Base method `itr` is statically `Any`, so the trait call dispatches
# at runtime on the value. sjulia's base lacked any rule matching a non-`Array`
# `AbstractArray` (StaticArrays' `SMatrix{2,2,Int64} <: ... <: AbstractArray{Int64,2}`),
# so `IteratorSize` fell through to the generic `IteratorSize(::Type) = HasLength()`
# and `_collect` built a 1-D Vector, dropping the shape. Upstream expresses the
# rule at the type level (`IteratorSize(::Type{<:AbstractArray{<:Any,N}}) =
# HasShape{N}()`), but sjulia's dispatcher cannot bind `N` through the abstract
# supertype chain, so the rule is mirrored as a value-based
# `IteratorSize(a::AbstractArray) = HasShape{ndims(a)}()` in base/generator.jl.
# `collect(::StaticVector)` (Issue #8131) must still return a 1-D Vector.
using StaticArrays

sm = SMatrix{2,2}(1, 2, 3, 4)        # column-major flat -> [1 3; 2 4]
sm32 = SMatrix{3,2}(1, 2, 3, 4, 5, 6) # 3x2 -> [1 4; 2 5; 3 6]
smf = SMatrix{2,2}(1.0, 2.0, 3.0, 4.0)
sv = SVector(1, 2, 3)

cm = collect(sm)
cm32 = collect(sm32)
cmf = collect(smf)
cv = collect(sv)

ok = cm == [1 3; 2 4] && cm isa Matrix{Int64} && size(cm) == (2, 2) &&
     cm32 == [1 4; 2 5; 3 6] && cm32 isa Matrix{Int64} && size(cm32) == (3, 2) &&
     cmf == [1.0 3.0; 2.0 4.0] && cmf isa Matrix{Float64} &&
     # static vector still collects to a 1-D Vector (Issue #8131 stays fixed)
     cv == [1, 2, 3] && cv isa Vector{Int64} &&
     # IteratorSize trait parity on the runtime value (what collect dispatches on)
     Base.IteratorSize(sm) isa Base.HasShape{2} &&
     Base.IteratorSize(sm32) isa Base.HasShape{2} &&
     Base.IteratorSize(sv) isa Base.HasShape{1} &&
     # plain Array shapes are unaffected
     collect([1 2; 3 4]) isa Matrix{Int64} &&
     collect([1, 2, 3]) isa Vector{Int64}

println(ok)
ok
