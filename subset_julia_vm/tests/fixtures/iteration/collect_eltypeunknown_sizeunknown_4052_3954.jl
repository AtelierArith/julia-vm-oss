using Test

struct EltypeUnknownSizeUnknownIter4052
    data
end

function Base.iterate(itr::EltypeUnknownSizeUnknownIter4052)
    return iterate(itr.data)
end

function Base.iterate(itr::EltypeUnknownSizeUnknownIter4052, state)
    return iterate(itr.data, state)
end

Base.IteratorEltype(::EltypeUnknownSizeUnknownIter4052) = Base.EltypeUnknown()
Base.IteratorSize(::EltypeUnknownSizeUnknownIter4052) = Base.SizeUnknown()

@testset "collect EltypeUnknown SizeUnknown grows through similar (Issues #4052/#3954)" begin
    int_values = collect(EltypeUnknownSizeUnknownIter4052((1, 2, 3)))
    @test typeof(int_values) === Vector{Int64}
    @test eltype(int_values) === Int64
    @test int_values == [1, 2, 3]

    empty_values = collect(EltypeUnknownSizeUnknownIter4052(()))
    @test typeof(empty_values) === Vector{Any}
    @test eltype(empty_values) === Any
    @test length(empty_values) == 0

    real_values = collect(EltypeUnknownSizeUnknownIter4052((1, 2.0)))
    @test typeof(real_values) === Vector{Real}
    @test eltype(real_values) === Real
    @test real_values[1] == 1
    @test real_values[2] == 2.0

    similar_values = Base.collect_similar([0.0], EltypeUnknownSizeUnknownIter4052((1, 2.0)))
    @test typeof(similar_values) === Vector{Real}
    @test eltype(similar_values) === Real
    @test similar_values[1] == 1
    @test similar_values[2] == 2.0
end

true
