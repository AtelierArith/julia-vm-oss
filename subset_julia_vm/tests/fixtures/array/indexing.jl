# Array indexing (1-based)

using Test

@testset "Array indexing" begin
    arr = [1, 2, 3, 4, 5]
    @test (arr[3]) == 3.0
end

true  # Test passed
