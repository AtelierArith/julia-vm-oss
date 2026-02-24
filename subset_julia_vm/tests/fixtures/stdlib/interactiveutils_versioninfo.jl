# Test InteractiveUtils.versioninfo() function
# Verifies that versioninfo() displays version information (Issue #1282)

using Test
using InteractiveUtils

@testset "InteractiveUtils versioninfo()" begin
    # versioninfo() should return nothing (it prints to stdout)
    @test versioninfo() === nothing
end

true
