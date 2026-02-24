# Simple union test

using Test

@testset "Simple Set union" begin
    a = Set([1, 2, 3])
    b = Set([2, 3, 4])
    u = union(a, b)
    @test (length(u)) == 4.0
end

true  # Test passed
