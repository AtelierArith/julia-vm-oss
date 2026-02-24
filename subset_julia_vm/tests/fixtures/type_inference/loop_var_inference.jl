# Test: Loop variable type inference from iterators
# The type inference engine should infer the type of loop variables
# based on the element type of the iterator.

using Test

# Define functions outside @testset (SubsetJuliaVM limitation)
function sum_int_array(arr)
    total = 0
    for x in arr
        # x should be inferred as Int64
        total += x
    end
    total
end

function sum_float_array(arr)
    total = 0.0
    for x in arr
        # x should be inferred as Float64
        total += x
    end
    total
end

function sum_range()
    total = 0
    for i in 1:5
        # i should be inferred as Int64
        total += i
    end
    total
end

function count_chars(s)
    count = 0
    for c in s
        # c should be inferred as Char
        count += 1
    end
    count
end

@testset "Loop variable type inference" begin
    # Test with integer array
    @test sum_int_array([1, 2, 3]) == 6
    @test sum_int_array([10, 20, 30]) == 60

    # Test with float array
    @test sum_float_array([1.5, 2.5, 3.5]) ≈ 7.5

    # Test with range
    @test sum_range() == 15

    # Test with string iteration
    @test count_chars("hello") == 5
end

true  # Test passed
