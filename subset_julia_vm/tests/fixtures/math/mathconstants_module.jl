# Test MathConstants module access
# Tests accessing constants via Base.MathConstants.constant_name

using Test

@testset "MathConstants module access (Issue #761)" begin
    # Test module access to constants via Base.MathConstants
    @test (abs(Base.MathConstants.π - 3.141592653589793) < 1e-10)
    @test (abs(Base.MathConstants.pi - 3.141592653589793) < 1e-10)
    @test (abs(Base.MathConstants.ℯ - 2.718281828459045) < 1e-10)
    @test (abs(Base.MathConstants.e - 2.718281828459045) < 1e-10)
    @test (abs(Base.MathConstants.γ - 0.5772156649015329) < 1e-10)
    @test (abs(Base.MathConstants.eulergamma - 0.5772156649015329) < 1e-10)
    @test (abs(Base.MathConstants.φ - 1.618033988749895) < 1e-10)
    @test (abs(Base.MathConstants.golden - 1.618033988749895) < 1e-10)
    @test (abs(Base.MathConstants.catalan - 0.9159655941772190) < 1e-10)
end

true  # Test passed
