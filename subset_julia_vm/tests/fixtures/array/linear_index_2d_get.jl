# Linear indexing on 2D array (get)
# Column-major: m[1]=m[1,1], m[2]=m[2,1], m[3]=m[1,2], m[4]=m[2,2]

using Test

@testset "Linear indexing on 2D array (get)" begin
    m = zeros(2, 3)
    m[1, 1] = 1.0
    m[2, 1] = 2.0
    m[1, 2] = 3.0
    m[2, 2] = 4.0
    m[1, 3] = 5.0
    m[2, 3] = 6.0
    @test (m[3]) == 3.0
end

true  # Test passed
