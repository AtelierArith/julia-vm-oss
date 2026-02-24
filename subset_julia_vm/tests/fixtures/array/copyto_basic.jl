# copyto!: copy elements from src to dest
# src = [1,2,3], dest[2] should be 2.0

using Test

@testset "copyto!: copy elements between arrays" begin
    src = [1.0, 2.0, 3.0]
    dest = zeros(3)
    copyto!(dest, src)
    @test (dest[2]) == 2.0
end

true  # Test passed
