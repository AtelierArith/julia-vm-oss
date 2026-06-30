using Test

struct GrowToPipelineIter4052
    data
end

function Base.iterate(itr::GrowToPipelineIter4052)
    return iterate(itr.data)
end

function Base.iterate(itr::GrowToPipelineIter4052, state)
    return iterate(itr.data, state)
end

Base.IteratorEltype(::GrowToPipelineIter4052) = Base.EltypeUnknown()
Base.IteratorSize(::GrowToPipelineIter4052) = Base.SizeUnknown()

@testset "collect SizeUnknown grow_to! pipeline (Issues #4052/#3954)" begin
    pushed = Base.push_widen(Int64[], 2.0)
    @test typeof(pushed) === Vector{Real}
    @test eltype(pushed) === Real
    @test pushed == [2.0]

    dest = Vector{Int64}(undef, 2)
    dest[1] = 1
    widened = Base.setindex_widen_up_to(dest, 2.0, 2)
    @test typeof(widened) === Vector{Real}
    @test eltype(widened) === Real
    @test widened[1] == 1
    @test widened[2] == 2.0

    grown = Base.grow_to!(Any[], GrowToPipelineIter4052((1, 2.0)))
    @test typeof(grown) === Vector{Real}
    @test eltype(grown) === Real
    @test grown[1] == 1
    @test grown[2] == 2.0

    values = collect(GrowToPipelineIter4052((1, 2.0, 3)))
    @test typeof(values) === Vector{Real}
    @test eltype(values) === Real
    @test values == [1, 2.0, 3]

    empty_values = collect(GrowToPipelineIter4052(()))
    @test typeof(empty_values) === Vector{Any}
    @test eltype(empty_values) === Any
    @test length(empty_values) == 0
end

true
