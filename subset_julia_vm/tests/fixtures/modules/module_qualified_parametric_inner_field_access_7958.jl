# Issue #7958: named field access `w.x` on an instance produced by a
# MODULE-QUALIFIED PARAMETRIC INNER constructor (`Mod.Wrapped(41)` ->
# `new{T}(...)`) previously failed with "type Wrapped{Int64} has no field x",
# even though `getfield(w, 1)` (positional) returned the correct value.
#
# Root cause: such an instance carries the instantiation name `Wrapped{Int64}`
# as its struct name and a fallback `type_id` of 0 because the instantiation was
# never registered in the runtime `struct_defs`. The dynamic `GetFieldByName`
# handler could not map the field name to an index. It now falls back to the
# compile-context `parametric_structs` schema (keyed by the base name `Wrapped`,
# which carries the declared field order) to resolve the index.
#
# The bug needed BOTH module qualification AND a parametric inner constructor:
# non-qualified parametric inner constructors and module-qualified parametric
# structs without an inner constructor already worked (static field access).
using Test

module QPIF7958
export Wrapped
struct Wrapped{T}
    x::T
    Wrapped(x::T) where T = new{T}(x + one(x))
end
end

module QPIF7958Multi
struct Pair2{A,B}
    a::A
    b::B
    Pair2(a::A, b::B) where {A,B} = new{A,B}(a, b)
end
end

module QPIF7958Two
struct TwoFields{T}
    first::T
    second::T
    TwoFields(a::T, b::T) where T = new{T}(a, b)
end
end

@testset "Issue #7958: module-qualified parametric inner ctor field access" begin
    # The exact MWE from the issue.
    w = QPIF7958.Wrapped(41)
    @test w.x == 42
    @test getfield(w, 1) == 42          # positional access already worked
    @test w.x == getfield(w, 1)
    @test typeof(w) == QPIF7958.Wrapped{Int64}
    @test nameof(typeof(w)) === :Wrapped
    @test fieldnames(typeof(w)) == (:x,)

    # Multi-parameter parametric inner constructor: each named field resolves.
    p = QPIF7958Multi.Pair2(1, 2.5)
    @test p.a == 1
    @test p.b == 2.5
    @test (p.a, p.b) == (getfield(p, 1), getfield(p, 2))

    # Two same-typed fields: named access maps to the right index, not just the
    # first slot.
    t = QPIF7958Two.TwoFields(10, 20)
    @test t.first == 10
    @test t.second == 20
    @test t.first != t.second
end

true
