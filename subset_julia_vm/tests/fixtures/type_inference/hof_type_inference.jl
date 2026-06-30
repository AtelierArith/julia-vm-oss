# Higher-order function type inference test
# Tests type inference for map and filter with lambda functions
#
# NOTE: Nested HOF calls have a known bug (Issue #1361).
# Tests 5 and 6 use intermediate variables as a workaround.

using Test

@testset "Higher-order function type inference" begin
    # Test 1: map with addition lambda (type preserved for same-type ops)
    result1 = map(x -> x + 1, [1, 2, 3])
    @test result1 == [2, 3, 4]
    @test length(result1) == 3

    # Test 2: map with multiplication lambda
    result2 = map(x -> x * 2, [1, 2, 3])
    @test result2 == [2, 4, 6]

    # Test 3: filter (type should be preserved)
    result3 = filter(x -> x > 0, [-1, 0, 1, 2, 3])
    @test result3 == [1, 2, 3]

    # Test 4: map with Float64 array
    result4 = map(x -> x + 0.5, [1.0, 2.0, 3.0])
    @test result4 == [1.5, 2.5, 3.5]

    # Test 4b: inline lambda return type feeds map result eltype inference
    result4b = map(x -> x * 2.0, [1, 2, 3])
    @test result4b == [2.0, 4.0, 6.0]
    @test typeof(result4b) === Vector{Float64}

    # Test 4c: qualified HOF calls use the same inline lambda return inference
    result4c = Base.map(x -> x * 2.0, [1, 2, 3])
    @test result4c == [2.0, 4.0, 6.0]
    @test typeof(result4c) === Vector{Float64}

    result4d = Base.broadcast(x -> x * 2.0, [1, 2, 3])
    @test result4d == [2.0, 4.0, 6.0]
    @test typeof(result4d) === Vector{Float64}

    # Test 5: nested map (Issue #1361 workaround: use intermediate variable)
    inner5 = map(x -> x + 1, [1, 2, 3])
    result5 = map(x -> x * 2, inner5)
    @test result5 == [4, 6, 8]

    # Test 6: chained filter and map (Issue #1361 workaround: use intermediate variable)
    filtered6 = filter(x -> x > 0, [-1, 0, 1, 2, 3])
    result6 = map(x -> x * 2, filtered6)
    @test result6 == [2, 4, 6]

    # Test 7: map with square function
    result7 = map(x -> x * x, [1, 2, 3, 4])
    @test result7 == [1, 4, 9, 16]

    # Test 8: inline lambda return type feeds reduce result inference
    result8 = reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    @test result8 == 3.5
    @test typeof(result8) === Float64

    # Test 9: qualified reduction HOF calls use the same inline lambda inference
    result9 = Base.reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    @test result9 == 3.5
    @test typeof(result9) === Float64

    result9b = Base.mapreduce(x -> x * 0.5, +, [1, 2, 3])
    @test result9b == 3.0
    @test typeof(result9b) === Float64

    # Test 10: qualified reduction HOF init keyword calls use the positional rewrite
    @test Base.reduce(+, [1, 2, 3]; init = 10) == 16
    @test Base.foldl(+, [1, 2, 3]; init = 10) == 16
    @test Base.foldr(+, [1, 2, 3]; init = 10) == 16
    @test Base.mapreduce(identity, +, [1, 2, 3]; init = 10) == 16
    @test Base.mapfoldl(identity, +, [1, 2, 3]; init = 10) == 16
    @test Base.mapfoldr(identity, +, [1, 2, 3]; init = 10) == 16
end

true
