# Test Some type - optional value wrapper

using Test

@testset "Some - optional value wrapper" begin

    # Create Some with a value
    s = Some(42)
    @assert s.value == 42

    # something unwraps Some
    @assert something(s) == 42

    # something with multiple args returns first Some
    @assert something(nothing, Some(10), 20) == 10

    # something returns first non-nothing value
    @assert something(nothing, 5) == 5

    # Some(nothing) is distinguishable from nothing
    sn = Some(nothing)
    @assert sn.value === nothing
    @assert something(sn) === nothing

    @test (true)
end

true  # Test passed
