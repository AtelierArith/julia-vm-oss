using Test

# Issue #5143: a small-Union-typed array literal (`Union{Int64,Float64}[...]`)
# must store each element verbatim — without coercing `Float64` members to the
# first/`Int64` member — so per-element multiple dispatch picks the correct
# concrete method (the union-splitting correctness goal of #5143).

classify_5143(x::Int64) = "int=$x"
classify_5143(x::Float64) = "float=$x"

@testset "Union-typed array literal preserves each member type" begin
    v = Union{Int64,Float64}[1, 2.5, 3, 4.0]

    # Container stays a Union element type, not a single member. (Compared by
    # rendered name; the `eltype(v) == Union{...}` type-object identity gap is
    # an independent defect tracked in Issue #5335.)
    @test string(typeof(v)) == "Vector{Union{Float64, Int64}}"
    @test string(eltype(v)) == "Union{Float64, Int64}"

    # Each element keeps its own concrete type (no Float64 -> Int64 coercion).
    @test typeof(v[1]) == Int64
    @test typeof(v[2]) == Float64
    @test typeof(v[3]) == Int64
    @test typeof(v[4]) == Float64
    @test v[2] == 2.5
    @test v[4] == 4.0
end

@testset "Union-typed array literal dispatches per element" begin
    v = Union{Int64,Float64}[1, 2.5, 3, 4.0]
    out = String[]
    for x in v
        push!(out, classify_5143(x))
    end
    @test out == ["int=1", "float=2.5", "int=3", "float=4.0"]
end

@testset "single-element Union literal keeps the float member" begin
    a = Union{Int64,Float64}[2.5]
    @test typeof(a[1]) == Float64
    @test a[1] == 2.5
    @test classify_5143(a[1]) == "float=2.5"

    b = Union{Int64,Float64}[2]
    @test typeof(b[1]) == Int64
    @test classify_5143(b[1]) == "int=2"
end

true
