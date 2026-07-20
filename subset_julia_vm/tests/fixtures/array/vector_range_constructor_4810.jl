# Issue #4810: Vector(::AbstractRange) returned the range unchanged
# instead of materializing to a Vector. Surfaced after the parity
# probe extension from PR #4806/#4808.
#
# Fix:
# - subset_julia_vm_compile/src/compile/expr/collection.rs::
#   compile_array_constructor: for the no-type-args single-arg form
#   (`Vector(range)`), emit Instr::CallBuiltin(RangeCollect, 1)
#   instead of returning the range unchanged.
# - subset_julia_vm/src/julia/base/array.jl: added
#   `Vector(r::AbstractRange) = collect(r)` and
#   `Vector{T}(r::AbstractRange) where {T}` for the typed form
#   (though typed form still hits the compile intercept — tracked
#   as follow-up #4811).
#
# Scope: this fixture covers the no-type-args form. The typed form
# `Vector{T}(::AbstractRange)` is tracked as follow-up #4811 and
# not asserted here.

using Test

@testset "Vector(::UnitRange) materializes (Issue #4810)" begin
    v = Vector(1:3)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end

@testset "Vector(::StepRangeLen Float) materializes (Issue #4810)" begin
    v = Vector(1:0.5:3)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 1.5, 2.0, 2.5, 3.0]
end

@testset "Vector(::Array) — array copy regression (Issue #4810)" begin
    # Vector(arr) on an array should still return an Array, but upstream
    # allocates a fresh vector rather than returning arr by identity.
    src = [10, 20, 30]
    v = Vector(src)
    @test typeof(v) === Vector{Int64}
    @test v == [10, 20, 30]
    @test !(v === src)
end

true
