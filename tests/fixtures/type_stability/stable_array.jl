# Type-stable array operations

using Test

# Type-stable function: creates Int64 array
function create_range_sum(n::Int64)
    total = 0
    for i in 1:n
        total = total + i
    end
    return total
end

# Type-stable function: calculates sum with Float64
function calculate_sum(n::Int64)
    result = 0.0
    for i in 1:n
        result = result + Float64(i)
    end
    return result
end

@testset "Type-stable array operations" begin
    @test create_range_sum(5) == 15
    @test calculate_sum(4) == 10.0
end

true
