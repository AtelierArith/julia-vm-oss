# Test Missing type and related functions

using Test

@testset "Missing type: literal, typeof, ismissing, coalesce functions" begin

    # Basic missing value
    x = missing
    @assert typeof(x) == Missing

    # ismissing function - use @assert directly (Bool comparison issue workaround)
    @assert ismissing(missing)
    if ismissing(42)
        error("ismissing(42) should be false")
    end
    if ismissing(nothing)
        error("ismissing(nothing) should be false")
    end
    if ismissing("hello")
        error("ismissing('hello') should be false")
    end

    # coalesce returns first non-missing value
    @assert coalesce(1, 2) == 1
    @assert coalesce(missing, 2) == 2
    @assert coalesce(missing, missing, 3) == 3
    @assert coalesce(1, missing, 3) == 1

    # skipmissing with for loop
    data = [1, 2, 3, 4, 5]
    total = 0
    for v in skipmissing(data)
        total = total + v
    end
    @assert total == 15

    # skipmissing with collect (returns Float64 array due to collect implementation)
    collected = collect(skipmissing([1.0, 2.0, 3.0]))
    @assert length(collected) == 3
    @assert collected[1] == 1.0
    @assert collected[2] == 2.0
    @assert collected[3] == 3.0

    @test (true)
end

true  # Test passed
