# Test: Array element type inference
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function first_element(arr)
    if !isempty(arr)
        return arr[1]  # Should infer element type from array
    else
        return nothing
    end
end

@testset "Array element type inference" begin
    @test first_element([1, 2, 3]) == 1
    @test first_element([1.0, 2.0, 3.0]) == 1.0
    @test first_element(Int64[]) === nothing
end

true
