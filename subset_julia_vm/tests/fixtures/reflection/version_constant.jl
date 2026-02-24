# Test VERSION constant and VersionNumber type
# VERSION is now a VersionNumber struct with major, minor, patch fields

using Test

@testset "VERSION constant: global version string (Issue #340)" begin

    result = true

    # Test that VERSION is a VersionNumber
    @assert typeof(VERSION) == VersionNumber

    # Test that we can access version fields (check they are integers >= 0)
    @assert VERSION.major >= 0
    @assert VERSION.minor >= 0
    @assert VERSION.patch >= 0

    # Test VersionNumber constructors
    v1 = VersionNumber(1, 2, 3)
    @assert v1.major == 1
    @assert v1.minor == 2
    @assert v1.patch == 3

    # Test 2-arg constructor (patch defaults to 0)
    v2 = VersionNumber(2, 5)
    @assert v2.major == 2
    @assert v2.minor == 5
    @assert v2.patch == 0

    # Test 1-arg constructor (minor and patch default to 0)
    v3 = VersionNumber(3)
    @assert v3.major == 3
    @assert v3.minor == 0
    @assert v3.patch == 0

    @test (result)
end

true  # Test passed
