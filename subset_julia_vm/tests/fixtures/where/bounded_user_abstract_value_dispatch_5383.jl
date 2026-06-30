# Issue #5383 (sub-case 1): a value-position bounded type variable whose upper
# bound is a USER-defined abstract type — `f(x::T) where {T<:Animal}` — must
# match a struct argument whose declared supertype chain reaches that abstract
# type. Previously `fb(Dog())` raised `NoMethodFound`: a user struct (`Dog`) and
# a user abstract type (`Animal`) both lower to `CoreType::Named`, and the
# `(Named, Named)` arm of `CoreType::is_subtype_of` was hardcoded to `false`, so
# the bounded method was never even considered.
#
# The fix resolves `Named <: Named` through the struct/abstract-parent registry
# (also seeded with abstract-type supertype links so an intermediate user
# abstract such as `Mammal` is walked transitively).
#
# Note: value-position *specificity ranking* of a bounded typevar method against
# an untyped `f(x)` fallback (sub-cases 2 and 3 of #5383) is a separate, still
# open problem (see the closing note in `long_form_bound_respected_5374.jl`), so
# this fixture deliberately competes only with concrete methods.

using Test

abstract type Animal end
abstract type Mammal <: Animal end   # intermediate abstract (transitive chain)
struct Cat <: Animal end             # direct subtype
struct Dog <: Mammal end             # transitive: Dog <: Mammal <: Animal
struct Fish <: Animal end            # direct subtype, no concrete method

fb(x::T) where {T<:Animal} = :bounded_animal
fb(x::Cat) = :cat
fb(x::Int64) = :int

@testset "bounded user-abstract typevar value dispatch (Issue #5383)" begin
    @test fb(Cat()) == :cat                # concrete struct beats the bounded typevar
    @test fb(Dog()) == :bounded_animal      # transitive subtype matches the bound
    @test fb(Fish()) == :bounded_animal     # direct subtype matches the bound
    @test fb(5) == :int                     # Int64 is NOT <: Animal: the bound is respected
end

true
