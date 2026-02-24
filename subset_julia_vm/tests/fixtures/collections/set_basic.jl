# Basic Set test - just create a Set and check length

using Test

@testset "Basic Set creation and length" begin
    a = Set([1, 2, 3])
    @test (length(a)) == 3.0
end

true  # Test passed
