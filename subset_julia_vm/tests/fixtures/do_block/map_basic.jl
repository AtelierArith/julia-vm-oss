# Basic do block with map - single parameter
# map(arr) do x; x*2; end is equivalent to map(x -> x*2, arr)

using Test

@testset "Basic do block with map - single parameter" begin
    arr = [1.0, 2.0, 3.0, 4.0]
    result = map(arr) do x
        x * 2.0
    end
    @test (sum(result)) == 20.0
end

true  # Test passed
