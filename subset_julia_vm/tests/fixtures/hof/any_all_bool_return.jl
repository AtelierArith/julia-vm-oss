# any(f, arr) and all(f, arr) must return Bool, not Int64 (Issue #2031)

using Test

@testset "any(f, arr) returns Bool" begin
    @test any(isodd, [1, 2, 3]) == true
    @test any(isodd, [2, 4, 6]) == false
    @test typeof(any(isodd, [1, 2, 3])) == Bool
    @test typeof(any(isodd, [2, 4, 6])) == Bool
end

@testset "all(f, arr) returns Bool" begin
    @test all(iseven, [2, 4, 6]) == true
    @test all(iseven, [1, 2, 3]) == false
    @test typeof(all(iseven, [2, 4, 6])) == Bool
    @test typeof(all(iseven, [1, 2, 3])) == Bool
end

@testset "any/all with empty arrays return Bool" begin
    @test any(isodd, Int64[]) == false
    @test all(iseven, Int64[]) == true
    @test typeof(any(isodd, Int64[])) == Bool
    @test typeof(all(iseven, Int64[])) == Bool
end

true
