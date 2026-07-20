using Test

struct MyInt9833 <: Integer end

Base.promote_rule(::Type{MyInt9833}, ::Type{Float64}) = Float64

@testset "promote_type reconciles opposite-direction and abstract rules" begin
    @test promote_type(MyInt9833, Float64) == Float64
    @test promote_type(Float64, MyInt9833) == Float64
    @test promote_type(MyInt9833, MyInt9833) == MyInt9833
    @test promote_type(BigInt, MyInt9833) == BigInt
    @test promote_type(MyInt9833, BigFloat) == BigFloat
    @test promote_type(Rational{Int64}, MyInt9833) == Rational{Integer}
    @test promote_type(Rational{Int64}, Float64) == Float64
end

true
