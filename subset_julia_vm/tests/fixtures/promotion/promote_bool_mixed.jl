using Test

# Test promote with Bool and mixed primitive types (Issue #2250).
# Previously, Bool and small integer types silently passed through
# promote_hardcoded unchanged because needs_julia_promote used a
# blocklist instead of a whitelist.

@testset "promote Bool + Int64" begin
    r = promote(true, Int64(2))
    @test r == (1, 2)
    @test typeof(r[1]) == Int64
    @test typeof(r[2]) == Int64
end

@testset "promote Bool + Float64" begin
    r = promote(true, 3.14)
    @test r[1] == 1.0
    @test typeof(r[1]) == Float64
    @test typeof(r[2]) == Float64
end

@testset "promote Bool + Bool" begin
    r = promote(true, false)
    @test r == (true, false)
    @test typeof(r[1]) == Bool
    @test typeof(r[2]) == Bool
end

@testset "promote Int32 + Int64 (Julia fallback)" begin
    r = promote(Int32(1), Int64(2))
    @test r == (1, 2)
    @test typeof(r[1]) == Int64
    @test typeof(r[2]) == Int64
end

@testset "promote Int8 + Float64 (Julia fallback)" begin
    r = promote(Int8(1), Float64(2.0))
    @test r[1] == 1.0
    @test r[2] == 2.0
    @test typeof(r[1]) == Float64
    @test typeof(r[2]) == Float64
end

true
