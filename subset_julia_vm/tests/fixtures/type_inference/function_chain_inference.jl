# Function chain type inference test
# Tests inter-procedural type propagation through function call chains

using Test

# Helper function that performs addition
function helper_add(x, y)
    return x + y
end

# Helper function that doubles a value
function helper_double(x)
    return x * 2
end

# Caller function that chains multiple helper calls
function caller_chain(a, b)
    sum_result = helper_add(a, b)
    doubled = helper_double(sum_result)
    return doubled
end

# Function with type-dependent return (polymorphic)
function identity_int(x)
    return x
end

function identity_float(x)
    return x
end

@testset "Inter-procedural function call chain type inference" begin
    # Test basic function chain with Int64
    @test caller_chain(1, 2) == 6  # (1+2)*2 = 6

    # Test basic function chain with Float64
    @test caller_chain(1.0, 2.0) == 6.0

    # Test helper functions directly
    @test helper_add(10, 20) == 30
    @test helper_double(5) == 10

    # Test polymorphic identity functions with numeric types
    @test identity_int(42) == 42
    @test identity_int(100) == 100
    @test identity_float(3.14) == 3.14
    @test identity_float(2.71) == 2.71

    # Test nested function calls
    @test helper_add(helper_double(2), helper_double(3)) == 10  # 4 + 6 = 10

    # Test multiple chained calls
    @test helper_double(helper_double(helper_double(1))) == 8  # 1*2*2*2 = 8
end

true
