# Test return type annotation applies convert() (Issue #415)
# f(x)::Int = expr should be equivalent to f(x) = convert(Int, expr)

using Test

function to_int(x)::Int64
    x
end

function to_float(x)::Float64
    x
end

function explicit_return(x)::Int64
    return x
end

function conditional_return(x)::Int64
    if x > 0
        return x
    else
        return -x
    end
end

function same_type(x)::Float64
    x
end

@testset "Return type annotation applies convert() to return values (Issue #415)" begin

    result = 0.0

    # Test 1: Float to Int conversion via return type annotation

    # convert(Int64, 1.0) should return 1
    if to_int(1.0) == 1
        result = result + 1.0
    end

    # Test 2: Int to Float conversion via return type annotation

    # convert(Float64, 5) should return 5.0
    val = to_float(5)
    if val == 5.0
        result = result + 1.0
    end

    # Test 3: Return type annotation with explicit return

    if explicit_return(3.0) == 3
        result = result + 1.0
    end

    # Test 4: Return type annotation with conditional

    if conditional_return(2.0) == 2
        result = result + 1.0
    end
    if conditional_return(-3.0) == 3
        result = result + 1.0
    end

    # Test 5: Return type annotation preserves value when types match

    # 5.5 is already Float64, should return unchanged
    if same_type(5.5) == 5.5
        result = result + 1.0
    end

    @test (result) == 6.0
end

true  # Test passed
