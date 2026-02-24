using Test

function sum5(a, b, c, d, e)
    return a + b + c + d + e
end

function collect_args(args...)
    return length(args)
end

@testset "splat with multiple arguments" begin
    a = [1, 2, 3]
    b = [10, 20]

    # Splat array into function call
    result = sum5(a..., b...)
    @test result == 36

    # Splat to count arguments
    @test collect_args(a...) == 3
    @test collect_args(b...) == 2
    @test collect_args(a..., b...) == 5
end

true
