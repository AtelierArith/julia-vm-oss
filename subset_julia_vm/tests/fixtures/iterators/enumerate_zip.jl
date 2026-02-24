# Test enumerate and zip iterator combinators (Issue #1885)

using Test

@testset "enumerate array" begin
    arr = [10.0, 20.0, 30.0]
    indices = Float64[]
    values = Float64[]
    for (i, v) in enumerate(arr)
        push!(indices, Float64(i))
        push!(values, v)
    end
    @test length(indices) == 3
    @test indices[1] == 1.0
    @test indices[2] == 2.0
    @test indices[3] == 3.0
    @test values[1] == 10.0
    @test values[2] == 20.0
    @test values[3] == 30.0
end

@testset "zip two arrays" begin
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    sums = Float64[]
    for (x, y) in zip(a, b)
        push!(sums, x + y)
    end
    @test length(sums) == 3
    @test sums[1] == 11.0
    @test sums[2] == 22.0
    @test sums[3] == 33.0
end

@testset "zip unequal lengths" begin
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0]
    count = 0
    for (x, y) in zip(a, b)
        count = count + 1
    end
    @test count == 2
end

true
