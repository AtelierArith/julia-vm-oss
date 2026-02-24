# Test: Positional args with kwargs...
# Combines positional arguments with keyword argument splatting

using Test

function p(x, y; kwargs...)
    return x + y + length(kwargs)
end

@testset "Positional args with kwargs...: function p(x, y; kwargs...)" begin


    result = p(1, 2, a=10, b=20, c=30)
    @test (Float64(result)) == 6.0
end

true  # Test passed
