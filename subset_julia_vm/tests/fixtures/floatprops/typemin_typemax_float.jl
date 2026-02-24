# typemin/typemax for floating-point types (Issue #2094)
# In Julia, typemin(Float64) = -Inf, typemax(Float64) = Inf

using Test

@testset "typemin/typemax for Float types (Issue #2094)" begin
    # Float64
    @test typemin(Float64) == -Inf
    @test typemax(Float64) == Inf
    @test isinf(typemin(Float64))
    @test isinf(typemax(Float64))
    @test typemin(Float64) < 0
    @test typemax(Float64) > 0

    # Float32
    @test isinf(typemin(Float32))
    @test isinf(typemax(Float32))
    @test typemin(Float32) < 0
    @test typemax(Float32) > 0

    # Float16
    @test isinf(typemin(Float16))
    @test isinf(typemax(Float16))
    @test typemin(Float16) < 0
    @test typemax(Float16) > 0

    # Int64 still works (regression check)
    @test typemin(Int64) < 0
    @test typemax(Int64) > 0
    @test typemax(Int64) == 9223372036854775807
end

true
