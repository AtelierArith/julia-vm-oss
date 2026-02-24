# Test copy(Dict) returns a Dict (not Vector) (Issue #1821)
# In official Julia, copy(d::Dict) creates a new Dict with the same entries.

using Test

@testset "copy(Dict) returns Dict" begin
    # Basic copy preserves entries
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    c = copy(d)
    @test get(c, "a", 0) == 1
    @test get(c, "b", 0) == 2
    @test get(c, "c", 0) == 3
    @test length(c) == 3

    # copy of empty Dict
    empty_d = Dict{String,Int64}()
    empty_c = copy(empty_d)
    @test length(empty_c) == 0
end

true
