# Test display with various value types
# Verifies that display works with different types

using Test

@testset "Display with various types" begin
    # Test display(x) returns nothing
    @test display(42) === nothing
    @test display(3.14) === nothing
    @test display("hello") === nothing
    @test display(true) === nothing

    # Test display(d, x) with TextDisplay
    d = TextDisplay(stdout)
    @test display(d, 42) === nothing
    @test display(d, "test") === nothing

    # Test display(m, x) with MIME
    @test display(MIME("text/plain"), 123) === nothing
end

true
