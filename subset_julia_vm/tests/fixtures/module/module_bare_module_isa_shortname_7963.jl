# Regression test for Issue #7963: a bare `Module` `isa` check must resolve to
# `Base.Module` in the current scope, NOT loosely match a module-local abstract
# type whose short name also happens to be `Module`. Before the fix,
# `Box() isa Module` wrongly returned `true` because the module-local
# `TypeOwner7955.Module` abstract supertype was recorded under the bare short
# name `Module` and matched by short-name family.
using Test

module TypeOwner7963
abstract type Module end
struct Box <: Module end
end

@testset "bare Module isa does not short-name-match module-local abstract (Issue #7963)" begin
    # Bare `Module` in Main is `Base.Module`, distinct from `TypeOwner7963.Module`.
    @test (TypeOwner7963.Module === Module) == false
    @test (TypeOwner7963.Box() isa Module) == false

    # The qualified reference still resolves correctly: a Box IS a
    # TypeOwner7963.Module.
    @test TypeOwner7963.Box() isa TypeOwner7963.Module

    # Sanity: a Box is its own type, and not a Base.Module.
    @test TypeOwner7963.Box() isa TypeOwner7963.Box
    @test isa(TypeOwner7963.Box(), Module) == false
end

true
