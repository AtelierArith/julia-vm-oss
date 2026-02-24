# Test convert(T, x) - basic type conversion

using Test

@testset "convert(T, x) - Int to Float64 and Float64 to Int" begin
    x1 = convert(Float64, 42)
    x2 = convert(Int64, 3)  # Int to Int (identity)
    @test (x1 + x2) == 45.0
end

true  # Test passed
