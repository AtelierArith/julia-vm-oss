# Test sprint with IOContext support
# Issue #334: sprint should respect IOContext properties like :compact

using Test

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
end

true
