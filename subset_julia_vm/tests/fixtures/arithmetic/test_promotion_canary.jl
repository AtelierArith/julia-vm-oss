# Prevention canary test: promotion still works after base library changes (Issue #2721)
# If operators.jl exceeds the function index limit or method distribution breaks,
# promotion-based arithmetic will fail. This test catches such regressions early.

using Test

@testset "promotion canary" begin
    # Int + Float promotion
    @test 1 + 2.0 == 3.0
    @test 2 * 3.5 == 7.0

    # Float32 + Float64 promotion
    @test Float32(1.0) + 2.0 == 3.0

    # Comparison across types
    @test 1 < 2.0
    @test 3.0 >= 3

    # isequal with numeric types
    @test isequal(1, 1) == true
    @test isequal(1, 2) == false
    @test isequal(1.0, 1.0) == true

    # Mixed-type min/max (requires promotion)
    @test min(1, 2.0) == 1.0
    @test max(1, 2.0) == 2.0
end

true
