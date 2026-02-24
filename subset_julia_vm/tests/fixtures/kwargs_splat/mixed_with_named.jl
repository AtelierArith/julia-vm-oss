# Test: Named kwargs combined with kwargs...
# Named kwargs are matched first, remaining go to kwargs...

using Test

function h(; x=0, kwargs...)
    return x + length(kwargs)
end

@testset "Named kwargs combined with kwargs...: function h(; x=0, kwargs...)" begin


    result1 = h(x=5)           # x=5, kwargs=()
    result2 = h(x=5, a=1, b=2) # x=5, kwargs=(a=1, b=2)

    @test (Float64(result1 == 5 && result2 == 7)) == 1.0
end

true  # Test passed
