# hcat: check element at row 2, column 2
# m[2,1] = 2.0, m[2,2] = 5.0

using Test

@testset "hcat: element value check" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    m = hcat(x, y)
    @test (m[2, 2]) == 5.0
end

true  # Test passed
