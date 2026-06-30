# Test size() returns tuple with correct arity for multidimensional arrays (Issue #3463)

using Test

@testset "type_inference_size_tuple_arity: size reflects array dimensions" begin
    v = [1.0, 2.0, 3.0]
    m = [1.0 2.0; 3.0 4.0]

    # 1D: size returns Tuple{Int64}
    @test typeof(size(v)) == Tuple{Int64}
    @test size(v) == (3,)

    # 2D: size returns Tuple{Int64, Int64}
    @test typeof(size(m)) == Tuple{Int64, Int64}
    @test size(m) == (2, 2)

    # size with dim index returns Int64
    @test typeof(size(m, 1)) == Int64
    @test typeof(size(m, 2)) == Int64
end

true
