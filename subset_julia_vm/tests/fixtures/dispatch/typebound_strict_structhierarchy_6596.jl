# Issue #6596: `Type{<:Bound}` bound names that are user abstracts / parametric
# spellings must be judged through the struct hierarchy, not permissively
# accepted. Pins the parity points against upstream julia 1.12.
using Test

abstract type Animal end
struct Dog <: Animal end
struct Cat <: Animal end
struct Rock end

describe(::Type{<:Animal}) = "animal"
describe(::Type) = "generic"

only_animal(::Type{<:Animal}) = "ok"

struct Tree end
m(::Type{Tree}) = "exact-tree"
m(::Type{<:Animal}) = "animal"

classify_pairs(::Type{<:Base.Pairs}) = "pairs"
classify_pairs(::Type) = "other"

function rock_method_errors()
    try
        only_animal(Rock)
        return false
    catch e
        return e isa MethodError
    end
end

@testset "Type{<:Bound} strict struct hierarchy (Issue #6596)" begin
    # `Type{<:UserAbstract}` subtyping via the `<:` operator.
    @test (Type{Dog} <: Type{<:Animal}) == true
    @test (Type{Cat} <: Type{<:Animal}) == true
    # The strictening: an unrelated concrete type is NOT a subtype.
    @test (Type{Rock} <: Type{<:Animal}) == false
    # Bare user-abstract bound on the value-type `<:`.
    @test (Dog <: Animal) == true
    @test (Rock <: Animal) == false

    # Dispatch on Type{<:UserAbstract}: matches subtypes, falls through for others.
    @test describe(Dog) == "animal"
    @test describe(Cat) == "animal"
    @test describe(Rock) == "generic"

    # A single Type{<:Animal} method MethodErrors for a non-Animal type object.
    @test only_animal(Dog) == "ok"
    @test rock_method_errors() == true

    # Exact Type{T} stays more specific than the bound.
    @test m(Tree) == "exact-tree"
    @test m(Dog) == "animal"

    # Pairs-family parametric bound (`Type{<:Base.Pairs}`) still resolves.
    p = pairs((a = 1, b = 2))
    @test classify_pairs(typeof(p)) == "pairs"
    @test classify_pairs(Int) == "other"
end

true
