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
end

true
