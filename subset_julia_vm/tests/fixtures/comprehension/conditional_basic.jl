# Conditional comprehension - basic filter
# [x for x in 1:10 if x > 5] returns [6, 7, 8, 9, 10]

using Test

@testset "Conditional comprehension - basic filter (x > 5)" begin
    result = [x for x in 1:10 if x > 5]
    @test (sum(result)) == 40.0
end

true  # Test passed
