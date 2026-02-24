# Test Date difference

using Test
using Dates

@testset "Date subtraction returns Day period" begin
    d1 = Date(2024, 1, 1)
    d2 = Date(2024, 1, 11)
    diff = d2 - d1  # Should be 10 days
    @test (Float64(value(diff))) == 10.0
end

true  # Test passed
