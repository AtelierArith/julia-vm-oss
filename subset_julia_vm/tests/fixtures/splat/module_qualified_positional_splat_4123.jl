using Test

module QualifiedSplat4123
    export fixed3, vararg_sum, forward_fixed

    fixed3(a, b, c) = a * 100 + b * 10 + c
    vararg_sum(xs...) = sum(xs)
    forward_fixed(xs...) = fixed3(xs...)
end

@testset "module-qualified positional splat calls (Issue #4123)" begin
    @test QualifiedSplat4123.fixed3((1, 2, 3)...) == 123
    @test QualifiedSplat4123.fixed3(1, (2, 3)...) == 123
    @test QualifiedSplat4123.vararg_sum([1, 2, 3, 4]...) == 10
    @test QualifiedSplat4123.forward_fixed((1, 2, 3)...) == 123

    @test Base.max((1, 5, 3)...) == 5
    @test Base.length(([1, 2, 3],)...) == 3
    @test Base.string("a", ("b", "c")...) == "abc"
    @test Base.:+((1, 2, 3)...) == 6
    @test Base.:*((2, 3, 4)...) == 24
end

true
