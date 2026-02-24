# Test basic display stack functionality
# Verifies AbstractDisplay, TextDisplay types and basic display function

using Test

@testset "Display stack basic functionality" begin
    # Test that AbstractDisplay type exists
    @test isa(AbstractDisplay, DataType)

    # Test that TextDisplay is a subtype of AbstractDisplay
    @test TextDisplay <: AbstractDisplay

    # Test that TextDisplay can be constructed with stdout
    d = TextDisplay(stdout)
    @test isa(d, TextDisplay)
    @test isa(d, AbstractDisplay)

    # Test that displays array exists
    @test isa(displays, Array)
end

true
