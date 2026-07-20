# Regression coverage for Issue #10077: a module-qualified function value
# (`h = Module.func`) must carry the SAME runtime type identity as the exact
# same function captured via a bare/imported name (`h = func`) — upstream
# Julia's generic function has ONE canonical name (`nameof`) regardless of the
# access path used to reach it. Before the fix, the qualified access path
# baked the module-qualified spelling (`"Pkg9992BId.transform10077"`) into the
# captured `FunctionValue`'s name, which made `isa Function` false (the
# embedded module-prefix dot corrupted the `typeof(...)` callable-singleton
# recognition) and made `typeof(...)` print the qualified spelling instead of
# the bare declared name.
#
# Covers, for the qualified-vs-bare access pair: `isa Function` (both true),
# `typeof(...)` identity (equal), and that dispatch/calling both values still
# works correctly (not just isa/typeof) — including as an HOF callback.

using Test

module Pkg9992BId
export transform10077
transform10077(x) = x + 1
end

using .Pkg9992BId

@testset "module-qualified vs bare function value identity (Issue #10077)" begin
    h_qualified = Pkg9992BId.transform10077
    h_bare = transform10077

    # Both access paths satisfy `isa Function`.
    @test h_qualified isa Function
    @test h_bare isa Function

    # Both access paths report the identical synthetic `typeof` type.
    @test typeof(h_qualified) == typeof(h_bare)

    # Calling both captured values still dispatches to the right method.
    @test h_qualified(10) == 11
    @test h_bare(10) == 11
    @test h_qualified(10) == h_bare(10)

    # Both work as ordinary HOF callbacks (calling correctness, not just
    # isa/typeof), confirming the fix did not disturb dispatch.
    @test map(h_qualified, [1, 2, 3]) == [2, 3, 4]
    @test map(h_bare, [1, 2, 3]) == [2, 3, 4]

    # A qualified reference used directly (without an intervening variable)
    # also resolves and calls correctly.
    @test Pkg9992BId.transform10077(41) == 42
end

true  # Test passed
