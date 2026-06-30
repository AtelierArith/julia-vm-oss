using Test

# Issue #4844: a `getfield(x, :value) !== nothing` (or `x.value !== nothing`)
# guard records a *path* refinement `x.value => Int64`. When a fresh alias
# `y = x` was then created, sjulia transferred that path refinement to
# `y.value`, so a read through the alias became narrower (`Int64`) than
# upstream's conservative `Union{Nothing,Int64}`.
#
# Upstream Julia does not propagate a field narrowing across a fresh alias
# binding (the MustAlias fact is tied to the guarded slot, not to `y`), so a
# read through the alias keeps the declared field union. These functions return
# an `Int` from the `else` branch so the (possibly narrowed) alias read is the
# only thing that can introduce / drop `Nothing` from the inferred result.
#
# The non-alias field re-read (`x.value` after the guard, no alias) is a
# separate, broader divergence that is intentionally out of scope here.

struct Box4844
    value::Union{Int64,Nothing}
end

# getfield guard then alias then read through alias
function alias_after_getfield_guard_4844(x::Box4844)
    if getfield(x, :value) !== nothing
        y = x
        return getfield(y, :value)
    end
    return 99
end

# dotted-field guard then alias then dotted read through alias
function alias_after_dot_guard_4844(x::Box4844)
    if x.value !== nothing
        y = x
        return y.value
    end
    return 99
end

@testset "alias does not inherit field path refinements" begin
    @test Base.infer_return_type(alias_after_getfield_guard_4844, Tuple{Box4844}) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(alias_after_getfield_guard_4844, Tuple{Box4844}) == Union{Nothing,Int64}
    @test Base.infer_return_type(alias_after_dot_guard_4844, Tuple{Box4844}) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(alias_after_dot_guard_4844, Tuple{Box4844}) == Union{Nothing,Int64}
    @test alias_after_getfield_guard_4844(Box4844(7)) == 7
    @test alias_after_getfield_guard_4844(Box4844(nothing)) == 99
    @test alias_after_dot_guard_4844(Box4844(7)) == 7
    @test alias_after_dot_guard_4844(Box4844(nothing)) == 99
end

true
