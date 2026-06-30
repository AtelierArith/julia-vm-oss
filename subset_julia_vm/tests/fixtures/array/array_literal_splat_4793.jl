# Issue #4793: Splat (v...) inside array literals [a, v..., b] failed to lower.
# Splat already worked in tuple literals and function calls; the array
# literal lowering path now also flattens splats inline by lowering
# `[a, v..., b]` to `vcat([a], v, [b])` and reusing vcat's varargs body.

using Test

@testset "Array literal splat: middle (Issue #4793)" begin
    v = [1, 2, 3]
    a = [10, v..., 20]
    @test length(a) == 5
    @test a == [10, 1, 2, 3, 20]
end

@testset "Array literal splat: start (Issue #4793)" begin
    v = [1, 2, 3]
    b = [v..., 100]
    @test b == [1, 2, 3, 100]
end

@testset "Array literal splat: end (Issue #4793)" begin
    v = [1, 2, 3]
    c = [0, v...]
    @test c == [0, 1, 2, 3]
end

@testset "Array literal splat: lone splat (Issue #4793)" begin
    v = [1, 2, 3]
    d = [v...]
    @test d == [1, 2, 3]
end

@testset "Array literal splat: multiple splats (Issue #4793)" begin
    v = [1, 2, 3]
    w = [4, 5]
    e = [v..., w...]
    @test e == [1, 2, 3, 4, 5]
end

@testset "Array literal splat: interleaved with scalars (Issue #4793)" begin
    v = [1, 2, 3]
    w = [4, 5]
    f = [0, v..., 99, w..., 100]
    @test f == [0, 1, 2, 3, 99, 4, 5, 100]
end

@testset "Array literal splat: tuple splat (Issue #4793)" begin
    t = (4, 5)
    g = [1, 2, t..., 3]
    @test g == [1, 2, 4, 5, 3]
end

@testset "Array literal splat: range splat (Issue #4793)" begin
    h = [0, (1:3)..., 100]
    @test h == [0, 1, 2, 3, 100]
end

@testset "Array literal splat: empty splat (Issue #4793)" begin
    e_empty = Int[]
    i = [1, e_empty..., 3]
    @test i == [1, 3]
end

@testset "Array literal splat: float promotion (Issue #4793)" begin
    fv = [1.0, 2.0]
    j = [10, fv..., 20]
    @test j == [10.0, 1.0, 2.0, 20.0]
    @test eltype(j) == Float64
end

true
