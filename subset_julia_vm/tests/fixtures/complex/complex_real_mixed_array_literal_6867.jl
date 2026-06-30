using Test
using LinearAlgebra

# Issue #6867: an annotation-free array literal mixing `Complex` and `Real`
# elements (`[1.0 + 0.0im, 2.0]`) inferred `Vector{Any}` instead of upstream's
# promoted `Vector{ComplexF64}`. The same-kind cases (`[1.0+2.0im, 3.0+4.0im]`)
# were already fixed by Issue #6851; this fills the Complex×Real gap.
#
# Root cause: the compile-time array element-type narrowing had no rule for
# mixing `Complex{T}` with a `Real`, so it fell back to `Any`. The fix reduces
# the element types with Julia's `promote_type` / Complex `promote_rule`
# (`promote_type(Complex{Float64}, Float64) == ComplexF64`), routing the literal
# to inline `Complex{Float64}` / `Complex{Float32}` storage and widening each
# real element via the `Complex{T}(x, 0)` constructor.
#
# Consequence of the old bug: `norm([1.0+0.0im, 2.0])` hit the generic fallback
# (`xi*xi` adding a Complex to a Float64 accumulator) and raised a runtime type
# error instead of dispatching to the `Complex{Float64}`-specialized method.

@testset "Complex×Real mixed array literal eltype (Issue #6867)" begin
    # Complex{Float64} + Float64 -> ComplexF64
    a = [1.0 + 0.0im, 2.0]
    @test typeof(a) == Vector{ComplexF64}
    @test eltype(a) == ComplexF64
    @test a[1] == 1.0 + 0.0im
    @test a[2] == 2.0 + 0.0im

    # Complex{Float64} + Int -> ComplexF64
    @test typeof([1.0 + 0.0im, 2]) == Vector{ComplexF64}
    # Complex{Int64} + Float64 -> ComplexF64
    @test typeof([1 + 0im, 2.0]) == Vector{ComplexF64}
    # Three-element mix (Complex, Float64, Int) -> ComplexF64
    @test typeof([1.0 + 0.0im, 2.0, 3]) == Vector{ComplexF64}

    # Int↔Float promotion still works (unchanged).
    @test typeof([1, 2.0]) == Vector{Float64}

    # ComplexF32 variants
    @test typeof([1.0f0 + 0.0f0im, 2.0f0]) == Vector{ComplexF32}
    @test typeof([1.0f0 + 0.0f0im, 2]) == Vector{ComplexF32}

    # The motivating case: norm dispatches to the Complex-specialized method.
    @test norm([1.0 + 0.0im, 2.0]) == 2.23606797749979

    # Stored values are correct and usable.
    @test sum([1.0 + 0.0im, 2.0, 3]) == 6.0 + 0.0im
end

true
