using Test

# Issue #5695: digits(n; pad=N) pads the result to at least N digits with trailing
# zeros (the number's leading zeros, digits being least-significant first).

@testset "digits with pad keyword (Issue #5695)" begin
    @test digits(100, base=10, pad=5) == [0, 0, 1, 0, 0]
    @test digits(5, base=2, pad=8) == [1, 0, 1, 0, 0, 0, 0, 0]
    @test digits(255, base=16, pad=4) == [15, 15, 0, 0]
    @test digits(0, pad=3) == [0, 0, 0]
    @test digits(12345, pad=2) == [5, 4, 3, 2, 1]   # already >= pad
    @test digits(7, pad=1) == [7]

    # No pad: unchanged.
    @test digits(123) == [3, 2, 1]
    @test digits(0) == [0]
    @test digits(255, base=16) == [15, 15]
end

true
