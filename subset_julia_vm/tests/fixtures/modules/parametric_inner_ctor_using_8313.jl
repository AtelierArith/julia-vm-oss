using Test

# Issue #8313: a parametric struct with an inner constructor, exported from a
# module and brought into scope with `using .M`, must be callable by its bare
# name. The bare name `Perm` collides with the bundled `Base.Order.Perm` (which
# has 2 type parameters), and the resolver picked a `*.Perm` key by HashMap order
# — so `Perm([1,2,3])` nondeterministically resolved to `Order.Perm` and failed
# with "Order.Perm{...} expects 2 type parameters, got 1". The fix resolves a
# bare name to the in-scope struct (current module / `using`-imported) first.

module M8313
    mutable struct Perm{T<:Integer}
        d::Vector{T}
        function Perm(d)
            return new{Int}(d)
        end
    end
    export Perm
end

using .M8313

@testset "bare exported parametric inner constructor (Issue #8313)" begin
    p = Perm([1, 2, 3])
    @test length(p.d) == 3
    @test p.d == [1, 2, 3]
    @test p isa M8313.Perm

    # Deterministic across repeated resolution (was HashMap-order dependent).
    for _ in 1:20
        @test length(Perm([10, 20]).d) == 2
    end
end

@testset "qualified form still works (Issue #8313)" begin
    q = M8313.Perm([4, 5])
    @test length(q.d) == 2
    @test q.d == [4, 5]
end

# The bundled `Base.Order.Perm` must still resolve inside its own module: sorting
# helpers that use it internally keep working (the user `Perm` does not shadow it
# for Base.Order's own references).
@testset "Base.Order sorting unaffected (Issue #8313)" begin
    @test sortperm([3, 1, 2]) == [2, 3, 1]
    @test sort([3, 1, 2]) == [1, 2, 3]
end

true
