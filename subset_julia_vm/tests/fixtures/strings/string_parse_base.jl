# string(x; base=N) and parse(T, s; base=N) - number base conversion (Issue #2036)

using Test

@testset "string(x; base=N)" begin
    # Hexadecimal (base 16)
    @test string(255, base=16) == "ff"
    @test string(0, base=16) == "0"
    @test string(16, base=16) == "10"

    # Binary (base 2)
    @test string(255, base=2) == "11111111"
    @test string(0, base=2) == "0"
    @test string(10, base=2) == "1010"

    # Octal (base 8)
    @test string(255, base=8) == "377"
    @test string(8, base=8) == "10"
    @test string(10, base=8) == "12"

    # Decimal (base 10) - identity
    @test string(42, base=10) == "42"
end

@testset "parse(Int, s; base=N)" begin
    # Hexadecimal (base 16)
    @test parse(Int, "ff", base=16) == 255
    @test parse(Int, "10", base=16) == 16
    @test parse(Int, "0", base=16) == 0

    # Binary (base 2)
    @test parse(Int, "11111111", base=2) == 255
    @test parse(Int, "1010", base=2) == 10

    # Octal (base 8)
    @test parse(Int, "377", base=8) == 255
    @test parse(Int, "12", base=8) == 10

    # Decimal (base 10)
    @test parse(Int, "42", base=10) == 42
end

@testset "round-trip: parse(Int, string(x; base=N); base=N) == x" begin
    @test parse(Int, string(100, base=16), base=16) == 100
    @test parse(Int, string(42, base=2), base=2) == 42
    @test parse(Int, string(73, base=8), base=8) == 73
end

true
