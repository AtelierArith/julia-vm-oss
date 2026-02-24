# Test promote_type bidirectional lookup
# promote_type calls promote_rule in both directions

using Test

@testset "promote_type bidirectional lookup" begin
    # Int64 + Float64 -> Float64 (regardless of order)
    @test promote_type(Int64, Float64) == Float64
    @test promote_type(Float64, Int64) == Float64

    # Bool + Int64 -> Int64 (regardless of order)
    @test promote_type(Bool, Int64) == Int64
    @test promote_type(Int64, Bool) == Int64

    # Bool + Float64 -> Float64 (regardless of order)
    @test promote_type(Bool, Float64) == Float64
    @test promote_type(Float64, Bool) == Float64
end

true
