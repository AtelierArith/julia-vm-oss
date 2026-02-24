# fill!: fill array with value (mutating)
# zeros(5) filled with 3.0, check a[3]

using Test

@testset "fill!: mutating fill with value" begin
    a = zeros(5)
    fill!(a, 3.0)
    @test (a[3]) == 3.0
end

true  # Test passed
