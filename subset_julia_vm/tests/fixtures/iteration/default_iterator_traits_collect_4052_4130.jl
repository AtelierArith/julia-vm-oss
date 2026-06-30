using Test

import Base: IteratorEltype, IteratorSize, collect, eltype, iterate, length

struct DefaultTraitIter4052
    n::Int64
end

length(x::DefaultTraitIter4052) = x.n
eltype(::Type{DefaultTraitIter4052}) = Float32

function iterate(x::DefaultTraitIter4052)
    if x.n <= 0
        return nothing
    end
    return (Float32(1), 2)
end

function iterate(x::DefaultTraitIter4052, state)
    if state > x.n
        return nothing
    end
    return (Float32(state), state + 1)
end

function collect(x::DefaultTraitIter4052)
    T = eltype(x)
    result = Vector{T}(undef, 0)
    next = iterate(x)
    while next !== nothing
        value, state = next
        push!(result, value)
        next = iterate(x, state)
    end
    return result
end

@testset "default iterator traits use type-object eltype (Issues #4052/#4130)" begin
    itr = DefaultTraitIter4052(3)

    @test typeof(Base.IteratorSize(DefaultTraitIter4052)) === typeof(Base.HasLength())
    @test typeof(Base.IteratorSize(itr)) === typeof(Base.HasLength())
    @test typeof(Base.IteratorEltype(DefaultTraitIter4052)) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorEltype(itr)) === typeof(Base.HasEltype())

    @test eltype(DefaultTraitIter4052) === Float32
    @test eltype(itr) === Float32

    vals = collect(itr)
    @test typeof(vals) === Vector{Float32}
    @test eltype(vals) === Float32
    @test length(vals) == 3
    @test vals[1] == Float32(1)
    @test vals[2] == Float32(2)
    @test vals[3] == Float32(3)

    @test typeof(Base.IteratorSize(Type)) === typeof(Base.HasLength())
    @test typeof(Base.IteratorSize(Any)) === typeof(Base.SizeUnknown())
    @test typeof(Base.IteratorEltype(Type)) === typeof(Base.HasEltype())
    @test typeof(Base.IteratorEltype(Any)) === typeof(Base.EltypeUnknown())
end

true
