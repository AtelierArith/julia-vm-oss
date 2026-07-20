# Scalar chained comparisons evaluate interior operands once (Issue #9375)

using Test

chain_c_9375 = 0
chain_g_9375() = (global chain_c_9375 += 1; 5)

chain_calls_9375 = 0
chain_bump_9375(x) = (global chain_calls_9375 += 1; x)

chain_skipped_9375 = 0
chain_later_9375(x) = (global chain_skipped_9375 += 1; x)

@testset "scalar chained comparison single evaluation (Issue #9375)" begin
    global chain_c_9375 = 0
    r = 0 <= chain_g_9375() < 10
    @test r
    @test chain_c_9375 == 1

    global chain_calls_9375 = 0
    all_true = 0 < chain_bump_9375(1) < chain_bump_9375(2) < 3
    @test all_true
    @test chain_calls_9375 == 2

    global chain_skipped_9375 = 0
    short_circuit = 10 < chain_later_9375(5) < chain_later_9375(6) < 20
    @test !short_circuit
    @test chain_skipped_9375 == 1
end

true
