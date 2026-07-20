using Test

# A duplicate keyword name replaces the value at its EXISTING (first
# occurrence) slot rather than moving it to the end or being lost to a
# HashMap's unspecified iteration order (Issue #11383).

f(; kw...) = kw

@testset "kwargs duplicate key overwrite in place: single splat source" begin
    kw = f(; ((:b, 1), (:a, 2), (:b, 3))...)
    @test collect(keys(kw)) == [:b, :a]
    @test collect(values(kw)) == [3, 2]
    @test kw[:b] == 3
    @test kw[:a] == 2
end

@testset "kwargs duplicate key overwrite in place: literal then splat" begin
    kw = f(; b = 1, ((:a, 2), (:b, 3))...)
    @test collect(keys(kw)) == [:b, :a]
    @test collect(values(kw)) == [3, 2]
end

true
