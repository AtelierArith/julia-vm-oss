# Direct-vs-callable array constructor parity audit (Issue #10213, #10250).
#
# Array constructors have two entry paths that can diverge:
#   - direct syntax: `Vector(src)` / `Vector{T}(src)`, intercepted at compile
#     time in `compile_array_constructor`.
#   - first-class callable use: `map(Vector, xs)` / `broadcast(Vector, xs)` /
#     `Vector.(xs)` / `f = Vector; f(x)`, which dispatch through Base methods
#     at runtime.
#
# #10085 (PR #10193) showed these can diverge: `Vector(::Vector)` existed
# directly but was missing as a callable Base method, and worse, the direct
# path returned the source vector by identity instead of copying it. This
# fixture pins parity across same-eltype, converting, range, tuple, and
# comprehension sources for both entry paths. Every assertion below is
# verified to hold, value-for-value, under `julia --startup-file=no` as well
# as sjulia (`bash scripts/fixture_julia_parity.sh`).
#
# Historical divergences discovered by this audit now have executable companion
# coverage. They stay outside this fixture only when the observation belongs to
# a different level (for example, the outer HOF result eltype rather than the
# one constructed element):
#
#   - #10187 outer-eltype parity: `map_vector_ctor_outer_eltype_10187.jl`.
#   - #10406 catchable callable-dispatch failures and broader parametric
#     callable parity: `parametric_ctor_callable_parity_10502.jl`.
#   - #10475 dotted parametric targets: `dotted_parametric_constructor_10475.jl`.
#
# All three issues are closed. `scripts/metamorphic_equivalence.sh --lane
# direct_callable` is the cross-entry value/type/exception ratchet above these
# focused upstream-parity fixtures.
#
# Issue #10404 (fixed alongside this fixture): the tuple-argument
# `MethodError` for a *typed* constructor (`Vector{Int64}((1,2,3))`) carried
# the wrong `f` field (`String` instead of `Vector{Int64}`). This fixture
# includes a regression assertion for that fix (see the tuple-source
# testset), in addition to the pre-existing `@test_throws MethodError`
# coverage in `array_agg_misc_9671.jl` (Issue #5041).

using Test

@testset "same-eltype vector source: direct vs callable copy semantics (Issue #10085 shape)" begin
    src = [1, 2, 3]

    direct = Vector(src)
    typed_direct = Vector{Int64}(src)
    mapped = map(Vector, [src])[1]
    typed_mapped = map(Vector{Int64}, [src])[1]
    broadcast_mapped = broadcast(Vector, [src])[1]
    typed_broadcast_mapped = broadcast(Vector{Int64}, [src])[1]
    dot_mapped = Vector.([src])[1]
    callable_var = (Vector)(src)
    typed_callable_var = (Vector{Int64})(src)
    f = Vector{Int64}
    typed_bound_callable = f(src)

    for v in (
        direct,
        typed_direct,
        mapped,
        typed_mapped,
        broadcast_mapped,
        typed_broadcast_mapped,
        dot_mapped,
        callable_var,
        typed_callable_var,
        typed_bound_callable,
    )
        @test v == src
        @test typeof(v) === Vector{Int64}
        @test !(v === src)
    end

    # Deep-copy check: mutating any of the constructed vectors must not
    # affect the source (the #10085 regression was exactly this: direct
    # syntax returned `src` unchanged, so mutating the "copy" mutated `src`
    # too).
    direct[1] = 100
    mapped[1] = 200
    callable_var[1] = 300
    @test src == [1, 2, 3]
    @test direct == [100, 2, 3]
    @test mapped == [200, 2, 3]
    @test callable_var == [300, 2, 3]
end

@testset "converting vector source: direct vs parametric callable" begin
    src = [1, 2, 3]
    v3 = Vector{Float64}(src)
    mapped = map(Vector{Float64}, [src])[1]
    broadcast_mapped = broadcast(Vector{Float64}, [src])[1]
    callable_var = (Vector{Float64})(src)
    f = Vector{Float64}
    bound_callable = f(src)

    for v in (v3, mapped, broadcast_mapped, callable_var, bound_callable)
        @test v == [1.0, 2.0, 3.0]
        @test typeof(v) === Vector{Float64}
        @test !(v === src)
    end
