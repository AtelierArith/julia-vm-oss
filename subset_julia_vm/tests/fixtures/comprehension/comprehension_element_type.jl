# Comprehension element type preservation (Issue #2125)
# In Julia, array comprehensions preserve the element type of the body expression.
# [x for x in 1:5] should produce Vector{Int64}, not Vector{Float64}.

using Test

@testset "Comprehension element type preservation (Issue #2125)" begin
    # Integer range → Vector{Int64}
    r1 = [x for x in 1:5]
    @test r1 == [1, 2, 3, 4, 5]
    @test typeof(r1) == Vector{Int64}

    # Integer expression → Vector{Int64}
    r2 = [x^2 for x in 1:4]
    @test r2 == [1, 4, 9, 16]
    @test typeof(r2) == Vector{Int64}

    # Filtered comprehension → Vector{Int64}
    r3 = [x for x in 1:10 if iseven(x)]
    @test r3 == [2, 4, 6, 8, 10]
    @test typeof(r3) == Vector{Int64}

    # Filtered with expression → Vector{Int64}
    r4 = [x^2 for x in 1:10 if isodd(x)]
    @test r4 == [1, 9, 25, 49, 81]
    @test typeof(r4) == Vector{Int64}

    # Integer arithmetic → Vector{Int64}
    r5 = [2*x + 1 for x in 1:5]
    @test r5 == [3, 5, 7, 9, 11]
    @test typeof(r5) == Vector{Int64}
end

true
