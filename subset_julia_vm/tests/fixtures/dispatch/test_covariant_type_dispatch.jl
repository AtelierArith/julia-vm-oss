# Test covariant bounds (<:T) in Type dispatch (Issue #2526)
using Test

abstract type Animal end
struct Dog <: Animal end
struct Cat <: Animal end

# Functions with covariant bound and exact match
type_name(::Type{<:Animal}) = "animal"
type_name(::Type{Dog}) = "dog"

@testset "Covariant Type dispatch" begin
    # Dog: exact match prefers Type{Dog} over Type{<:Animal}
    @test type_name(Dog) == "dog"

    # Cat: no exact match, falls back to Type{<:Animal}
    @test type_name(Cat) == "animal"
end

true
