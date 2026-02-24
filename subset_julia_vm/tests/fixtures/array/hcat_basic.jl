# hcat: horizontal concatenation creates 2-column matrix
# hcat([1,2,3], [4,5,6]) -> 3x2 matrix, size(m, 2) = 2

using Test

@testset "hcat: horizontal concatenation column count (Int64)" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    m = hcat(x, y)
    @test (size(m, 2)) == 2
end

true  # Test passed
