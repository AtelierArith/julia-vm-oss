# map(Vector, ::Vector{Vector}) preserves the precise OUTER element type, and the
# direct (`Vector(src)`) vs callable (`map(Vector, [src])`) vs typed
# (`Vector{T}(src)`) array-constructor paths agree on type + copy semantics
# (Issues #10187, #10272, #10250).
#
# Before the fix, `map(Vector, [[1], [2]])` returned `Vector{Array{Any, Any}}`
# even though every copied inner element is `Vector{Int64}`: the value-mode
# collect/map result builder tagged the outer array with the generic `Array`
# struct id instead of the concrete `Vector{Int64}` element type.

using Test

@testset "map(Vector, ::Vector{Vector}) outer eltype (Issue #10187)" begin
    xs = [[1], [2]]
    ys = map(Vector, xs)

    @test typeof(ys) === Vector{Vector{Int64}}
    @test ys isa Vector{Vector{Int64}}
    @test typeof(ys[1]) === Vector{Int64}
    @test ys == [[1], [2]]
    # Elements are fresh copies, not aliases of the source rows.
    @test !(ys[1] === xs[1])
    ys[1][1] = 99
    @test xs == [[1], [2]]

    # Empty result must stay Vector{Any} (matches upstream) — the fix must NOT
    # over-tag an empty map to Vector{Vector{Int64}} (regression lock-in).
    @test typeof(map(Vector, Vector{Int64}[])) === Vector{Any}
end

@testset "map over other callable container constructors" begin
    @test typeof(map(Vector, [[1.0, 2.0], [3.0]])) === Vector{Vector{Float64}}
    @test typeof(map(Matrix, [[1 2; 3 4], [5 6; 7 8]])) === Vector{Matrix{Int64}}
    # A non-constructor callable that returns a fresh vector per element.
    @test typeof(map(x -> [x], [1, 2, 3])) === Vector{Vector{Int64}}
    # Same shape via a generator + collect.
    @test typeof(collect(Vector(x) for x in [[1], [2]])) === Vector{Vector{Int64}}
end

@testset "direct vs callable vs typed array-ctor parity (Issue #10250)" begin
    # For several source element types, `Vector(src)`, `map(Vector, [src])[1]`,
    # and `Vector{T}(src)` must agree on the resulting type and copy semantics.
    src_int = [1, 2, 3]
    src_float = [1.0, 2.0]
    src_str = ["a", "b"]
    src_bool = [true, false]

    for src in Any[src_int, src_float, src_str, src_bool]
        direct = Vector(src)
        callable = map(Vector, [src])[1]
        @test typeof(direct) === typeof(callable)
        @test direct == src == callable
        # Both allocate fresh storage rather than aliasing the source.
        @test !(direct === src)
        @test !(callable === src)
    end

    # Typed constructor converts eltype but keeps copy semantics.
    typed = Vector{Float64}(src_int)
    @test typeof(typed) === Vector{Float64}
    @test typed == [1.0, 2.0, 3.0]
    @test !(typed === src_int)
    @test src_int == [1, 2, 3]

    # The typed and direct forms agree when the eltype already matches.
    @test typeof(Vector{Int64}(src_int)) === typeof(Vector(src_int))
end

true
