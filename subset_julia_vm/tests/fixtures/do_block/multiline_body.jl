# Do block with multiline body

using Test

@testset "Do block with multiline body" begin
    arr = [1.0, 2.0, 3.0]
    result = map(arr) do x
        y = x * 2.0
        z = y + 1.0
        z
    end
    @test (result[1]) == 3.0
end

true  # Test passed
