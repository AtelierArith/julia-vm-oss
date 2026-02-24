# Test Printf/sprintf formatting functions (Issue #2864)
#
# The Printf module re-exports @printf and @sprintf from Base.
# The sprintf builtin supports C-style format specifiers: %d, %s, %f, %x, %X, %o, %c, %%
# Note: `using Printf` currently has a parse error with `import Base: @printf, @sprintf`.
# Tests use the `sprintf` function and `@printf` macro directly from Base.

using Test

@testset "sprintf integer formatting (%d)" begin
    @test sprintf("%d", 42) == "42"
    @test sprintf("%d", -7) == "-7"
    @test sprintf("%d", 0) == "0"
    @test sprintf("%d", 1000000) == "1000000"
end

@testset "sprintf string formatting (%s)" begin
    @test sprintf("%s", "hello") == "hello"
    @test sprintf("%s %s", "hello", "world") == "hello world"
    @test sprintf("%s", "") == ""
end

@testset "sprintf hex formatting (%x, %X)" begin
    @test sprintf("%x", 255) == "ff"
    @test sprintf("%X", 255) == "FF"
    @test sprintf("%x", 16) == "10"
    @test sprintf("%x", 0) == "0"
end

@testset "sprintf octal formatting (%o)" begin
    @test sprintf("%o", 8) == "10"
    @test sprintf("%o", 7) == "7"
    @test sprintf("%o", 0) == "0"
    @test sprintf("%o", 64) == "100"
end

@testset "sprintf float formatting (%f)" begin
    @test sprintf("%f", 3.14) == "3.14"
    @test sprintf("%f", -2.5) == "-2.5"
    @test sprintf("%e", 1.5) == "1.5"
end

@testset "sprintf char formatting (%c)" begin
    @test sprintf("%c", 65) == "A"
    @test sprintf("%c", 97) == "a"
    @test sprintf("%c", 48) == "0"
end

@testset "sprintf percent literal (%%)" begin
    @test sprintf("100%%") == "100%"
    @test sprintf("%%") == "%"
    @test sprintf("%d%%", 50) == "50%"
end

@testset "sprintf multiple arguments" begin
    @test sprintf("%d %d %d", 1, 2, 3) == "1 2 3"
    @test sprintf("%s=%d", "x", 10) == "x=10"
    @test sprintf("%s is %d", "answer", 42) == "answer is 42"
    @test sprintf("%d+%d=%d", 1, 2, 3) == "1+2=3"
end

@testset "sprintf no format specifiers" begin
    @test sprintf("hello world") == "hello world"
    @test sprintf("") == ""
end

true
