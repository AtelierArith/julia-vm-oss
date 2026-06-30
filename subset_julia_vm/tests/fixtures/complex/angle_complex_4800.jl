# Issue #4800: angle(::Complex) was only defined for
# Complex{Float64}, so angle(1 + 1im) (a Complex{Int64}) failed
# at compile-time with NoMethodFound.
#
# Fix: widened the signature to `angle(z::Complex{T}) where {T<:Real}`
# in subset_julia_vm/src/julia/base/complex.jl, mirroring upstream
# `Base.angle(z::Complex) = atan(imag(z), real(z))`.

using Test

@testset "angle(::Complex{Int64}) works (Issue #4800)" begin
    @test angle(1 + 1im) ≈ 0.7853981633974483
    @test angle(-1 + 0im) ≈ 3.141592653589793
    @test angle(0 - 1im) ≈ -1.5707963267948966
    @test angle(3 + 4im) ≈ 0.9272952180016122
end

@testset "angle(::Complex{Float64}) regression (Issue #4800)" begin
    @test angle(1.0 + 0.0im) === 0.0
    @test angle(0.0 + 1.0im) ≈ 1.5707963267948966
end

true
