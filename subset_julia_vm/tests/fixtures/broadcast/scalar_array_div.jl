# Scalar / Array and Array / Scalar operations (broadcast division)
# Tests scalar-array division for various numeric types.
# Related: Issue #1601

using Test

@testset "Array ./ Float64 scalar" begin
    a = [2.0, 4.0, 6.0]
    result = a ./ 2.0
    @test result == [1.0, 2.0, 3.0]
end

@testset "Float64 scalar ./ Array" begin
    a = [1.0, 2.0, 4.0]
    result = 8.0 ./ a
    @test result == [8.0, 4.0, 2.0]
end

@testset "Int64 array ./ Int64 scalar" begin
    a = [4, 10, 18]
    result = a ./ 2
    @test result == [2.0, 5.0, 9.0]
end

@testset "Division results are Float64" begin
    a = [1, 2, 3]
    result = a ./ 1
    @test typeof(result[1]) == Float64
end

true
