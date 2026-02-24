# Verify dispatch priority: concrete types and parametric types
# always take precedence over the Number fallback

using Test

@testset "Dispatch priority over Number fallback" begin
    # Concrete type methods should win over Number fallback
    # Int64 + Int64 -> add_int (not promote -> add_int)
    @test typeof(1 + 2) == Int64

    # Float64 + Float64 -> add_float (not promote -> add_float)
    @test typeof(1.0 + 2.0) == Float64

    # Float32 + Float32 -> Float32 method (not Number fallback)
    @test typeof(Float32(1.0) + Float32(2.0)) == Float32

    # Mixed types go through Number fallback and produce correct promoted type
    @test typeof(Float32(1.0) + 2) == Float32
    @test typeof(1 + Float32(2.0)) == Float32
end

true
