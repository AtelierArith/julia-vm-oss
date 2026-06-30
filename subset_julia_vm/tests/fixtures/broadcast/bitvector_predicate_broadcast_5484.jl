using Test

@testset "Predicate broadcast returns BitVector (Issue #5484)" begin
    xs = [-3, 0, 4]
    bz = broadcast(iszero, xs)
    bs = broadcast(signbit, xs)

    @test typeof(bz) == BitVector
    @test typeof(bs) == BitVector
    @test eltype(bz) == Bool
    @test length(bz) == 3
    @test bz[1] == false
    @test bz[2] == true
    @test bz[3] == false
    @test bs[1] == true
    @test bs[2] == false
    @test bs[3] == false
end

true
