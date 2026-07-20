# Direct-vs-callable parametric constructor parity audit (Issue #10502).
#
# Prevention follow-up from Issue #10405 / PR #10487: first-class parametric
# constructor values (`ctor = Vector{Float64}; map(ctor, xs)`) dispatch through
# runtime callable `Value::DataType` handling, which augments dispatch
# candidates by scanning `Name{...}` method rows. Two invariants must hold
# (see docs/vm/CHECKLISTS.md, "Parametric DataType callable constructor
# dispatch"):
#
#   1. Candidate augmentation must only add generic TypeVar methods
#      (`Name{T}(...) where {T}`), never concrete instantiation siblings —
#      `Rational{Bool}` is not a generic fallback for `Rational{BigInt}`.
#   2. Generic candidates must not suppress the default `DataType`
#      constructor fallback after a dispatch miss — the `Complex{Int64}(re,
#      im)` class must keep constructing.
#
# This fixture pairs DIRECT parametric constructor syntax with the
# FIRST-CLASS callable forms (bound variable, `map`, `broadcast`) for the
# public parametric constructors beyond the Vector/Array/Matrix coverage in
# `tests/fixtures/array/ctor_direct_vs_callable_parity_10213.jl`:
# Dict{K,V}, Rational{T} (with Bool/Int32/BigInt sibling parameterizations),
# Complex{T}, plus user-defined parametric structs for the generic-TypeVar
# and sibling-isolation directions. Every assertion below is verified,
# value-for-value, under `julia --startup-file=no` 1.12 as well as sjulia
# (`bash scripts/fixture_julia_parity.sh`).
#
# Known, tracked divergences deliberately NOT asserted here (asserting a
# divergent dimension would break the pass/fail parity this fixture gates
# on — same policy as the #10213 fixture header):
#
#   - Issue #10592 (fixed by PR #11476, verified here): once a user
#     outer-constructor method is defined on a concrete instantiation
#     (`CtorAuditBox10502{Int64}(s::String)`), the default constructor for
#     OTHER argument types on that same instantiation
#     (`CtorAuditBox10502{Int64}(7)`) used to misdispatch into the String
#     method and crash. The fix that made the explicit where-parametric
#     outer participate in dispatch without suppressing the default field
#     constructor (Issue #11404) also restored this default-ctor-after-miss
#     path; both directions are now asserted below, direct and
#     bound-callable.
#   - Issue #10593: a parametric default constructor applied to a
#     non-convertible argument (`(CtorAuditBox10502{Float64})("abc")`) is an
#     uncatchable compile error at top level and silently constructs a
#     corrupt instance in function context, where upstream raises a
#     catchable MethodError. The `@test_throws MethodError` sibling guards
#     for the user struct are omitted; the equivalent guards ARE asserted
#     for Base's Rational/Complex callables, which raise MethodError
#     correctly.
#   - Issue #10475 (now closed): dotted calls whose callee is itself a parametric
#     constructor expression (`Vector{T}.(xs)`) fail during lowering; the
#     equivalent paths are covered via `map`/`broadcast`/bound callables.

using Test

@testset "Vector/Array/Matrix bound parametric callable vs direct (Issue #10405 shape)" begin
    xs = [1, 2, 3]

    vctor = Vector{Float64}
    @test vctor(xs) == Vector{Float64}(xs)
    @test typeof(vctor(xs)) === Vector{Float64}
    @test map(vctor, [xs])[1] == [1.0, 2.0, 3.0]

    actor = Array{Float64}
    @test actor(xs) == Array{Float64}(xs)
    @test typeof(actor(xs)) === Vector{Float64}

    m = [1 2; 3 4]
    mctor = Matrix{Float64}
    @test mctor(m) == Matrix{Float64}(m)
    @test typeof(mctor(m)) === Matrix{Float64}
end

