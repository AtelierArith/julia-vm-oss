# Test ExponentialBackOff iterator for error handling
# Issue #448: Error handling (エラー処理)

using Test

@testset "ExponentialBackOff" begin
    # Test basic construction
    ebo = ExponentialBackOff(n=3, first_delay=1.0, max_delay=10.0, factor=2.0, jitter=0.0)
    @test ebo.n == 3
    @test ebo.first_delay == 1.0
    @test ebo.max_delay == 10.0
    @test ebo.factor == 2.0
    @test ebo.jitter == 0.0

    # Test default construction
    ebo_default = ExponentialBackOff()
    @test ebo_default.n == 1
    @test ebo_default.first_delay == 0.05

    # Test iteration - with zero jitter, delays should be deterministic
    delays = Float64[]
    for d in ExponentialBackOff(n=3, first_delay=1.0, max_delay=100.0, factor=2.0, jitter=0.0)
        push!(delays, d)
    end
    @test length(delays) == 3
    @test delays[1] == 1.0
    # With factor=2.0 and jitter=0.0, delays should double each time
    @test delays[2] == 2.0
    @test delays[3] == 4.0

    # Test max_delay capping
    delays_capped = Float64[]
    for d in ExponentialBackOff(n=5, first_delay=1.0, max_delay=3.0, factor=2.0, jitter=0.0)
        push!(delays_capped, d)
    end
    @test length(delays_capped) == 5
    @test delays_capped[1] == 1.0
    @test delays_capped[2] == 2.0
    # Third and beyond should be capped at max_delay
    @test delays_capped[3] == 3.0
    @test delays_capped[4] == 3.0
    @test delays_capped[5] == 3.0

    # Test length
    @test length(ExponentialBackOff(n=5)) == 5
end

true
