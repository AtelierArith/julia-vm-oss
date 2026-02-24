# Test sizehint! function (no-op performance hint)

using Test

@testset "sizehint!" begin
    # sizehint! returns the collection unchanged
    a = [1, 2, 3]
    @test sizehint!(a, 100) === a
    @test length(a) == 3

    # Works with empty arrays
    b = Int64[]
    @test sizehint!(b, 10) === b
    @test length(b) == 0

    # Can still push! after sizehint!
    sizehint!(b, 5)
    push!(b, 42)
    @test length(b) == 1
    @test b[1] == 42
end

true
