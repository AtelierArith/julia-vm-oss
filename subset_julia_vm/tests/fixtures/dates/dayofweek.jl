# Test dayofweek function

using Test
using Dates

@testset "dayofweek returns 1-7 (Monday-Sunday)" begin
    # 2024-06-15 is Saturday (day 6)
    # 2024-06-17 is Monday (day 1)
    d1 = Date(2024, 6, 15)
    d2 = Date(2024, 6, 17)
    @test (Float64(dayofweek(d1) * 10 + dayofweek(d2))) == 61.0
end

true  # Test passed
