# Test last for heterogeneous tuple returns last element type (Issue #3467)
# Julia: typeof(last((1, 2.0))) == Float64

using Test

@testset "tuple_last_heterogeneous: last returns last element of heterogeneous tuple" begin
    t = (1, 2.0)
    @test last(t) == 2.0
    @test typeof(last(t)) == Float64

    t2 = (1, 2.0, "hello")
    @test last(t2) == "hello"
    @test typeof(last(t2)) == String

    t3 = (1, 2.0, 3)
    @test last(t3) == 3
    @test typeof(last(t3)) == Int64
end

true
