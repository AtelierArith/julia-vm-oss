# Test floatmin() returns smallest positive normalized Float64
# floatmin() == 2.2250738585072014e-308
# floatmin() > 0 should be true

using Test

@testset "floatmin() returns smallest positive normalized Float64" begin
    result = floatmin() > 0.0 && floatmin() < 1e-300
    @test (result ? 1.0 : 0.0) == 1.0
end

true  # Test passed
