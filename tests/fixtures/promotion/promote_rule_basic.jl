# Test basic promote_rule for primitive types
# These rules are defined in subset_julia_vm/src/julia/base/promotion.jl

using Test

@testset "promote_rule basic tests" begin
    # Bool promotes to Int64
    @test promote_rule(Int64, Bool) == Int64

    # Float64 is wider than Int64
    @test promote_rule(Float64, Int64) == Float64

    # Float64 is wider than Float32
    @test promote_rule(Float64, Float32) == Float64

    # Int64 is wider than Int32
    @test promote_rule(Int64, Int32) == Int64
end

true
