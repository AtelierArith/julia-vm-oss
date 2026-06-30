using Test

@testset "collect Take/Drop trait paths (Issue #4052)" begin
    empty_take = Iterators.take(Int64[], 3)
    @test typeof(Base.IteratorEltype(empty_take)) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorSize(empty_take)) === typeof(Base.HasLength())
    empty_take_values = collect(empty_take)
    @test typeof(empty_take_values) === Vector{Int64}
    @test length(empty_take_values) == 0

    long_take_values = collect(Iterators.take([1, 2], 5))
    @test typeof(long_take_values) === Vector{Int64}
    @test long_take_values == [1, 2]

    float_take_values = collect(Iterators.take(Float64[], 2))
    @test typeof(float_take_values) === Vector{Float64}
    @test length(float_take_values) == 0

    empty_drop = Iterators.drop(Int64[], 1)
    @test typeof(Base.IteratorEltype(empty_drop)) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorSize(empty_drop)) === typeof(Base.HasLength())
    empty_drop_values = collect(empty_drop)
    @test typeof(empty_drop_values) === Vector{Int64}
    @test length(empty_drop_values) == 0

    drop_all_values = collect(Iterators.drop([1, 2], 5))
    @test typeof(drop_all_values) === Vector{Int64}
    @test length(drop_all_values) == 0
end

true
