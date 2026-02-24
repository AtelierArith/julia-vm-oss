# Test promote_rule default fallback
# The default promote_rule returns Union{} (no rule defined)

using Test

@testset "promote_rule default fallback" begin
    # Same type always returns that type via promote_type
    @test promote_type(Int64, Int64) == Int64
    @test promote_type(Float64, Float64) == Float64

    # promote_type works by calling promote_rule both ways
    # If neither returns a rule, it falls back to Any
end

true
