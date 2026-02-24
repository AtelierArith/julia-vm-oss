# Array comprehension: sum of squares 1+4+9+16 = 30

using Test

@testset "Array comprehension" begin
    arr = [x^2 for x in 1:4]
    @test (sum(arr)) == 30.0
end

true  # Test passed
