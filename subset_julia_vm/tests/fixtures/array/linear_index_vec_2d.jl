# vec on 2D array using linear indexing
# m is 2x3, vec(m) should give [1,2,3,4,5,6] in column-major order

using Test

@testset "vec on 2D array using linear indexing" begin
    m = zeros(2, 3)
    m[1, 1] = 1.0
    m[2, 1] = 2.0
    m[1, 2] = 3.0
    m[2, 2] = 4.0
    m[1, 3] = 5.0
    m[2, 3] = 6.0
    v = vec(m)
    @test (v[3]) == 3.0
end

true  # Test passed
