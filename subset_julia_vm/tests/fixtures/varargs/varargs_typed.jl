# Test varargs parameters with type annotations (Issue #1678)
# Typed varargs like (x::Int64, ys::Int64...) should work correctly

using Test

# Helper functions defined outside testset
sum_typed(x::Int64, ys::Int64...) = x + sum(ys)
count_typed(x::Int64, ys::Int64...) = 1 + length(ys)
mul_typed(x::Float64, ys::Float64...) = x * prod(ys)

@testset "Typed varargs parameters" begin
    # Basic typed varargs with Int64
    @test count_typed(1) == 1
    @test count_typed(1, 2) == 2
    @test count_typed(1, 2, 3) == 3
    @test count_typed(1, 2, 3, 4, 5) == 5

    # Typed varargs with computation
    @test sum_typed(10) == 10
    @test sum_typed(10, 5) == 15
    @test sum_typed(1, 2, 3, 4) == 10

    # Typed varargs with Float64
    @test mul_typed(2.0) == 2.0
    @test mul_typed(2.0, 3.0) == 6.0
    @test mul_typed(2.0, 3.0, 4.0) == 24.0
end

true
