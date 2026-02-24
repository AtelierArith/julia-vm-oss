# similar() function for arrays (Issue #2129)
# Creates an uninitialized array with the same element type and shape (or specified size).

using Test

@testset "similar(a) - same type and shape (Issue #2129)" begin
    a = [1, 2, 3]
    b = similar(a)
    @test typeof(b) == Vector{Int64}
    @test length(b) == 3

    c = [1.0, 2.0, 3.0, 4.0]
    d = similar(c)
    @test typeof(d) == Vector{Float64}
    @test length(d) == 4
end

@testset "similar(a, n) - same type, different size (Issue #2129)" begin
    a = [1, 2, 3]
    b = similar(a, 5)
    @test typeof(b) == Vector{Int64}
    @test length(b) == 5

    c = [1.0, 2.0]
    d = similar(c, 10)
    @test typeof(d) == Vector{Float64}
    @test length(d) == 10

    # Zero-length array
    e = similar(a, 0)
    @test typeof(e) == Vector{Int64}
    @test length(e) == 0
end

true
