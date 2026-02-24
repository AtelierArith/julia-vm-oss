# prod([2, 3, 4]) = 24

using Test

@testset "prod([2,3,4]) = 24 (Pure Julia implementation)" begin
    arr = [2.0, 3.0, 4.0]
    @test (prod(arr)) == 24.0
end

true  # Test passed
