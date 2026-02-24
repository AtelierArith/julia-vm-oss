# Test exported split and rsplit functions

using Test

@testset "split and rsplit exports" begin
    # split - split string by delimiter
    parts = split("a,b,c", ",")
    @test length(parts) == 3
    @test isequal(parts[1], "a")
    @test isequal(parts[2], "b")
    @test isequal(parts[3], "c")

    # rsplit - reverse split (same result for simple cases)
    rparts = rsplit("a,b,c", ",")
    @test length(rparts) == 3
    @test isequal(rparts[1], "a")
    @test isequal(rparts[2], "b")
    @test isequal(rparts[3], "c")
end

true
