# Conditional comprehension - filter even numbers

using Test

@testset "Conditional comprehension - filter even numbers" begin
    result = [x for x in 1:10 if x % 2 == 0]
    @test (sum(result)) == 30.0
end

true  # Test passed
