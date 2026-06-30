# Consolidated regression fixture for the typed-container "no widening" umbrella (Issue #5073).
# Pins the type-loss matrix enumerated in the umbrella so future widening regressions are caught.
# Sub-issues #5039 / #5040 / #5041 and the #4646 boxed-numeric cluster are all merged; this
# fixture locks their parity-with-upstream behavior in one place.

using Test

@testset "typed allocation keeps declared element type" begin
    @test typeof(zeros(2)) === Vector{Float64}
    @test typeof(zeros(Int8, 2)) === Vector{Int8}
    @test typeof(ones(2)) === Vector{Float64}
    @test typeof(ones(Int8, 3)) === Vector{Int8}
    @test typeof(fill(Int8(3), 2)) === Vector{Int8}
    @test typeof(fill(2.0f0, 3)) === Vector{Float32}
end

@testset "typed allocation (n-dimensional / parametric)" begin
    @test typeof(zeros(Int8, (2, 2))) === Matrix{Int8}
    @test typeof(zeros(Int8, 2, 2)) === Matrix{Int8}
    @test typeof(zeros(Complex{Float64}, 2)) === Vector{Complex{Float64}}
end

@testset "boxed numeric values keep their real type" begin
    @test typeof(Any[Int8(3)][1]) === Int8
    @test typeof(Real[Int8(3)][1]) === Int8
    a = Any[Int8(3)]
    push!(a, Int16(5))
    @test typeof(a[2]) === Int16
    @test eltype(Real[1 // 2, 3]) === Real
    @test typeof(Real[1 // 2, 3][1]) === Rational{Int64}
    @test typeof(Real[1 // 2, 3][2]) === Int64
end

@testset "typed comprehension T[expr ...] converts and keeps T" begin
    @test typeof(Float64[i for i in 1:3]) === Vector{Float64}
    @test typeof(Int8[i for i in 1:3]) === Vector{Int8}
    @test typeof(Bool[x > 0 for x in [-1, 0, 1]]) === Vector{Bool}
    @test Float64[i for i in 1:3] == [1.0, 2.0, 3.0]
end

@testset "Vector{T}(::Tuple) is a MethodError (matches upstream)" begin
    @test_throws MethodError Vector{Int64}((1, 2, 3))
end

true
