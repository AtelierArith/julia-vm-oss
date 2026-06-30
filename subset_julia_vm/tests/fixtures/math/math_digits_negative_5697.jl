using Test

# Issue #5697: digits of a negative integer are SIGNED (each digit negative),
# not the digits of abs(n).

@testset "digits of a negative integer is signed (Issue #5697)" begin
    @test digits(-100) == [0, 0, -1]
    @test digits(-5, base=2) == [-1, 0, -1]
    @test digits(-255, base=16) == [-15, -15]
    @test digits(-1) == [-1]
    @test digits(-100, pad=5) == [0, 0, -1, 0, 0]

    # Positive and zero are unchanged.
    @test digits(100) == [0, 0, 1]
    @test digits(123) == [3, 2, 1]
    @test digits(0) == [0]
    @test digits(255, base=16) == [15, 15]
end

true
