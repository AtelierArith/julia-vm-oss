# Issue #4973: `ntuple` must be a first-class function value.
#
# Previously `ntuple` existed only as a Rust builtin HOF (BuiltinId::Ntuple),
# so referencing it as a value (`f = ntuple`, `Base.ntuple`) raised
# UndefVarError / "Base has no function named ntuple". It is now backed by a
# pure-Julia method in base/tuple.jl while the direct call shapes
# `ntuple(f, n)` / `ntuple(f, Val(N))` keep their constant-propagation fast path.

using Test

double_idx_4973(i) = 2i

function add_capture_4973(a)
    g = ntuple
    return g(i -> i + a, 3)
end

apply_4973(fn) = fn(identity, 3)

function indirect_negative_4973()
    f = ntuple
    try
        f(identity, -1)
        return false
    catch e
        return e isa ArgumentError
    end
end

@testset "ntuple first-class (Issue #4973)" begin
    # Direct call shapes still work (fast path preserved).
    @test ntuple(identity, 3) == (1, 2, 3)
    @test ntuple(double_idx_4973, 4) == (2, 4, 6, 8)
    @test ntuple(identity, 0) == ()

    # Val length fast path preserved.
    @test ntuple(identity, Val(3)) == (1, 2, 3)
    @test ntuple(double_idx_4973, Val(4)) == (2, 4, 6, 8)
    @test ntuple(identity, Val(0)) == ()

    # First-class value via local binding.
    f = ntuple
    @test f(identity, 3) == (1, 2, 3)
    @test f(double_idx_4973, 4) == (2, 4, 6, 8)
    @test f(identity, 0) == ()

    # Base-qualified value.
    g = Base.ntuple
    @test g(identity, 3) == (1, 2, 3)

    # Captured closure passed through a first-class ntuple reference.
    @test add_capture_4973(10) == (11, 12, 13)

    # Passing ntuple as an argument to another function.
    @test apply_4973(ntuple) == (1, 2, 3)

    # First-class invocation validates its length argument like upstream.
    @test indirect_negative_4973()
end

true
