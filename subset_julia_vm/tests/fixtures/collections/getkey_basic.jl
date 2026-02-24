# Test getkey function
# getkey(dict, key, default) returns the key if it exists, else default

using Test

@testset "getkey - return the key if it exists, else default" begin

    # Test with integer keys
    d = Dict(1 => 100, 2 => 200, 3 => 300)

    # Key exists - returns the key
    result = true
    result = result && getkey(d, 1, 0) == 1
    result = result && getkey(d, 2, 0) == 2
    result = result && getkey(d, 3, 0) == 3

    # Key does not exist - returns default
    result = result && getkey(d, 99, -1) == -1
    result = result && getkey(d, 100, -999) == -999

    # Test with different default values
    d2 = Dict(10 => "ten", 20 => "twenty")
    result = result && getkey(d2, 10, 0) == 10
    result = result && getkey(d2, 20, 0) == 20
    result = result && getkey(d2, 30, 0) == 0  # not found, return default 0

    @test (result)
end

true  # Test passed
