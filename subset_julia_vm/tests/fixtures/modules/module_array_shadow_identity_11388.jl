# A user struct named Array in a module is an unrelated nominal type: it is
# NOT isa/<: the Base-owned native Array family, its module-local bare
# annotation (Base.iterate(::Array) inside the module) still dispatches to the
# local struct via signature-side owner qualification, and splat stays
# method-first (Issues #11388/#11395).

using Test

module Faux11388
struct Array
    data::Memory{Int}
    dims::Tuple{Int}
end
Base.iterate(::Array) = (99, nothing)
Base.iterate(::Array, ::Nothing) = nothing
end

memory = Memory{Int}(undef, 2)
memory[1] = 1
memory[2] = 2
a = Faux11388.Array(memory, (2,))

f11388(xs...) = xs

@testset "user Array struct keeps its own identity (Issue #11388)" begin
    @test !(Faux11388.Array <: Base.Array)
    @test !isa(a, Base.Array)
    @test isa(a, Faux11388.Array)
    @test isa([1, 2], Base.Array)
    @test f11388(a...) == (99,)
    @test string(typeof(a)) == "Main.Faux11388.Array"
end

true
