using Test

struct SizeUnknownEltypeIter4052
    data
end

function Base.iterate(itr::SizeUnknownEltypeIter4052)
    return iterate(itr.data)
end

function Base.iterate(itr::SizeUnknownEltypeIter4052, state)
    return iterate(itr.data, state)
end

Base.eltype(::SizeUnknownEltypeIter4052) = Int64
Base.IteratorEltype(::SizeUnknownEltypeIter4052) = Base.HasEltype()
Base.IteratorSize(::SizeUnknownEltypeIter4052) = Base.SizeUnknown()

@testset "collect SizeUnknown HasEltype uses container allocation (Issues #4052/#3954)" begin
    itr = SizeUnknownEltypeIter4052((2, 4))

    values = Base.collect_similar([0], itr)
    @test typeof(values) === Vector{Int64}
    @test eltype(values) === Int64
    @test values == [2, 4]

    push!(values, 6)
    @test values == [2, 4, 6]

    empty_values = Base.collect_similar([0], SizeUnknownEltypeIter4052(()))
    @test typeof(empty_values) === Vector{Int64}
    @test eltype(empty_values) === Int64
    @test length(empty_values) == 0
end

true
