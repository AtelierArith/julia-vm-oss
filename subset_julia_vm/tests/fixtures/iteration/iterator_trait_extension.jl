using Test

import Base: IteratorSize, IteratorEltype, length, iterate

struct MixedEltypeUnknownIterator
end

IteratorSize(::MixedEltypeUnknownIterator) = Base.HasLength()
IteratorEltype(::MixedEltypeUnknownIterator) = Base.EltypeUnknown()
length(::MixedEltypeUnknownIterator) = 2
iterate(::MixedEltypeUnknownIterator) = (1, 2)

function iterate(::MixedEltypeUnknownIterator, state)
    if state == 2
        return (2.0, 3)
    end
    return nothing
end

@testset "Iterator trait extension preserves Base methods (Issues #4088/#4102)" begin
    @test typeof(Base.IteratorEltype(1:5)) === typeof(Base.HasEltype())

    mixed = MixedEltypeUnknownIterator()
    collected = collect(mixed)
    @test typeof(collected) === Vector{Real}
    @test eltype(collected) === Real
    @test length(collected) == 2
    @test collected[1] == 1
    @test collected[2] == 2.0
end

struct RuntimeEltypeIterator
end

Base.eltype(::RuntimeEltypeIterator) = Float32
runtime_eltype(x::Any) = eltype(x)

@testset "Runtime eltype dispatch for user structs (Issue #4052)" begin
    itr = RuntimeEltypeIterator()
    @test eltype(itr) === Float32
    @test runtime_eltype(itr) === Float32
end

true
