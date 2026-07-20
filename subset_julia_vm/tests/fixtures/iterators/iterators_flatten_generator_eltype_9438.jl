# Issue #9438 (iterator-trait algebra of #10050 / #10463): `collect` of a
# flatten whose INNER iterables are `Base.Generator`s must recover the concrete
# element type via value-based `grow_to!` widening, not widen to `Vector{Any}`.
#
# Upstream `IteratorEltype(::Type{Flatten{I}}) = _flatteneltype(I, IteratorEltype(I))`
# with `_flatteneltype(I, ::HasEltype) = IteratorEltype(eltype(I))` and
# `_flatteneltype(I, et) = EltypeUnknown()`: a flatten's element type is known
# only when BOTH the outer iterator AND its element type (the inner iterables)
# have a known eltype. `IteratorEltype(::Generator) == EltypeUnknown()`, so a
# flatten over generators is `EltypeUnknown()` and `collect` grows/promotes from
# the actual elements. sjulia previously hardcoded `IteratorEltype(::Flatten) =
# HasEltype()`, joining `eltype(::Generator)==Any` into `Vector{Any}`.

using Test

@testset "flatten-over-generators collect eltype (Issue #9438)" begin
    # The issue's MWE: generator-of-generators, both forms.
    @test collect(x + y for x in 1:2 for y in 1:3) == [2, 3, 4, 3, 4, 5]
    @test collect(x + y for x in 1:2 for y in 1:3) isa Vector{Int64}

    g = Base.Generator(x -> Base.Generator(y -> x + y, 1:2), 1:2)
    @test collect(Iterators.flatten(g)) == [2, 3, 3, 4]
    @test collect(Iterators.flatten(g)) isa Vector{Int64}

    # Array-of-generators: outer HasEltype but element (Generator) EltypeUnknown
    # ⇒ still recovers Int64 by widening.
    ag = collect(Iterators.flatten([(x for x in 1:2), (x for x in 3:4)]))
    @test ag == [1, 2, 3, 4]
    @test ag isa Vector{Int64}

    # Mixed-type generator elements promote (Int + Float ⇒ Float64), proving the
    # widening is a real typejoin/promote, not a hardcoded Int fast path.
    h = Base.Generator(x -> Base.Generator(y -> x + y, 1:2), [1, 2.0])
    @test collect(Iterators.flatten(h)) == [2.0, 3.0, 3.0, 4.0]
    @test collect(Iterators.flatten(h)) isa Vector{Float64}

    # Regression guards: concrete array/range inners keep their known eltype
    # (must NOT be routed through EltypeUnknown widening spuriously).
    @test collect(Iterators.flatten([[1, 2], [3, 4]])) isa Vector{Int64}
    @test collect(Iterators.flatten((1:2, 8:9))) isa Vector{Int64}
    @test collect(Iterators.flatten(["ab", "cd"])) isa Vector{Char}

    # Tuple/NamedTuple outer: the per-field trait check must treat every field's
    # IteratorEltype. Generator fields ⇒ widening (Vector{Int64}); array fields
    # ⇒ per-field promote_typejoin, recovered even for EMPTY typed inners.
    tg = collect(Iterators.flatten(((x for x in 1:2), (x for x in 3:4))))
    @test tg == [1, 2, 3, 4]
    @test tg isa Vector{Int64}
    @test collect(Iterators.flatten((Int8[], Int16[]))) isa Vector{Signed}
    @test collect(Iterators.flatten((a = Int8[], b = Int16[]))) isa Vector{Signed}
    @test collect(Iterators.flatten((a = Int8[1, 2], b = Int16[3, 4]))) isa Vector{Signed}

    # Dependent inner range still flattens correctly to a typed Vector.
    @test collect(x + y for x in 1:3 for y in 1:x) == [2, 3, 4, 4, 5, 6]
    @test collect(x + y for x in 1:3 for y in 1:x) isa Vector{Int64}
end

true
