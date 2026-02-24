# Test DateTime constructor

using Test
using Dates

@testset "DateTime constructor and hour/minute/second accessors" begin
    dt = DateTime(2024, 6, 15, 10, 30, 45)
    @test (Float64(hour(dt) * 100 + minute(dt) * 10 + second(dt))) == 1345.0
end

true  # Test passed
