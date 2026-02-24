# findfirst(f, A) - find first index where predicate returns true (Issue #1996)

using Test

@testset "findfirst with predicate" begin
    @test findfirst(x -> x > 3, [1, 2, 5, 4]) == 3
end

true
