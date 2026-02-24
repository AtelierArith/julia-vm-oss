# Test Union type preservation in codegen (Issue #1682)
# Ensures Union types don't collapse to Any during compilation
# These tests complement union_types.jl with additional patterns

using Test

# Function returning Union{Int64, Float64} based on condition
function get_union_numeric(flag)
    if flag == true
        42        # Int64
    else
        3.14      # Float64
    end
end

# Function returning Union{Nothing, Int64} (common pattern with iterate)
function maybe_int(flag)
    if flag == true
        100       # Int64
    else
        nothing   # Nothing
    end
end

# Function that uses iterate (returns Union{Nothing, Tuple})
function first_element(arr)
    iter = iterate(arr)
    if iter === nothing
        return nothing
    else
        return iter[1]
    end
end

# Function with nested conditionals returning Union{Int64, Float64}
function nested_numeric(a, b)
    if a == true
        if b == true
            1         # Int64
        else
            1.0       # Float64
        end
    else
        2.5           # Float64
    end
end

# Compute results outside of testset to avoid potential scoping issues
result_numeric_true = get_union_numeric(true)
result_numeric_false = get_union_numeric(false)
result_maybe_true = maybe_int(true)
result_maybe_false = maybe_int(false)
result_first_nonempty = first_element([1, 2, 3])
result_first_empty = first_element(Int64[])
result_nested_tt = nested_numeric(true, true)
result_nested_tf = nested_numeric(true, false)
result_nested_ft = nested_numeric(false, true)

@testset "Union type preservation in codegen" begin
    # Test basic Union{Int64, Float64}
    @test result_numeric_true == 42
    @test result_numeric_false == 3.14

    # Test Union{Nothing, Int64}
    @test result_maybe_true == 100
    @test result_maybe_false === nothing

    # Test iterate Union preservation
    @test result_first_nonempty == 1
    @test result_first_empty === nothing

    # Test nested Union
    @test result_nested_tt == 1
    @test result_nested_tf == 1.0
    @test result_nested_ft == 2.5
end

true
