# Test BroadcastStyle binary combination rules (Issue #2532)
# Tests broadcaststyle_combine function for resolving style pairs.
#
# Workaround: Uses non-parametric types due to Issue #2523.

using Test

@testset "BroadcastStyle combination rules" begin
    # Unknown + Unknown = Unknown
    @test broadcaststyle_combine(Unknown(), Unknown()) isa Unknown

    # Unknown loses to DefaultArrayStyle
    r1 = broadcaststyle_combine(DefaultArrayStyle1(), Unknown())
    @test r1 isa DefaultArrayStyle1

    r2 = broadcaststyle_combine(Unknown(), DefaultArrayStyle2())
    @test r2 isa DefaultArrayStyle2

    # Same type returns same type
    r3 = broadcaststyle_combine(DefaultArrayStyle1(), DefaultArrayStyle1())
    @test r3 isa DefaultArrayStyle1

    # Scalar promotes to vector
    r4 = broadcaststyle_combine(DefaultArrayStyle0(), DefaultArrayStyle1())
    @test r4 isa DefaultArrayStyle1

    # Scalar promotes to matrix
    r5 = broadcaststyle_combine(DefaultArrayStyle0(), DefaultArrayStyle2())
    @test r5 isa DefaultArrayStyle2

    # Vector promotes to matrix
    r6 = broadcaststyle_combine(DefaultArrayStyle1(), DefaultArrayStyle2())
    @test r6 isa DefaultArrayStyle2
end

true
