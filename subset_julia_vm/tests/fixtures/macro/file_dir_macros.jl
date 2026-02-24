# Test @__FILE__ and @__DIR__ macros
# These macros return information about the source file at compile time

using Test

@testset "@__FILE__ and @__DIR__ macros" begin
    # @__FILE__ returns the current file path as a string
    file = @__FILE__
    @test typeof(file) == String
    @test length(file) > 0

    # @__DIR__ returns the directory of the current file as a string
    dir = @__DIR__
    @test typeof(dir) == String
    @test length(dir) > 0

    # Both should be strings
    @test isa(file, String)
    @test isa(dir, String)
end

true
