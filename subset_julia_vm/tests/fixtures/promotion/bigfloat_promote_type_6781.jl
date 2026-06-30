using Test

# Issue #6781: promote_type(BigFloat, <Float>) and promote_type(BigInt, <Float>)
# must return BigFloat (matching upstream). The bug was that a method whose
# parameter is a `Union{Type{A}, Type{B}, ...}` (as the BigFloat / BigInt /
# Rational promote_rule methods are written, Issue #5070) was never matched at
# runtime against a concrete type-object argument, so promote_rule fell back to
# the generic `Union{}` rule and promote_type widened to typejoin (AbstractFloat
# / Real) instead of the registered result type.

@testset "promote_type BigFloat with floats (Issue #6781)" begin
    @test promote_type(BigFloat, Float64) === BigFloat
    @test promote_type(Float64, BigFloat) === BigFloat
    @test promote_type(BigFloat, Float32) === BigFloat
    @test promote_type(BigFloat, Float16) === BigFloat
    @test promote_type(BigFloat, Int64) === BigFloat
    @test promote_type(BigFloat, BigInt) === BigFloat
    @test promote_type(BigFloat, Bool) === BigFloat
end

@testset "promote_type BigInt with floats (Issue #6781)" begin
    @test promote_type(BigInt, Float64) === BigFloat
    @test promote_type(Float64, BigInt) === BigFloat
    @test promote_type(BigInt, Float32) === BigFloat
    @test promote_type(BigInt, Float16) === BigFloat
    @test promote_type(BigInt, Int64) === BigInt
end

@testset "promote_rule Union-of-Type dispatch (Issue #6781)" begin
    @test promote_rule(BigFloat, Float64) === BigFloat
    @test promote_rule(BigInt, Float64) === BigFloat
    @test promote_rule(Rational{Int8}, Int8) === Rational{Int8}
    @test promote_rule(Rational{BigInt}, Int64) === Rational{BigInt}
end

@testset "user Union-of-Type method dispatch (Issue #6781)" begin
    f(::Type{Int64}) = "int64"
    f(::Union{Type{Float64}, Type{Float32}}) = "float"
    @test f(Int64) == "int64"
    @test f(Float64) == "float"
    @test f(Float32) == "float"
end

true
