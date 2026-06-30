using Test
using Printf
@testset "@sprintf in value position passes all arguments (Issue #5683)" begin
    @test @sprintf("%d", 42) == "42"
    @test @sprintf("%d-%d", 1, 2) == "1-2"
    @test @sprintf("%d,%d,%d", 1, 2, 3) == "1,2,3"
    @test @sprintf("%s", "hi") == "hi"
    @test @sprintf("%s and %s", "a", "b") == "a and b"
    @test @sprintf("hello") == "hello"
    # assigned and used
    s = @sprintf("%d", 7)
    @test s == "7"
    @test s * "!" == "7!"
    # nested in expressions
    @test length(@sprintf("%d%d", 1, 2)) == 2
    @test uppercase(@sprintf("%s", "x")) == "X"
end
true
