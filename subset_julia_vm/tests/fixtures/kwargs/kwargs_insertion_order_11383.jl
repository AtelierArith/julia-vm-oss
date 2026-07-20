using Test

# Keyword-argument materialization must preserve insertion order: each key
# keeps the position of its FIRST occurrence (Issue #11383). Runtime kwargs
# used to accumulate in a `HashMap`, so `kwargs...` observed hash order
# instead.

f(; kw...) = kw

@testset "kwargs insertion order: literal keyword arguments" begin
    kw = f(; z = 1, a = 2)
    @test collect(keys(kw)) == [:z, :a]
    @test collect(values(kw)) == [1, 2]
    @test kw[:z] == 1
    @test kw[:a] == 2
end

@testset "kwargs insertion order: tuple-of-pairs splat" begin
    kw = f(; ((:b, 1), (:a, 2))...)
    @test collect(keys(kw)) == [:b, :a]
    @test collect(values(kw)) == [1, 2]
end

true
