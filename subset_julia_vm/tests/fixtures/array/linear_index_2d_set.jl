# Linear indexing on 2D array (set)
# Column-major: m[4] = m[2,2]

using Test

@testset "Linear indexing on 2D array (set)" begin
    m = zeros(2, 3)
    m[4] = 99.0  # Sets m[2,2]
    @test (m[2, 2]) == 99.0
end

true  # Test passed
