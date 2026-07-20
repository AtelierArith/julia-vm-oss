using Test

@testset "reductions over non-indexable iterators (Issue #8816)" begin
    d = Dict(:x => 10, :y => 20)
    vals = values(d)

    @test sum(vals) == 30
    @test maximum(vals) == 20
    @test minimum(vals) == 10
    @test prod(vals) == 200
    @test count(x -> x > 5, vals) == 2

    numeric_keys = Dict(1 => 10, 2 => 20)
    ks = keys(numeric_keys)
    @test sum(ks) == 3
    @test maximum(ks) == 2
    @test minimum(ks) == 1
    @test prod(ks) == 2
    @test count(x -> x > 1, ks) == 1

    s = Set([1, 2, 3])
    @test sum(s) == 6
    @test prod(s) == 6
    @test maximum(s) == 3
    @test minimum(s) == 1
end

true
