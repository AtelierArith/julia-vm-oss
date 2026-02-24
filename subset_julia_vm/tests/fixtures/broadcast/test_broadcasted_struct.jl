# Test Broadcasted struct (Issue #2534)
# Verifies the Broadcasted lazy wrapper type exists and has correct fields.
# Field names use workaround names: bc_args (Issue #2534), axes_val
# to avoid compiler collision with Expr.args field access.

using Test

@testset "Broadcasted struct" begin
    # Verify field names (style, f, bc_args, axes_val — 4 fields)
    @test fieldnames(Broadcasted) == (:style, :f, :bc_args, :axes_val)

    # Verify fieldcount
    @test fieldcount(Broadcasted) == 4
end

true
