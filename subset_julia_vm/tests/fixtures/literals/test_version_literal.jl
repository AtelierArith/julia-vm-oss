# Test version string literal v"..."

using Test

@testset "Version string literals" begin
    # Basic version literal
    v1 = v"1.2.3"
    @test v1.major == 1
    @test v1.minor == 2
    @test v1.patch == 3

    # Two-part version (patch defaults to 0)
    v2 = v"2.5"
    @test v2.major == 2
    @test v2.minor == 5
    @test v2.patch == 0

    # Single-part version (minor and patch default to 0)
    v3 = v"10"
    @test v3.major == 10
    @test v3.minor == 0
    @test v3.patch == 0

    # Compare with VERSION constant
    @test VERSION.major >= 0

    # Check struct type
    @test typeof(v1) == VersionNumber
end

true