end

@testset "range source: direct vs callable" begin
    r = 1:3
    vr = Vector(r)
    @test vr == [1, 2, 3]
    @test typeof(vr) === Vector{Int64}

    vr2 = Vector{Float64}(r)
    @test vr2 == [1.0, 2.0, 3.0]
    @test typeof(vr2) === Vector{Float64}

    # Callable path (this fixture checks the constructed element; companion
    # #10187 coverage checks the outer map result's eltype).
    mapped_r = map(Vector, [r])[1]
    @test mapped_r == [1, 2, 3]
    @test typeof(mapped_r) === Vector{Int64}

    typed_mapped_r = map(Vector{Int64}, [r])[1]
    @test typed_mapped_r == [1, 2, 3]
    @test typeof(typed_mapped_r) === Vector{Int64}

    converted_mapped_r = map(Vector{Float64}, [r])[1]
    @test converted_mapped_r == [1.0, 2.0, 3.0]
    @test typeof(converted_mapped_r) === Vector{Float64}
end

@testset "comprehension source: direct syntax" begin
    vc = Vector([x for x in 1:3])
    @test vc == [1, 2, 3]
    @test typeof(vc) === Vector{Int64}
end

@testset "tuple source: direct and callable both raise a catchable MethodError (Issue #5041)" begin
    @test_throws MethodError Vector((1, 2, 3))
    @test_throws MethodError Vector{Int64}((1, 2, 3))
    @test_throws MethodError map(Vector, [(1, 2), (3, 4)])

    # Regression check for Issue #10404: the typed tuple MethodError's `f`
    # field must name the constructor (`Vector{Int64}`), not `String`.
    caught = nothing
    try
        Vector{Int64}((1, 2, 3))
    catch e
        caught = e
    end
    @test caught isa MethodError
    @test caught.f === Vector{Int64}
end

@testset "Array / Matrix direct-syntax copy parity baseline" begin
    src = [1, 2, 3]
    av = Array(src)
    @test av == src
    @test typeof(av) === Vector{Int64}
    @test !(av === src)

    av2 = Array{Int64}(src)
    @test av2 == src
    @test typeof(av2) === Vector{Int64}
    @test !(av2 === src)

    mapped_a = map(Array, [src])[1]
    @test mapped_a == src
    @test typeof(mapped_a) === Vector{Int64}
    @test !(mapped_a === src)

    callable_a = (Array)(src)
    @test callable_a == src
    @test typeof(callable_a) === Vector{Int64}
    @test !(callable_a === src)

    typed_mapped_a = map(Array{Float64}, [src])[1]
    @test typed_mapped_a == [1.0, 2.0, 3.0]
    @test typeof(typed_mapped_a) === Vector{Float64}
    @test !(typed_mapped_a === src)

    m = [1 2; 3 4]
    mm = Matrix(m)
    @test mm == m
    @test typeof(mm) === Matrix{Int64}
    @test !(mm === m)

    mm2 = Matrix{Int64}(m)
    @test mm2 == m
    @test typeof(mm2) === Matrix{Int64}
    @test !(mm2 === m)
end

@testset "Matrix callable parity" begin
    m = [1 2; 3 4]
    mapped_m = map(Matrix, [m])[1]
    @test mapped_m == m
    @test typeof(mapped_m) === Matrix{Int64}
    @test !(mapped_m === m)

    typed_mapped_m = map(Matrix{Int64}, [m])[1]
    @test typed_mapped_m == m
    @test typeof(typed_mapped_m) === Matrix{Int64}
    @test !(typed_mapped_m === m)

    converted_mapped_m = map(Matrix{Float64}, [m])[1]
    @test converted_mapped_m == [1.0 2.0; 3.0 4.0]
    @test typeof(converted_mapped_m) === Matrix{Float64}
    @test !(converted_mapped_m === m)
end

true
