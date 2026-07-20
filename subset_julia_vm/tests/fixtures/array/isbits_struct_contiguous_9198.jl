# Issue #9198 S4: byte-contiguous unboxed storage for arrays of an all-`Float64`
# isbits immutable struct (`ArrayElementType::StructInlineF64` /
# `ArrayData::StructF64`). `Vector{Vec2}(undef, n)` stores the fields as raw
# interleaved f64 (like upstream's inline `jl_get_genericmemory_layout` case 1),
# so `sizeof` matches upstream's `n * field_count * 8` instead of the boxed
# `n * 8`. Construction / getindex / setindex! / iteration / collect / push! are
# value-parity with upstream; `Complex{Float64}` arrays stay correct.
#
# Verified against upstream Julia 1.12.

using Test

struct Vec2
    x::Float64
    y::Float64
end

struct Vec3
    x::Float64
    y::Float64
    z::Float64
end

function build(n)
    a = Vector{Vec2}(undef, n)
    for i in 1:n
        a[i] = Vec2(i * 1.0, i * 2.0)
    end
    a
end

function total(a)
    s = 0.0
    for v in a
        s += v.x + v.y
    end
    s
end

@testset "Vector{Vec2} contiguous unboxed storage (Issue #9198 S4)" begin
    a = build(3)
    # eltype / typeof preserved (not widened to Any)
    @test eltype(a) === Vec2
    @test typeof(a) === Vector{Vec2}
    # sizeof is the unboxed layout: 3 elements × 2 Float64 × 8 = 48 bytes
    @test sizeof(a) == 48
    @test sizeof(a) == 3 * sizeof(Vec2)
    # getindex reconstructs the concrete named struct
    @test a[1] == Vec2(1.0, 2.0)
    @test a[2].x == 2.0
    @test a[2].y == 4.0
    # iteration + field access
    @test total(a) == 18.0
    # setindex! round-trip
    a[1] = Vec2(10.0, 20.0)
    @test a[1] == Vec2(10.0, 20.0)
    @test a[2] == Vec2(2.0, 4.0)
end

@testset "collect / push! over Vector{Vec2} (Issue #9198 S4)" begin
    a = build(3)
    c = collect(a)
    @test typeof(c) === Vector{Vec2}
    @test c == a
    @test sizeof(c) == 48

    push!(a, Vec2(100.0, 200.0))
    @test length(a) == 4
    @test a[4] == Vec2(100.0, 200.0)
    @test a[1] == Vec2(1.0, 2.0)
end

@testset "map / comprehension over Vector{Vec2} (Issue #9198 S4)" begin
    a = build(3)
    @test map(v -> v.x + v.y, a) == [3.0, 6.0, 9.0]
    @test sum(v -> v.x, a) == 6.0
    @test [v.y for v in a] == [2.0, 4.0, 6.0]
end

@testset "N-field all-Float64 struct + empty (Issue #9198 S4)" begin
    d = Vector{Vec3}(undef, 2)
    d[1] = Vec3(1.0, 2.0, 3.0)
    d[2] = Vec3(4.0, 5.0, 6.0)
    @test typeof(d) === Vector{Vec3}
    # 2 elements × 3 Float64 × 8 = 48 bytes
    @test sizeof(d) == 48
    @test d[1] == Vec3(1.0, 2.0, 3.0)
    @test d[2].z == 6.0

    e = Vector{Vec2}(undef, 0)
    @test sizeof(e) == 0
    @test eltype(e) === Vec2
    push!(e, Vec2(7.0, 8.0))
    @test e[1] == Vec2(7.0, 8.0)
end

@testset "Complex{Float64} arrays stay contiguous & correct (Issue #9198 S4)" begin
    c = Vector{Complex{Float64}}(undef, 5)
    for i in 1:5
        c[i] = Complex(i * 1.0, -i * 1.0)
    end
    # 5 elements × 16 bytes
    @test sizeof(c) == 80
    @test c[3] == Complex(3.0, -3.0)
    @test sum(c) == Complex(15.0, -15.0)
end

true
