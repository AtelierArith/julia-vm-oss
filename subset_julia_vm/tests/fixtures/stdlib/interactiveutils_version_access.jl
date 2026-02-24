# Test VERSION constant access from within modules
# Verifies that VERSION is a proper VersionNumber struct, not a String (Issue #1282)

using Test

@testset "VERSION constant" begin
    # VERSION should be a VersionNumber struct
    @test typeof(VERSION) == VersionNumber

    # VERSION fields should be accessible
    v = VERSION
    @test v.major >= 0
    @test v.minor >= 0
    @test v.patch >= 0

    # VERSION string construction should work
    version_str = string(v.major, ".", v.minor, ".", v.patch)
    @test typeof(version_str) == String
end

true
