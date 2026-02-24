# Bare operators as function arguments (Issue #1985)
# Tests that operators (+, -, *, /) can be passed directly to higher-order functions

using Test

@testset "reduce with bare operators" begin
    @test reduce(+, [1, 2, 3, 4]) == 10
    @test reduce(*, [1, 2, 3, 4]) == 24
    @test reduce(-, [1, 2, 3]) == -4
end

@testset "accumulate with bare operators" begin
    @test accumulate(+, [1, 2, 3, 4]) == [1, 3, 6, 10]
    @test accumulate(*, [1, 2, 3, 4]) == [1, 2, 6, 24]
end

@testset "foldl/foldr with bare operators" begin
    @test foldl(+, [1, 2, 3, 4]) == 10
    @test foldl(-, [1, 2, 3]) == -4
    @test foldr(-, [1, 2, 3]) == 2
end

@testset "reduce with init and bare operators" begin
    @test reduce(+, [1, 2, 3], 10) == 16
    @test reduce(*, [2, 3, 4], 1) == 24
end

true
