# Test collection operations through untyped function parameters (Issue #1832)
# When collections are passed through function parameters typed as Any,
# the compiler defaults to Array/Dict instructions. The VM runtime must
# dispatch to the correct collection type at runtime.

using Test

# push! on Set through untyped function parameter
function add_element(c, x)
    push!(c, x)
    return c
end

# delete! on Set through untyped function parameter
function remove_element(c, x)
    delete!(c, x)
    return c
end

# length through untyped function parameter
function get_length(c)
    return length(c)
end

@testset "Collection operations through Any-typed params" begin
    # push! on Set through function
    s = Set([1, 2, 3])
    result = add_element(s, 4)
    @test length(result) == 4
    @test 4 in result

    # push! duplicate element on Set through function
    s2 = Set([10, 20])
    result2 = add_element(s2, 10)
    @test length(result2) == 2

    # delete! on Set through function
    s3 = Set([1, 2, 3])
    result3 = remove_element(s3, 2)
    @test length(result3) == 2
    @test !(2 in result3)

    # length on Set through function
    s4 = Set([5, 6, 7, 8])
    @test get_length(s4) == 4
end

true
