# reshape: change array dimensions
# Expected: 6.0 (total elements after reshape)

using Test

@testset "reshape changes array dimensions" begin

    arr = [1, 2, 3, 4, 5, 6]
    mat = reshape(arr, 2, 3)
    @test (Float64(length(mat))) == 6.0
end

true  # Test passed
