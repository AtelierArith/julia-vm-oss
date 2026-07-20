# Owner-scoped UnionAll TypeVar projection identity survives a populated
# constructed-TypeVar cache (Issue #10420; regression shape from Issue #10412).
#
# ORDER IS LOAD-BEARING. The #10412 regression only surfaced after a user
# `TypeVar(:T)` had populated the constructed-TypeVar cache (the
# `Vector{T}.parameters[1] === T` identity path, Issue #4698): the split
# `.var` projection then disagreed with the `.parameters` projection of the
# same UnionAll body. So this fixture FIRST runs the cache-populating
# constructed-type pattern, THEN asserts the owner-scoped wrapper-chain
# identities. `UnionAll.var` and body `.parameters` are ONE owner-scoped
# identity domain; constructed parametric type arguments (`Vector{T}`) are a
# SEPARATE identity domain (docs/vm/CHECKLISTS.md, Issue #10420).
#
# Verified against julia 1.12 (all assertions hold upstream in this order).

using Test

@testset "reflection: Vector owner-scoped TypeVar projection identity after cache population (Issue #10420)" begin
    # Step 1 (cache population — must come first): a user TypeVar used as a
    # constructed type argument keeps its identity through `.parameters`.
    T = TypeVar(:T)
    @test Vector{T}.parameters[1] === T

    # Step 2: the wrapper chain still projects ONE owner-scoped identity —
    # `.var` and the body `.parameters` entry are the same TypeVar object.
    @test Vector.var === Vector.body.parameters[1]
    @test Vector.body.parameters[1] === Vector.var
    @test Vector.body.parameters[1] isa TypeVar

    # Step 3: the two identity domains stay separate — the wrapper-chain
    # TypeVar is NOT the user's constructed TypeVar.
    @test Vector.body.parameters[1] !== T
    @test Vector.var !== T
end

@testset "reflection: Dict owner-scoped TypeVar projection identity after cache population (Issue #10420)" begin
    # Step 1 (cache population — must come first): both Dict binders.
    K = TypeVar(:K)
    V = TypeVar(:V)
    @test Dict{K, V}.parameters[1] === K
    @test Dict{K, V}.parameters[2] === V

    # Step 2: the two-binder wrapper chain projects owner-scoped identities.
    @test Dict.var === Dict.body.body.parameters[1]
    @test Dict.body.var === Dict.body.body.parameters[2]
    @test Dict.body.body.parameters[1] isa TypeVar
    @test Dict.body.body.parameters[2] isa TypeVar

    # Step 3: identity domains stay separate.
    @test Dict.body.body.parameters[1] !== K
    @test Dict.body.body.parameters[2] !== V
end

true
