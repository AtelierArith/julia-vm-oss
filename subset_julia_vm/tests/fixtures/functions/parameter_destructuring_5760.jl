using Test

@testset "function and lambda parameter destructuring (#5760)" begin
    function functions_param_destructure_full((a, b))
        a + b
    end

    functions_param_destructure_short((a, b)) = a * b

    @test functions_param_destructure_full((3, 4)) == 7
    @test functions_param_destructure_short((3, 4)) == 12

    @test (((a, b),) -> a - b)((9, 2)) == 7
    @test map(((k, v),) -> v * 2, [1 => 10, 2 => 20]) == [20, 40]
end

true
