# Issue #4273: comparison-aware `limit_type_size` widening in lattice joins.
#
# A loop accumulator whose type nests one structural level deeper each
# iteration (e.g. `x = (x,)`, `a = [a]`) used to keep growing the inferred
# type until the absolute depth/length caps were hit — much later than
# upstream Julia, which bounds the growth as soon as the new type is more
# complex than the previous-iteration comparison type and widens it to its
# wrapper.
#
# These are runtime checks: the programs must still execute and produce the
# correct values. The point of the issue is that inference of these
# deeply-/recursively-nested accumulators stays *bounded* (no blow-up), while
# normal, non-growing accumulators are inferred exactly as before.

using Test

# ---------------------------------------------------------------------------
# (a) Tuple that nests one level deeper each iteration. Runtime values must be
# correct; inference must terminate (bounded) rather than chase unbounded
# tuple nesting.
# ---------------------------------------------------------------------------
function nest_tuple(n::Int)
    x = (0,)
    for _ in 1:n
        x = (x,)
    end
    return x
end

# Count how deeply a value is nested as `(inner,)` single-element tuples.
function nest_depth(x)
    d = 0
    while x isa Tuple && length(x) == 1 && x[1] isa Tuple
        d += 1
        x = x[1]
    end
    return d
end

# ---------------------------------------------------------------------------
# (b) Vector that wraps itself each iteration. Same recursive-growth shape via
# a different wrapper (Array). Runtime correctness is what we assert.
# ---------------------------------------------------------------------------
function nest_vector(n::Int)
    a = [1]
    for _ in 1:n
        a = [a]
    end
    return a
end

# ---------------------------------------------------------------------------
# (c) Control: a plain numeric accumulator must be unaffected by the new
# comparison-aware widening — it never grows structurally.
# ---------------------------------------------------------------------------
function plain_sum(n::Int)
    acc = 0
    for i in 1:n
        acc = acc + i
    end
    return acc
end

@testset "recursive type growth bounded (Issue #4273)" begin
    # Tuple nesting: values stay correct at small and larger depths.
    @test nest_tuple(0) === (0,)
    @test nest_tuple(1) === ((0,),)
    @test nest_depth(nest_tuple(3)) == 3
    @test nest_depth(nest_tuple(10)) == 10

    # Vector nesting executes and round-trips the innermost element.
    v = nest_vector(3)
    inner = v
    while inner isa Vector && length(inner) == 1 && inner[1] isa Vector
        inner = inner[1]
    end
    @test inner == [1]

    # Control accumulator unchanged.
    @test plain_sum(0) === 0
    @test plain_sum(10) === 55
end

true
