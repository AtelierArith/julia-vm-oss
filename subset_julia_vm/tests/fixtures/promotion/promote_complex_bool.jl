using Test

# Test promote with Complex and Bool (Issue #2257).
# This tests that the promote_hardcoded whitelist correctly
# falls back to Julia for Complex types.

@testset "promote Complex + Bool" begin
    c = Complex(1.0, 2.0)
    r = promote(true, c)
    # Bool should be promoted to Complex{Float64}
    @test typeof(r[1]) == Complex{Float64}
    @test typeof(r[2]) == Complex{Float64}
    @test r[1] == Complex(1.0, 0.0)
    @test r[2] == c
end

@testset "promote Bool + Complex order" begin
    c = Complex(3.0, 4.0)
    r = promote(c, false)
    @test typeof(r[1]) == Complex{Float64}
    @test typeof(r[2]) == Complex{Float64}
    @test r[1] == c
    @test r[2] == Complex(0.0, 0.0)
end

@testset "promote Complex + Int64" begin
    c = Complex(1.0, 2.0)
    r = promote(c, Int64(5))
    @test typeof(r[1]) == Complex{Float64}
    @test typeof(r[2]) == Complex{Float64}
    @test r[1] == c
    @test r[2] == Complex(5.0, 0.0)
end

true
