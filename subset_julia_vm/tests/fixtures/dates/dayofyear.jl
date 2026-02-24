# Test dayofyear function

using Test
using Dates

@testset "dayofyear function for various dates" begin
    d1 = Date(2024, 1, 1)   # Day 1
    d2 = Date(2024, 12, 31) # Day 366 (leap year)
    d3 = Date(2023, 12, 31) # Day 365 (non-leap year)
    @test (Float64(dayofyear(d1) + dayofyear(d2) + dayofyear(d3))) == 732.0
end

true  # Test passed
