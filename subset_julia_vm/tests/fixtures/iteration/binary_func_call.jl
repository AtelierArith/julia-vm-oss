# Test calling binary function passed as parameter

using Test

# Define binary functions
add(a, b) = a + b
mult(a, b) = a * b

# Function that takes a binary operator and applies it
function apply_op(op::Function, x, y)
    return op(x, y)
end

@testset "Binary function parameter call" begin
    @test apply_op(add, 3, 4) == 7
    @test apply_op(mult, 3, 4) == 12
end

true
