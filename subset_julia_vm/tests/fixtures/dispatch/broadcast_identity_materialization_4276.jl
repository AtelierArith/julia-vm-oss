using Test

double_broadcast_4276(x) = x + x

@testset "generic unary broadcast materializes typed arrays (Issue #4276)" begin
    ints = broadcast(identity, [1, 2])
    @test ints == [1, 2]
    @test typeof(ints) == Vector{Int64}
    @test eltype(ints) == Int64

    doubled = broadcast(double_broadcast_4276, [1, 2])
    @test doubled == [2, 4]
    @test typeof(doubled) == Vector{Int64}
    @test eltype(doubled) == Int64

    floats = broadcast(identity, [1.0, 2.0])
    @test floats == [1.0, 2.0]
    @test typeof(floats) == Vector{Float64}
    @test eltype(floats) == Float64
end

true
