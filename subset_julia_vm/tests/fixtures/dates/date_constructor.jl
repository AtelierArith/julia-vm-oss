# Test Date constructor

using Test
using Dates

@testset "Date constructor and year/month/day accessors" begin
    d = Date(2024, 6, 15)
    @test (Float64(year(d) + month(d) + day(d))) == 2045.0
end

true  # Test passed
