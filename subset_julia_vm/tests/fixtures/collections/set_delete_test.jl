# Test delete! on Set

using Test

@testset "Test push! and delete! on Set" begin
    f = Set([10, 20])
    push!(f, 30)
    l1 = length(f)  # Should be 3
    delete!(f, 10)
    l2 = length(f)  # Should be 2
    @test (l1 * 10 + l2) == 32.0
end

true  # Test passed
