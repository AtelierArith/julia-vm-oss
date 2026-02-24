# Test: Loop variable type inference
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function sum_array(arr)
    total = 0
    for x in arr  # x should be inferred as Int64
        total += x
    end
    total
end

@testset "Loop variable type inference" begin
    @test sum_array([1, 2, 3]) == 6
    @test sum_array(Int64[]) == 0
end

true
