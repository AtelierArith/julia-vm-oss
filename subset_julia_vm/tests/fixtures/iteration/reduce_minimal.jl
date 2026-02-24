# Minimal test for reduce-like operation

using Test

add(a, b) = a + b

# Simple reduce implementation
function simple_reduce(op::Function, arr)
    y = iterate(arr)
    if y === nothing
        error("empty")
    end
    acc = y[1]
    y = iterate(arr, y[2])
    while y !== nothing
        acc = op(acc, y[1])
        y = iterate(arr, y[2])
    end
    return acc
end

@testset "Simple reduce" begin
    r1 = simple_reduce(add, [1, 2, 3, 4])
    @test r1 == 10
end

true
