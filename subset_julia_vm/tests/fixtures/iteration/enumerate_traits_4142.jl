using Test

@testset "enumerate traits follow upstream Julia (Issue #4142)" begin
    e = enumerate([10, 20, 30])
    @test IteratorSize(e) isa HasShape{1}
    @test IteratorEltype(e) isa HasEltype
    @test eltype(e) == Tuple{Int64, Int64}
    @test size(e) == (3,)
    @test last(e) == (3, 30)
end

@testset "collect(enumerate(...)) preserves tuple eltype (Issue #4142)" begin
    values = collect(enumerate([10, 20, 30]))
    @test typeof(values) === Vector{Tuple{Int64, Int64}}
    @test values == [(1, 10), (2, 20), (3, 30)]
end

true
