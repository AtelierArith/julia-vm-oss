# vec: flatten 1D array (should preserve length)

using Test

@testset "vec: flatten 1D preserves length (Int64)" begin
    a = [1.0, 2.0, 3.0, 4.0]
    b = vec(a)
    @test (length(b)) == 4
end

true  # Test passed
