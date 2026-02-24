# Test VersionNumber type and VERSION constant

using Test

@testset "VersionNumber type" begin
    # Test basic construction
    v = VersionNumber(1, 2, 3)
    @test v.major == 1
    @test v.minor == 2
    @test v.patch == 3
    
    # Test constructor with defaults
    v2 = VersionNumber(2, 5)
    @test v2.major == 2
    @test v2.minor == 5
    @test v2.patch == 0
    
    v3 = VersionNumber(3)
    @test v3.major == 3
    @test v3.minor == 0
    @test v3.patch == 0
end

@testset "VERSION constant" begin
    # VERSION should be a VersionNumber
    @test typeof(VERSION) == VersionNumber
    
    # VERSION should have valid fields
    @test VERSION.major >= 0
    @test VERSION.minor >= 0
    @test VERSION.patch >= 0
end

true  # Test passed
