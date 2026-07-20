# Regression test for Issue #11021 (fixed as part of Issue #10989, StructId
# Phase 2b): same-named structs declared in sibling modules must NOT compare
# `==`/`===` as one type. Display already distinguished them
# (`A1x.Box{Int64}` vs `A2x.Box{Int64}`), but type equality/identity used to
# strip module qualification unconditionally before comparing, so two
# structurally unrelated types from different modules collapsed into one.
#
# The fix is a module-prefix-AWARE comparison, not a blanket strip: a BARE
# reference still legitimately denotes the same type as a QUALIFIED reference
# to the SAME declaration (Issue #8100, covered by its own fixture
# `module_short_type_name_value_8100.jl`), but two DIFFERENT modules'
# same-named declarations must stay distinct even when their bare tails
# match.
using Test

module A1x11021
struct Box{T}
    x::T
end
end

module A2x11021
struct Box{T}
    x::T
end
end

# Nested (nested-module) same-named structs.
module Outer1x11021
module Inner1x11021
struct Box{T} end
end
end

module Outer2x11021
module Inner2x11021
struct Box{T} end
end
end

# A struct that shadows a Base type NAME (`Vector`) inside two different
# user modules -- must not collapse into "the Base Vector type" or into each
# other.
module ShadowVec1x11021
struct Vector end
end

module ShadowVec2x11021
struct Vector end
end

@testset "same-named struct identity across sibling modules (Issue #11021)" begin
    # Parametric application: `M1.Box{Int} == M2.Box{Int}` must be false.
    @test (A1x11021.Box{Int} == A2x11021.Box{Int}) == false
    @test (A1x11021.Box{Int} === A2x11021.Box{Int}) == false

    # Bare (unparameterized) type object comparison.
    @test (A1x11021.Box == A2x11021.Box) == false
    @test (A1x11021.Box === A2x11021.Box) == false

    # A struct is still identical to itself (sanity: the fix must not make
    # every struct comparison false).
    @test A1x11021.Box{Int} == A1x11021.Box{Int}
    @test A1x11021.Box{Int} === A1x11021.Box{Int}
    @test A1x11021.Box === A1x11021.Box

    # Instance-level equality and typeof identity.
    @test A1x11021.Box(1) == A1x11021.Box(1)
    @test typeof(A1x11021.Box(1)) == typeof(A1x11021.Box(1))
    @test (typeof(A1x11021.Box(1)) == typeof(A2x11021.Box(1))) == false
    @test (typeof(A1x11021.Box(1)) === typeof(A2x11021.Box(1))) == false

    # Nested modules: same base struct name at the same nesting depth under
    # different parents must stay distinct.
    @test (Outer1x11021.Inner1x11021.Box{Int} == Outer2x11021.Inner2x11021.Box{Int}) == false
    @test Outer1x11021.Inner1x11021.Box{Int} === Outer1x11021.Inner1x11021.Box{Int}

    # Base-shadowing struct names: two unrelated modules both declaring
    # `struct Vector end` must not collapse into each other (nor into the
    # real `Base.Vector`, though that is covered structurally by module
    # qualification already distinguishing "Vector" from "M.Vector").
    @test (ShadowVec1x11021.Vector == ShadowVec2x11021.Vector) == false
    @test ShadowVec1x11021.Vector === ShadowVec1x11021.Vector
end

true
