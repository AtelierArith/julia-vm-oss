# vec: flatten 2D matrix to 1D (column-major order)
# 2x3 matrix -> 6 element vector, v[3] = m[1,2] = 3.0

using Test

@testset "vec: flatten 2D matrix (column-major)" begin
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
