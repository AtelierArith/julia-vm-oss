using Test

@testset "N-ary Int32 broadcast singleton fallback (Issue #5499)" begin
    result = broadcast(+, Int32[1, 2], Int32[10], Int32[100, 200])

    @test result == Int32[111, 212]
    @test typeof(result) == Vector{Int32}
end

true
