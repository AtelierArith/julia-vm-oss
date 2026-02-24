# Test covariant type parameters in function dispatch (Issue #834)

using Test

# Function definitions must be outside @testset

# Test 1: Function with Array{<:Number} constraint
function sum_numbers(arr::Array{<:Number})
    total = 0.0
    for x in arr
        total += x
    end
    total
end

# Test 2: Function with Vector{<:Integer} constraint
function count_positive(arr::Vector{<:Integer})
    count = 0
    for x in arr
        if x > 0
            count += 1
        end
    end
    count
end

# Test 3: Function overloading with covariant types
function process_array(arr::Array{<:Integer})
    "integer array"
end

function process_array(arr::Array{<:AbstractFloat})
    "float array"
end

@testset "Covariant type parameters in dispatch" begin
    # Test 1: Array{<:Number} should accept Int64 arrays
    int_arr = [1, 2, 3, 4, 5]
    @test sum_numbers(int_arr) == 15.0

    # Test 2: Array{<:Number} should accept Float64 arrays
    float_arr = [1.5, 2.5, 3.0]
    @test sum_numbers(float_arr) == 7.0

    # Test 3: Vector{<:Integer} should accept Int64 arrays
    @test count_positive([1, -2, 3, -4, 5]) == 3
    @test count_positive([-1, -2, -3]) == 0

    # Test 4: Dispatch based on element type constraint
    @test process_array([1, 2, 3]) == "integer array"
    @test process_array([1.0, 2.0, 3.0]) == "float array"
end

true
