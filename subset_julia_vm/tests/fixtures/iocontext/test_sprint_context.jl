# Test sprint with IOContext support
# Issue #334: sprint should respect IOContext properties like :compact

using Test
import Base: show

struct SprintContextProbe10008 end

function show(io::IOContext, x::SprintContextProbe10008)
    print(io, get(io, :compact, false) ? "compact" : "wide")
end

function show(io::IO, x::SprintContextProbe10008)
    print(io, "wide")
end

@testset "sprint with context" begin
    # sprint with :compact => true should reduce decimal places for floats
    # The Pure Julia sprint implementation is used when context kwarg is provided
    s1 = sprint(show, 66.66666; context=:compact => true)
    @test length(s1) < 10  # Should be shorter than full precision

    # Test compact with different values
    s2 = sprint(show, 123.456789; context=:compact => true)
    @test startswith(s2, "123.")

    # Test with zero
    s3 = sprint(show, 0.0; context=:compact => true)
    @test s3 == "0.0"

    # Test NaN and Inf
    s4 = sprint(show, NaN; context=:compact => true)
    @test s4 == "NaN"

    s5 = sprint(show, Inf; context=:compact => true)
    @test s5 == "Inf"

    s6 = sprint(show, -Inf; context=:compact => true)
    @test s6 == "-Inf"

    @test sprint(show, SprintContextProbe10008()) == "wide"
    @test sprint(show, SprintContextProbe10008(); context=:compact => true) == "compact"
    @test sprint(show, SprintContextProbe10008(); context=(:compact => true,)) == "compact"
    @test sprint(print, SprintContextProbe10008(); context=:compact => true) == "compact"

    # Issue #10065: `print` uses the same keyword context route as `show`.
    @test sprint(print, "abc"; context=:compact => true) == "abc"
    @test sprint(print, "x=", 10; context=:compact => true) == "x=10"
end

true
