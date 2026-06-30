using Test

struct R8253{T <: Integer}
end

issue8253(::R8253{T}) where T <: Integer = Rational{T}(1)

@testset "Rational constructor through parametric method type variable (Issue #8253)" begin
    x = issue8253(R8253{BigInt}())
    @test typeof(x) === Rational{BigInt}
    @test x == big(1)//big(1)
    @test numerator(x) == big(1)
    @test denominator(x) == big(1)
end

true
