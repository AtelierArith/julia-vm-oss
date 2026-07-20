# A `where`-clause bound expression that references a genuinely undefined
# identifier must raise `UndefVarError`, matching upstream Julia -- not be
# silently accepted as a nominal (`Struct`) type placeholder (Issue #10226,
# discovered while fixing Issue #10100's where-binder shadow-scope bug).
#
# Root cause: `type_name_value`/`typevar_bound_value_expr`
# (subset_julia_vm_lowering/src/lowering/expr/mod.rs) had no way to distinguish "a
# legitimately free/unbound where-clause type variable" from "a typo / truly
# undefined global name" -- both fell through to the same permissive
# `Struct(name)` nominal-placeholder fallback. The fix: an unresolved
# where-bound name that is neither a declared where-binder in scope nor a
# recognized builtin/static type name is now lowered as an ordinary variable
# reference (`Expr::Var`), so it goes through normal runtime global lookup --
# resolving correctly for legitimately-declared globals (structs, abstract
# types, aliases) and raising `UndefVarError` for anything else, exactly like
# upstream.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

# Struct/abstract-type declarations live at top level (not nested inside a
# `@testset` block), matching the existing fixture convention.
struct WrapB10226{T} end
abstract type MyAbstract10226 end
struct MySub10226 <: MyAbstract10226 end
struct Foo10226 end
struct WrapC10226{T}
    x::T
end

@testset "where-bound UndefVarError: MWE reproduction (Issue #10226)" begin
    # Previously: silently accepted as `Vector{Int64} where
    # Int64<:SomeUndefinedName123` (a phantom nominal `Struct` bound), no
    # error at all.
    @test_throws UndefVarError (Vector{Int64} where Int64<:SomeUndefinedName123)

    # Self-referential bound naming a TypeVar that was never declared
    # anywhere else (distinct from Issue #10100's builtin-name-collision
    # self-reference, which correctly resolves to the builtin and must NOT
    # error -- see the regression testset below).
    @test_throws UndefVarError (Vector{T} where T<:T)
end

@testset "where-bound UndefVarError: broader undefined-name shapes (Issue #10226)" begin
    @test_throws UndefVarError (Vector{T} where T<:UndefinedStructABC10226)
    @test_throws UndefVarError (Vector{Int64} where Int64<:SomeUndefinedName123)

    # Undefined name in the UPPER bound of a two-sided `lower<:T<:upper` form.
    @test_throws UndefVarError (Vector{T} where Int64<:T<:SomeUndefUpperBound10226)

    # Undefined bound in the anonymous single-variable `Foo{X} where
    # X<:Bound` position (not just the two-name-collision shape).
    @test_throws UndefVarError (WrapB10226{T} where T<:AnotherUndefinedName10226)

    # Undefined bound reached via a multi-variable where clause.
    @test_throws UndefVarError (Pair{K,V} where {K<:Real, V<:StillUndefined10226})
end

