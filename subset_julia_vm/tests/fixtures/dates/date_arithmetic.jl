# Test Date arithmetic

using Test
using Dates

@testset "Date + Day arithmetic" begin
    d1 = Date(2024, 1, 1)
    d2 = d1 + Day(100)
    # January 1 + 100 days should be April 10, 2024
    @test (Float64(month(d2) * 100 + day(d2))) == 410.0
end

true  # Test passed
