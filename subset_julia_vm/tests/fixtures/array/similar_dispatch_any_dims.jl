# Regression fixture for Issue #3777.
#
# The compile-time `similar` dispatch in compile/expr/call/mod.rs previously
# only routed to the builtin when the dim args inferred as a fixed integer
# width. Two common patterns inside a function body fell through to method
# dispatch and errored at runtime with "No method matching similar(...)":
#
#   1. Inline `similar(mat, size(mat, 1), size(mat, 2))` — `BuiltinOp::Size`
#      defaulted to F64 in the Builtin inference table.
#   2. `similar(arr, length(arr) * n)` where `n` is an Any-typed param —
#      `I64 * Any` infers as `Any`.
#
# Both were silently broken before the fix.

using Test

@testset "similar(mat, inline size(...), size(...))" begin
    function f(mat)
        similar(mat, size(mat, 1), size(mat, 2))
    end

    a = [1 2 3; 4 5 6]
    r = f(a)
    @test typeof(r) === Matrix{Int64}
    @test size(r) == (2, 3)

    b = [1.0 2.0; 3.0 4.0]
    s = f(b)
    @test typeof(s) === Matrix{Float64}
    @test size(s) == (2, 2)
end

@testset "similar(arr, length(arr) * n) — Any × Any arithmetic in dim" begin
    function g(arr, n)
        similar(arr, length(arr) * n)
    end

    r = g([1, 2, 3], 4)
    @test typeof(r) === Vector{Int64}
    @test length(r) == 12

    s = g([1.0, 2.0], 3)
    @test typeof(s) === Vector{Float64}
    @test length(s) == 6
end

@testset "similar(arr, total) — local Any value" begin
    function h(arr, n)
        len = length(arr)
        total = len * n
        similar(arr, total)
    end

    r = h([true, false], 5)
    @test typeof(r) === Vector{Bool}
    @test length(r) == 10
end

@testset "repeat(arr, n) round-trip type preservation" begin
    # Pure Julia `repeat` was migrated to similar(arr, total) once #3777 landed.
    @test typeof(repeat([1, 2], 3)) === Vector{Int64}
    @test repeat([1, 2], 3) == [1, 2, 1, 2, 1, 2]
    @test typeof(repeat([true, false], 2)) === Vector{Bool}
    @test typeof(repeat(["a"], 3)) === Vector{String}
end

true
