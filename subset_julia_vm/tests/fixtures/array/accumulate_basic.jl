# Test accumulate(op, A) for cumulative operations (Issue #1839)

using Test

myadd(a, b) = a + b
mymax(a, b) = a > b ? a : b
mymin(a, b) = a < b ? a : b

@testset "accumulate with addition" begin
    result = accumulate((a, b) -> a + b, [1, 2, 3, 4])
    @test length(result) == 4
    @test result[1] == 1
    @test result[2] == 3
    @test result[3] == 6
    @test result[4] == 10
end

@testset "accumulate with multiplication" begin
    result = accumulate((a, b) -> a * b, [1, 2, 3, 4])
    @test length(result) == 4
    @test result[1] == 1
    @test result[2] == 2
    @test result[3] == 6
    @test result[4] == 24
end

@testset "accumulate with max" begin
    result = accumulate(mymax, [3, 1, 4, 1, 5])
    @test length(result) == 5
    @test result[1] == 3
    @test result[2] == 3
    @test result[3] == 4
    @test result[4] == 4
    @test result[5] == 5
end

@testset "accumulate with min" begin
    result = accumulate(mymin, [3, 1, 4, 1, 5])
    @test length(result) == 5
    @test result[1] == 3
    @test result[2] == 1
    @test result[3] == 1
    @test result[4] == 1
    @test result[5] == 1
end

@testset "accumulate with init" begin
    result = accumulate((a, b) -> a + b, [1, 2, 3], 10)
    @test length(result) == 3
    @test result[1] == 11
    @test result[2] == 13
    @test result[3] == 16
end

@testset "accumulate single element" begin
    result = accumulate((a, b) -> a + b, [42])
    @test length(result) == 1
    @test result[1] == 42
end

@testset "accumulate Float64" begin
    result = accumulate((a, b) -> a + b, [1.0, 2.0, 3.0])
    @test length(result) == 3
    @test result[1] == 1.0
    @test result[2] == 3.0
    @test result[3] == 6.0
end

@testset "accumulate with named function" begin
    result = accumulate(myadd, [10, 20, 30])
    @test length(result) == 3
    @test result[1] == 10
    @test result[2] == 30
    @test result[3] == 60
end

true
