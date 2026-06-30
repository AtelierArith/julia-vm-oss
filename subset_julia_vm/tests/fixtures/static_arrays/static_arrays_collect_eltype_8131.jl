# Issue #8131: collect(::StaticVector) preserves the element type.
#
# Previously `collect(SVector(1, 2, 3))` returned `Vector{Any}`: the generic
# iterate-based `_collect` path asked `eltype(itr)` for a statically-opaque
# static-array value, and the `eltype` builtin had no arm for the flat
# `StaticArrayInline`/`StaticArray` carriers, so it widened to `Any` (and the VM
# could not iterate a `StaticArrayInline` at all). `eltype` now reports the
# concrete element type and static arrays are iterable, so `collect` materializes
# a `Vector{T}` with the correct element type and values, matching upstream
# StaticArrays.
using StaticArrays

r1 = collect(SVector(1, 2, 3))
r2 = collect(SVector(1.0, 2.0))
r3 = collect(SVector{2,Int32}(Int32(1), Int32(2)))
re = collect(SVector{0,Int}())

ok = r1 == [1, 2, 3] && r1 isa Vector{Int64} &&
     r2 == [1.0, 2.0] && r2 isa Vector{Float64} &&
     r3 == Int32[1, 2] && r3 isa Vector{Int32} &&
     re isa Vector{Int64} && length(re) == 0 &&
     # eltype reports the concrete element type for an opaque static-array value
     eltype(SVector(1, 2, 3)) === Int64 &&
     eltype(SVector(1.0, 2.0)) === Float64 &&
     # the narrowed `_collect` trait path keeps the element type (Issue #8131 repro)
     Base._collect(1:1, SVector(1, 2, 3), Base.HasEltype(), Base.HasLength()) isa Vector{Int64} &&
     # iterate-based reductions over a static vector now work
     sum(SVector(1, 2, 3)) == 6

println(ok)
ok
