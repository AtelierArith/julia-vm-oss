# Dict comprehension tests

using Test

@testset "Dict comprehension: Dict(k => v for k in iter [if cond])" begin

    # Basic Dict comprehension from array
    arr = [1, 2, 3]
    d = Dict(x => x^2 for x in arr)
    # d[1]=1, d[2]=4, d[3]=9

    # Dict comprehension from range
    d2 = Dict(i => 2*i for i in 1:3)
    # d2[1]=2, d2[2]=4, d2[3]=6

    # Dict comprehension with filter
    d3 = Dict(x => x^2 for x in 1:5 if x > 2)
    # d3[3]=9, d3[4]=16, d3[5]=25

    # Verify: 1 + 4 + 9 + 2 + 4 + 6 + 9 + 16 + 25 = 76
    result = d[1] + d[2] + d[3] + d2[1] + d2[2] + d2[3] + d3[3] + d3[4] + d3[5]
    @test (result) == 76.0
end

true  # Test passed
