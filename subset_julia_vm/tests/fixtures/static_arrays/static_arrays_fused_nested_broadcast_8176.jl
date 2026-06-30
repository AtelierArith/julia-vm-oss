# Issue #8176: an all-static *fused nested* broadcast (e.g. `abs.(v .+ w)` where
# both `v` and `w` are `SVector`) errored with an out-of-bounds index instead of
# returning the element-wise static result.
#
# sjulia fuses a `.`-chain into a tree of nested `Broadcasted` nodes. The outer
# `materialize(Broadcasted(abs, (Broadcasted(+, (v, w)),)))` calls
# `copy(instantiate(bc))`; `instantiate` computes axes via
# `_broadcastable_shape(inner)`, and the inner all-static `Broadcasted` has empty
# axes `()` (the generic shape system sees bare static arrays as 0-dimensional
# scalars). `_broadcastable_shape(::Broadcasted)` then indexed the empty `ax[1]`
# and crashed *before* `copy` ran — so the static-broadcast hook never claimed
# the broadcast. The `n == 0` case now returns the scalar shape `()`, letting
# `copy`'s static hook fire and produce the static result (Issue #8161's hook
# already classifies the whole nested operand tree).
#
# Mixed static/dynamic fused-nested broadcasts (Issue #8161) keep returning a
# dynamic `Array`; only the all-static case is a static `SVector`/`SMatrix`.
using StaticArrays

a = @SVector [1.0, 2.0, 3.0]
b = @SVector [10.0, 20.0, 30.0]
d = [100.0, 200.0, 300.0]      # dynamic, for the mixed regression checks

# all-static fused nested → STATIC (SVector) result
r1 = abs.(a .+ b)              # abs over a fused static broadcast
r2 = 2.0 .* (a .+ b)          # scalar ⊙ fused static broadcast
r3 = (a .+ b) .* a            # two fused static broadcasts
r4 = abs.(2.0 .* (a .+ b))    # 3-level fused
r5 = a .+ b .+ a              # left-assoc fused chain

# mixed static/dynamic fused nested → DYNAMIC result (Issue #8161, regression)
r6 = abs.(a .- d)             # abs over a mixed broadcast

ok =
    r1 == [11.0, 22.0, 33.0] && r1 isa SVector{3,Float64} &&
    r2 == [22.0, 44.0, 66.0] && r2 isa SVector{3,Float64} &&
    r3 == [11.0, 44.0, 99.0] && r3 isa SVector{3,Float64} &&
    r4 == [22.0, 44.0, 66.0] && r4 isa SVector{3,Float64} &&
    r5 == [12.0, 24.0, 36.0] && r5 isa SVector{3,Float64} &&
    # mixed stays a 3-element Float64 container (parity-clean: upstream returns a
    # Sized* StaticArray, the subset a plain Vector — both AbstractVector{Float64})
    r6 == [99.0, 198.0, 297.0] && r6 isa AbstractVector{Float64} && length(r6) == 3

println(ok)
ok
