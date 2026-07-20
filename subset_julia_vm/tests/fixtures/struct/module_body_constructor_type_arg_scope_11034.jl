using Test

module ScopeOwnerA11034
abstract type Bound end
struct V <: Bound end
struct Wrap{T} <: Bound end
struct C{T}
    C{T}() where {T<:Bound} = :a
end
const flat = C{V}()
const nested = C{Wrap{V}}()
make() = C{V}()
end

module ScopeOwnerB11034
abstract type Bound end
struct V <: Bound end
struct Wrap{T} <: Bound end
struct C{T}
    C{T}() where {T<:Bound} = :b
end
const flat = C{V}()
const nested = C{Wrap{V}}()
make() = C{V}()
end

@testset "module-body constructor type-argument scope" begin
    @test (ScopeOwnerA11034.flat, ScopeOwnerB11034.flat) == (:a, :b)
    @test (ScopeOwnerA11034.nested, ScopeOwnerB11034.nested) == (:a, :b)
    @test (ScopeOwnerA11034.make(), ScopeOwnerB11034.make()) == (:a, :b)
    @test ScopeOwnerA11034.C{ScopeOwnerA11034.V}() == :a
    @test ScopeOwnerB11034.C{ScopeOwnerB11034.V}() == :b
end

true
