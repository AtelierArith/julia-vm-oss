# Test take and drop iterator combinators (Issue #1885)

using Test

@testset "take from array" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = Float64[]
    for x in take(arr, 3)
        push!(result, x)
    end
    @test length(result) == 3
    @test result[1] == 1.0
    @test result[2] == 2.0
    @test result[3] == 3.0
end

@testset "take more than length" begin
    arr = [1.0, 2.0]
    result = Float64[]
    for x in take(arr, 5)
        push!(result, x)
    end
    @test length(result) == 2
end

@testset "take zero" begin
    arr = [1.0, 2.0, 3.0]
    count = 0
    for x in take(arr, 0)
        count = count + 1
    end
    @test count == 0
end

@testset "drop from array" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = Float64[]
    for x in drop(arr, 2)
        push!(result, x)
    end
    @test length(result) == 3
    @test result[1] == 3.0
    @test result[2] == 4.0
    @test result[3] == 5.0
end

@testset "drop all" begin
    arr = [1.0, 2.0]
    count = 0
    for x in drop(arr, 5)
        count = count + 1
    end
    @test count == 0
end

@testset "drop zero" begin
    arr = [1.0, 2.0, 3.0]
    result = Float64[]
    for x in drop(arr, 0)
        push!(result, x)
    end
    @test length(result) == 3
    @test result[1] == 1.0
end

true
