# Test Month arithmetic

using Test
using Dates

@testset "Date + Month arithmetic with day clamping" begin
    d1 = Date(2024, 1, 31)
    d2 = d1 + Month(1)  # 2024-02-29 (clamps to last day of Feb)
    d3 = d1 + Month(3)  # 2024-04-30 (clamps to last day of Apr)
    @test (Float64(day(d2) * 100 + day(d3))) == 2930.0
end

true  # Test passed
