# Return type annotation - basic test
# f(x)::Int = x converts the return value to Int
# The return type annotation applies convert(ReturnType, result) to the return value.

using Test

function triple(x)::Int64
    x * 3
end

@testset "Function with return type annotation (f(x)::Int syntax)" begin
    @test (triple(7)) == 21.0
end

true  # Test passed
