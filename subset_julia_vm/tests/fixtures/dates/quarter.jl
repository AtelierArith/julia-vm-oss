# Test quarter function

using Test
using Dates

@testset "quarterofyear function returns 1-4" begin
    q1 = quarterofyear(Date(2024, 2, 15))  # Q1
    q2 = quarterofyear(Date(2024, 5, 15))  # Q2
    q3 = quarterofyear(Date(2024, 8, 15))  # Q3
    q4 = quarterofyear(Date(2024, 11, 15)) # Q4
    @test (Float64(q1 * 1000 + q2 * 100 + q3 * 10 + q4)) == 1234.0
end

true  # Test passed
