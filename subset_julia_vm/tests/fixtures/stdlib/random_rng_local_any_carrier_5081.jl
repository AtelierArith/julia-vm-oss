using Random
using Test

function rng_local_roundtrip_5081()
    rng = Xoshiro(123)
    return rng !== nothing
end

function rng_local_reassign_5081()
    rng = Xoshiro(123)
    rng = 42
    return rng
end

@testset "RNG local carrier consolidation (Issue #5081)" begin
    @test rng_local_roundtrip_5081()
    @test rng_local_reassign_5081() == 42
end

true