@testset "where-bound regression guards: legitimate usage must NOT error (Issue #10226)" begin
    # Plain unbounded declaration -- the overwhelmingly common case.
    @test typeof(Vector{T} where T) == UnionAll

    # Builtin upper bounds.
    @test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
    @test string(Vector{T} where T<:Integer) == "Vector{T} where T<:Integer"

    # Issue #10100 regression: binder name colliding with a builtin type
    # name, both the self-referential and non-self-referential shapes, must
    # keep working with zero errors (this is NOT the #10226 bug -- the
    # bound resolves to the real builtin, not an undefined name).
    @test string(Vector{Int64} where Int64<:Int64) == "Vector{Int64} where Int64<:Int64"
    @test string(Vector{Int64} where Int64<:Real) == "Vector{Int64} where Int64<:Real"

    # Multiple where-bound variables, unbounded and partially bounded.
    # (Whether `Array{T,N} where N` collapses to the canonical `Array{T}`
    # alias in `string(...)` is an unrelated, pre-existing display
    # convention -- not part of Issue #10226 -- so assert structurally
    # instead of pinning the exact display string.)
    @test typeof(Array{T,N} where {T,N}) == UnionAll
    let a = Array{T,N} where {T<:Real,N}
        @test typeof(a) == UnionAll
        @test Array{Float64,1}(undef, 0) isa a
        @test !(Array{String,1}(undef, 0) isa a)
    end
    @test string(Dict{K,V} where {K<:Integer, V<:AbstractString}) ==
          "Dict{K, V} where {K<:Integer, V<:AbstractString}"

    # A where-bound var referencing a SIBLING where-bound var declared in the
    # same clause (must still resolve via the existing bound_scope lookup,
    # unaffected by the Issue #10226 fallback).
    @test string(Pair{K,V} where {K, V<:K}) == "Pair{K, V} where {K, V<:K}"

    # Chained where clauses: outer binder's bound refers to a builtin, inner
    # binder's bound refers to the outer binder.
    chained = Vector{T} where T<:S where S<:Real
    @test string(chained) == "Vector{T} where {S<:Real, T<:S}"
    @test Vector{Float64} <: chained
    # Value-level `Float64[1.0] isa chained` is tracked separately by Issue #10410.
    @test !(Any[1, 2] isa chained)

    # Issue #10274: the current binder must be excluded from its own bound
    # lookup without hiding an outer binder with the same name.
    same_name_nested = (Vector{T} where T<:T) where T<:Real
    @test Vector{Float64} <: same_name_nested
    @test Float64[1.0] isa same_name_nested
    @test !(Any[1, 2] isa same_name_nested)
    # Issue #10572: display/structural parity. Upstream keeps BOTH binders
    # (the outer `T<:Real` and the redundant inner `T<:T`); the inner binder
    # now references the outer binder's TypeVar (its `where`-binder let-local)
    # instead of substituting its bound value away, so the outer binder stays
    # USED and the constructed UnionAll no longer collapses to one binder.
    @test string(same_name_nested) == "Vector{T} where {T<:Real, T<:T}"

    # Issues #10301 / #10302 / #10303 (found on PR #10231, fixed by PR
    # #10411 + PR #10454): the UNPARENTHESIZED chain spelling of the same
    # nested same-name construct. Previously this spelling either corrupted
    # the reparsed type (`isa` -> false, #10301) or aborted the whole
    # process with an uncatchable Rust stack overflow during bound
    # resolution (#10302); the parenthesized spelling above additionally
    # failed to recurse through structural where-lowering (#10303). Both
    # spellings must lower identically and agree with upstream on every
    # semantic check, AND (Issue #10572) on `string`/`show` display.
    same_name_chain = Vector{T} where T<:T where T<:Real
    @test typeof(same_name_chain) == UnionAll
    @test Vector{Float64} <: same_name_chain
    @test Float64[1.0] isa same_name_chain
    @test Int64[1, 2] isa same_name_chain
    @test !(Any[1, 2] isa same_name_chain)
    @test !(["a"] isa same_name_chain)
    @test same_name_chain == same_name_nested
    @test string(same_name_chain) == "Vector{T} where {T<:Real, T<:T}"

    # A where-bound naming a REAL, previously-declared struct or abstract
    # type is exactly the risky "false positive" case this fix must not
    # break: the bound resolves via ordinary global lookup at runtime, not
    # a static literal, so a legitimately-defined global must still work.
    @test string(Vector{T} where T<:MyAbstract10226) ==
          "Vector{T} where T<:MyAbstract10226"

    # Self-referential collision with a REAL user struct name (not a
    # builtin) -- must resolve via global lookup, not error.
    @test string(Vector{Foo10226} where Foo10226<:Foo10226) ==
          "Vector{Foo10226} where Foo10226<:Foo10226"

    # A plain global reassignment used as a where-bound target.
    MyAlias10226 = Int64
    @test string(Vector{T} where T<:MyAlias10226) == "Vector{T} where T<:Int64"

    # Compound bound expressions (parametric / Union / qualified) are
    # untouched by this fix and must keep working exactly as before.
    @test string(Vector{T} where T<:Vector{Int}) == "Vector{T} where T<:Vector{Int64}"
    @test string(Vector{T} where T<:Union{Int,Float64}) ==
          "Vector{T} where T<:Union{Float64, Int64}"

    # Anonymous covariant bound shorthand with a builtin target (the
    # undefined-name variant of this shorthand is a separate, structurally
    # different gap tracked by Issue #10373 -- out of this issue's scope).
    @test string(Vector{<:Real}) == "Vector{<:Real}"

    # Function signature `where` with a legitimate builtin bound.
    f10226(x::Vector{T}) where T<:Real = length(x)
    @test f10226([1, 2, 3]) == 3

    g10226(x::T, y::S) where {T<:Real, S<:Integer} = (x, y)
    @test g10226(1.0, 2) == (1.0, 2)

    # Struct definitions with `where` in their (parametric) declaration are
    # a wholly different (declaration-position) lowering path, untouched by
    # this fix.
    @test string(WrapC10226) == "WrapC10226"
end

true
