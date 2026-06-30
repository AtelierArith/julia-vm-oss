# Test: parametric constructor return-type resolution (Issue #5922 wave 5)
# Pins the constructor-inference families migrated to the tfuncs adapter's
# StructInstantiation seam: parametric struct ctor (inferred type args +
# on-demand instantiation), `{`-instantiated ctor names (Val{N}(), T{P}()),
# and the parametric Rational ctor. The Dict non-builtin-pattern fallback is
# pinned by unit tests only: non-builtin Dict(...) argument shapes are not
# yet compilable end-to-end (Issue #6531).
using Test

struct CtorPoint5922{T}
    x::T
    y::T
end

@testset "parametric ctor resolution (Issue #5922)" begin
    # Parametric struct ctor: type args inferred from arguments.
    p = CtorPoint5922(1, 2)
    @test p.x + p.y == 3
    @test p isa CtorPoint5922{Int64}

    pf = CtorPoint5922(1.5, 2.5)
    @test pf isa CtorPoint5922{Float64}
    @test pf.y == 2.5

    # Array of parametric struct instances keeps the concrete element type.
    arr = [CtorPoint5922(1.0, 2.0), CtorPoint5922(3.0, 4.0)]
    @test arr[2].y == 4.0
    @test eltype(arr) == CtorPoint5922{Float64}

    # `{`-instantiated ctor names.
    v = Val{2}()
    @test v isa Val{2}
    pe = CtorPoint5922{Int64}(7, 8)
    @test pe isa CtorPoint5922{Int64}
    @test pe.x == 7

    # Parametric Rational ctor.
    r = Rational(1, 2)
    @test r + r == 1//1
    @test r isa Rational{Int64}
end

true
