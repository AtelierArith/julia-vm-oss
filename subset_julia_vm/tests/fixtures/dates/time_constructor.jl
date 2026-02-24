# Test Time constructor

using Test
using Dates

@testset "Time constructor and hour/minute/second accessors" begin
    t = Time(14, 30, 15)
    @test (Float64(hour(t) * 100 + minute(t) + second(t))) == 1445.0
end

true  # Test passed
