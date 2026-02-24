# Conditional comprehension - transform + filter
# Square odd numbers: [1, 9, 25, 49, 81]

using Test

@testset "Conditional comprehension - transform + filter (square odd numbers)" begin
    result = [x^2 for x in 1:10 if x % 2 == 1]
    @test (sum(result)) == 165.0
end

true  # Test passed
