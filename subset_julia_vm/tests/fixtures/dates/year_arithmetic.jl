# Test Year arithmetic

using Test
using Dates

@testset "Date + Year arithmetic with leap year handling" begin
    d1 = Date(2020, 2, 29)  # Leap year
    d2 = d1 + Year(1)  # 2021-02-28 (Feb 29 doesn't exist in 2021)
    d3 = d1 + Year(4)  # 2024-02-29 (Leap year again)
    @test (Float64(day(d2) * 100 + day(d3))) == 2829.0
end

true  # Test passed
