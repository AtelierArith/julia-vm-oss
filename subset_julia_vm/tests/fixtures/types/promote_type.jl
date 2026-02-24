# Test: promote_type type promotion (Issue #762)

using Test

@testset "promote_type type promotion (Issue #762)" begin
    # Same type returns that type
    @test promote_type(Int64, Int64) == Int64
    @test promote_type(Float64, Float64) == Float64

    # Int64 + Float64 should return Float64 (Issue #762)
    @test promote_type(Int64, Float64) == Float64
    @test promote_type(Float64, Int64) == Float64

    # Smaller integers promote to larger integers
    @test promote_type(Int32, Int64) == Int64
    @test promote_type(Int64, Int32) == Int64

    # Bool promotes to integers
    @test promote_type(Bool, Int64) == Int64
    @test promote_type(Int64, Bool) == Int64

    # Bool promotes to Float64
    @test promote_type(Bool, Float64) == Float64
    @test promote_type(Float64, Bool) == Float64
end

true  # Test passed
