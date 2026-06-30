# Issue #6806 (PR B): `setindex!` (`xs[i] = v`) called through an untyped binding
# must reach a user-defined `setindex!(::Vector{Int64}, ::Int, ::Int)` override
# instead of the native write fast path. The `IndexStore` fast path is gated on
# the `disable_array_setindex_specialization` compile-context flag (mirroring the
# #6657 getindex flag), set when the program defines a user `setindex!` array
# override, so the override is reached via dispatch.
#
# The override here is a no-op (returns the array unchanged) so the gate's effect
# is directly observable: if dispatch reaches the override the array is NOT
# mutated, whereas a non-overridden element type still writes normally. (A
# value-recording override is avoided because nested `Ref` `setindex!` inside a
# user `setindex!` override is a separate pre-existing limitation.) Verified
# against upstream Julia 1.12.
using Test

import Base: setindex!
setindex!(xs::Vector{Int64}, v::Int, i::Int) = xs  # no-op override

store!(a, i, v) = (a[i] = v; a)

@testset "setindex!(::Any) dispatch to user array override (#6806)" begin
    # Int array with the override: the write is routed to the no-op override
    # (fast path refused by the gate), so the array is unchanged.
    xs = [10, 20, 30]
    store!(xs, 2, 99)
    @test xs == [10, 20, 30]

    # explicit call form also reaches the override (no mutation)
    setindex!(xs, 7, 1)
    @test xs == [10, 20, 30]

    # a non-overridden element type still writes normally (no Float override)
    ys = [1.0, 2.0, 3.0]
    store!(ys, 1, 5.0)
    @test ys == [5.0, 2.0, 3.0]
end

[10, 20, 30] == [10, 20, 30]
