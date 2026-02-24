# Test BroadcastStyle type hierarchy (Issue #2531)
# Verifies that BroadcastStyle abstract type and concrete subtypes are defined correctly.
#
# Workaround: Uses non-parametric types (DefaultArrayStyle0, DefaultArrayStyle1, etc.)
# instead of parametric DefaultArrayStyle{N} due to Issue #2523.

using Test

@testset "BroadcastStyle type hierarchy" begin
    # Unknown is a subtype of BroadcastStyle
    @test Unknown() isa BroadcastStyle

    # DefaultArrayStyle0 (scalar) is a subtype of AbstractArrayStyle and BroadcastStyle
    @test DefaultArrayStyle0() isa AbstractArrayStyle
    @test DefaultArrayStyle0() isa BroadcastStyle

    # DefaultArrayStyle1 (vector) is a subtype of AbstractArrayStyle and BroadcastStyle
    @test DefaultArrayStyle1() isa AbstractArrayStyle
    @test DefaultArrayStyle1() isa BroadcastStyle

    # DefaultArrayStyle2 (matrix) is a subtype of AbstractArrayStyle and BroadcastStyle
    @test DefaultArrayStyle2() isa AbstractArrayStyle
    @test DefaultArrayStyle2() isa BroadcastStyle

    # ArrayConflict is a subtype of AbstractArrayStyle and BroadcastStyle
    @test ArrayConflict() isa AbstractArrayStyle
    @test ArrayConflict() isa BroadcastStyle
end

true
