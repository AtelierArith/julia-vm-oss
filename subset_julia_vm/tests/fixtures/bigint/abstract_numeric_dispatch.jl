# Test BigInt/BigFloat arithmetic inside functions with abstract numeric type annotations
# When a parameter is typed as ::Number, ::Real, ::Integer, etc., the actual runtime
# value could be BigInt or BigFloat. Binary operations must use runtime dispatch
# instead of hardcoded intrinsics like AddFloat/AddInt.
# Related: Issue #2498

using Test

double_number(x::Number) = x + x
triple_real(x::Real) = x + x + x
square_integer(x::Integer) = x * x
add_numbers(x::Number, y::Number) = x + y
cmp_lt(x::Number, y::Number) = x < y
half_number(x::Number, y::Number) = x / y

@testset "Abstract numeric type dispatch with BigInt/BigFloat" begin
    @testset "::Number with BigInt" begin
        @test double_number(big(21)) == big(42)
        @test typeof(double_number(big(21))) == BigInt
    end

    @testset "::Number with Int64 and Float64" begin
        @test double_number(10) == 20
        @test double_number(3.14) == 6.28
    end

    @testset "::Real with BigInt" begin
        @test triple_real(big(10)) == big(30)
        @test typeof(triple_real(big(10))) == BigInt
    end

    @testset "::Integer with BigInt" begin
        @test square_integer(big(7)) == big(49)
        @test typeof(square_integer(big(7))) == BigInt
    end

    @testset "Two abstract params with BigInt" begin
        @test add_numbers(big(10), big(20)) == big(30)
        @test add_numbers(big(10), 20) == big(30)
        @test add_numbers(10, big(20)) == big(30)
        @test add_numbers(10, 20) == 30
    end

    @testset "Comparisons with abstract params" begin
        @test cmp_lt(big(10), big(20)) == true
        @test cmp_lt(big(20), big(10)) == false
        @test cmp_lt(10, 20) == true
    end

    @testset "::Number with BigFloat" begin
        @test double_number(big(3.14)) == big(6.28)
        @test typeof(double_number(big(3.14))) == BigFloat
    end

    @testset "Division with BigFloat abstract" begin
        @test half_number(big(6.0), big(2.0)) == big(3.0)
        @test typeof(half_number(big(6.0), big(2.0))) == BigFloat
    end
end

true