@testset "Dict{K,V} direct vs first-class callable" begin
    dctor = Dict{String, Int64}

    d_direct = Dict{String, Int64}("a" => 1, "b" => 2)
    d_callable = dctor("a" => 1, "b" => 2)
    @test d_callable == d_direct
    @test typeof(d_callable) === Dict{String, Int64}

    d_empty = dctor()
    @test isempty(d_empty)
    @test typeof(d_empty) === Dict{String, Int64}

    d_vec = dctor(["x" => 10, "y" => 20])
    @test d_vec == Dict{String, Int64}(["x" => 10, "y" => 20])
    @test typeof(d_vec) === Dict{String, Int64}
end

@testset "Rational{T} direct vs callable + sibling parameterization guards" begin
    rctor = Rational{Int64}
    @test rctor(1, 2) === Rational{Int64}(1, 2)
    @test rctor(3) === Rational{Int64}(3)
    @test typeof(rctor(1, 2)) === Rational{Int64}

    rmapped = map(Rational{Int64}, [1, 2, 3])
    @test rmapped == [1 // 1, 2 // 1, 3 // 1]
    @test typeof(rmapped[1]) === Rational{Int64}

    # Sibling instantiations each resolve to their OWN parameterization: if
    # runtime candidate augmentation ever treated a concrete instantiation
    # (e.g. a Rational{Bool} method row) as a generic fallback for a
    # differently-parameterized callable, these exact-type assertions break.
    rb = Rational{Bool}
    @test rb(true) === Rational{Bool}(true)
    @test typeof(rb(true)) === Rational{Bool}

    r32 = Rational{Int32}
    @test typeof(r32(1, 2)) === Rational{Int32}

    rbig = Rational{BigInt}
    @test rbig(1, 2) == Rational{BigInt}(1, 2)
    @test typeof(rbig(1, 2)) === Rational{BigInt}

    # Negative guards: the Bool sibling must not absorb a value it cannot
    # represent, and a dispatch miss must surface as a catchable error, not
    # a silent reroute through some other instantiation's method.
    @test_throws InexactError rb(2)
    @test_throws MethodError rctor("nope")
end

@testset "Complex{T} direct vs callable + default ctor fallback after miss" begin
    # The `Complex{Int64}(re, im)` class from Issue #10502: generic
    # `Complex{T}` conversion candidates exist, and finding them must NOT
    # suppress the default DataType constructor fallback.
    cctor = Complex{Int64}
    @test cctor(3, 4) === Complex{Int64}(3, 4)
    @test cctor(3) === Complex{Int64}(3, 0)
    @test typeof(cctor(3, 4)) === Complex{Int64}

    cmapped = map(Complex{Int64}, [1, 2])
    @test cmapped == [1 + 0im, 2 + 0im]
    @test typeof(cmapped[1]) === Complex{Int64}

    cf = Complex{Float64}
    @test cf(1, 2) === ComplexF64(1.0, 2.0)
    @test typeof(cf(1, 2)) === Complex{Float64}

    # Dispatch miss stays a catchable MethodError (not a sibling reroute).
    @test_throws MethodError cctor("nope")
end

# A generic TypeVar constructor method IS a legitimate shared candidate for
# every instantiation — the positive direction of invariant 1.
struct CtorAuditGen10502{T}
    x::T
end

CtorAuditGen10502{T}(s::String) where {T} = CtorAuditGen10502{T}(T(length(s)))

@testset "generic TypeVar ctor method is shared across instantiations" begin
    @test CtorAuditGen10502{Int64}("abc").x === 3
    @test CtorAuditGen10502{Float64}("abcd").x === 4.0

    gi = CtorAuditGen10502{Int64}
    gf = CtorAuditGen10502{Float64}
    @test gi("abc").x === 3
    @test gf("hello").x === 5.0
    @test map(gf, ["xy"])[1].x === 2.0

    # Default constructor still reachable alongside the generic method.
    @test CtorAuditGen10502{Float64}(9.5).x === 9.5
    @test gf(8.5).x === 8.5
end

# A method defined ONLY on one concrete instantiation must stay invisible to
# sibling parameterizations — invariant 1's user-struct direction. The
# non-convertible-argument MethodError direction is still tracked by Issue
# #10593 (see header); the default-ctor-after-miss direction (Issue #10592)
# is fixed and asserted directly below (own hits + own-instantiation default
# ctor fallback + sibling isolation).
struct CtorAuditBox10502{T}
    x::T
end

CtorAuditBox10502{Int64}(s::String) = CtorAuditBox10502(length(s))

@testset "concrete instantiation method: own hits + sibling isolation" begin
    @test CtorAuditBox10502{Int64}("abc").x == 3
    ctor_i = CtorAuditBox10502{Int64}
    @test ctor_i("abcd").x == 4

    # Default constructor fallback on the SAME instantiation that owns the
    # String outer: an Int64 argument does not match the String method, so
    # the default field constructor must still fire (Issue #10592, fixed by
    # PR #11476) — direct and bound-callable.
    @test CtorAuditBox10502{Int64}(7).x === 7
    @test typeof(CtorAuditBox10502{Int64}(7)) === CtorAuditBox10502{Int64}
    @test ctor_i(9).x === 9

    # Sibling instantiation keeps its untouched default constructor, direct
    # and callable — the Int64-only String method must not leak into it.
    # (These Float64 arguments can never select the String method; the
    # scenario below covers the argument shape that COULD.)
    @test CtorAuditBox10502{Float64}(1.5).x === 1.5
    ctor_f = CtorAuditBox10502{Float64}
    @test ctor_f(2.5).x === 2.5
    @test map(ctor_f, [3.5])[1].x === 3.5
end

# Codex review follow-up (PR #10601): the String-method scenario above cannot
# catch a candidate-augmentation leak by itself — a Float64 argument never
# matches the leaked `::String` method, so a regression that wrongly added the
# Int64-only row to the Float64 callable's candidate set would still pass.
# This scenario uses an argument shape (Bool) that the sibling-only method
# DOES accept while the sibling's default constructor also legitimately
# handles it (convert(Float64, true) === 1.0). If the Int64-only Bool method
# ever leaked into the `CtorAuditSib10502{Float64}` callable, these calls
# would return `CtorAuditSib10502{Int64}(10)` and the exact value/type
# assertions below would fail — no MethodError direction needed (that
# direction stays blocked by Issue #10593, see header).
struct CtorAuditSib10502{T}
    x::T
end

CtorAuditSib10502{Int64}(b::Bool) = CtorAuditSib10502(b ? 10 : 20)

@testset "sibling-only method argument shape: a leak would select it" begin
    # Own hits: direct + bound callable.
    @test CtorAuditSib10502{Int64}(true).x === 10
    sib_i = CtorAuditSib10502{Int64}
    @test sib_i(false).x === 20
    @test typeof(sib_i(true)) === CtorAuditSib10502{Int64}

    # Default constructor fallback on the SAME instantiation that owns the
    # Bool method: an Int64 argument does not match the Bool method, so the
    # default field constructor must still fire (Issue #10592, fixed by PR
    # #11476) — direct and bound-callable.
    @test CtorAuditSib10502{Int64}(7).x === 7
    @test typeof(CtorAuditSib10502{Int64}(7)) === CtorAuditSib10502{Int64}
    @test sib_i(9).x === 9

    # Sibling with the SAME argument shape: the default constructor must win,
    # direct, bound-callable, and map.
    @test CtorAuditSib10502{Float64}(true).x === 1.0
    sib_f = CtorAuditSib10502{Float64}
    @test sib_f(true).x === 1.0
    @test sib_f(false).x === 0.0
    @test typeof(sib_f(true)) === CtorAuditSib10502{Float64}

    mapped = map(sib_f, [true, false])
    @test mapped[1].x === 1.0
    @test mapped[2].x === 0.0
    @test typeof(mapped[1]) === CtorAuditSib10502{Float64}
end

true
