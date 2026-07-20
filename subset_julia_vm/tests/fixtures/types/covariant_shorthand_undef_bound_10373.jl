# The anonymous covariant/contravariant bound shorthand (`Vector{<:Name}` /
# `Vector{>:Name}`) in VALUE position must raise `UndefVarError` when the
# bound names a genuinely undefined identifier, matching upstream Julia --
# not be silently accepted as a compound type-name literal (Issue #10373,
# sibling of Issue #10226's explicit-`where` bound validation).
#
# Root cause: `is_dynamic_type_arg` (subset_julia_vm_lowering/src/lowering/expr/mod.rs)
# classified EVERY `<:`/`>:`-prefixed type argument as static, so the whole
# expression lowered to one compound type-name string parsed permissively by
# `subset_julia_vm_types` (`parse_covariant_bound` builds an anonymous
# `TypeVar` with no existence check). Additionally, a top-level assignment
# whose RHS is such an expression was registered as a static string type
# alias (`extract_type_alias_from_binding`), bypassing runtime resolution
# entirely. The fix keeps the shorthand static only when the bound resolves
# against the static tables (builtin/static names, registered aliases,
# compound bound expressions); a bare-identifier bound nothing static knows
# now routes through the dynamic type-construct path, where
# `lower_anonymous_bound_type_arg` -> `typevar_bound_value_expr` (Issue
# #10226) resolves the bound via ordinary runtime global lookup -- resolving
# user structs / abstract types / runtime type-valued globals correctly and
# raising `UndefVarError` for anything else.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

# Struct/abstract-type declarations live at top level, matching fixture
# convention.
abstract type AbsCov10373 end
struct SubCov10373 <: AbsCov10373 end
struct FooCov10373 end
struct RockCov10373 end

# Top-level assignment of a covariant-bound shorthand naming a REAL user
# struct / abstract type: previously registered as a static string alias, now
# routed through ordinary assignment + the dynamic construct path -- the
# resulting global must still hold the correct UnionAll value.
glob_cov_10373 = Vector{<:FooCov10373}
glob_nested_10373 = Dict{String, <:AbsCov10373}

@testset "anonymous bound shorthand UndefVarError: MWE (Issue #10373)" begin
    # Previously: silently accepted, printed `Vector{<:SomeUndefinedNameABC}`.
    @test_throws UndefVarError Vector{<:SomeUndefinedNameABC10373}

    # Contravariant shorthand has the same gap.
    @test_throws UndefVarError Vector{>:SomeUndefinedNameABC10373}

    # Nested inside a multi-parameter type application.
    @test_throws UndefVarError Dict{String, <:SomeUndefinedNameABC10373}
    @test_throws UndefVarError Dict{String, >:SomeUndefinedNameABC10373}

    # Nested inside an inner parametric type argument.
    @test_throws UndefVarError Vector{Vector{<:SomeUndefinedNameABC10373}}

    # `Type{<:Undef}` shorthand.
    @test_throws UndefVarError Type{<:SomeUndefinedNameABC10373}
end

# Assignment position: inside a function body (ordinary, non-alias statement
# lowering) the undefined bound must also raise at runtime.
function local_undef_bound_10373()
    x = Vector{<:NopeUndefLocal10373}
    x
end

@testset "anonymous bound shorthand UndefVarError: assignment shapes (Issue #10373)" begin
    @test_throws UndefVarError local_undef_bound_10373()
end

@testset "anonymous bound shorthand regression guards (Issue #10373)" begin
    # Builtin bounds stay on the static path -- display unchanged.
    @test string(Vector{<:Real}) == "Vector{<:Real}"
    @test string(Vector{>:Int}) == "Vector{>:Int64}"
    @test string(Dict{<:Integer, <:AbstractString}) ==
          "Dict{<:Integer, <:AbstractString}"
    @test string(Vector{Vector{<:Real}}) == "Vector{Vector{<:Real}}"

    # A user struct / abstract type bound resolves via runtime global lookup
    # (the risky false-positive case this fix must not break).
    @test string(Vector{<:FooCov10373}) == "Vector{<:FooCov10373}"
    @test string(Vector{<:AbsCov10373}) == "Vector{<:AbsCov10373}"
    @test string(Vector{>:SubCov10373}) == "Vector{>:SubCov10373}"
    @test string(Dict{String, <:AbsCov10373}) == "Dict{String, <:AbsCov10373}"

    # Top-level assignments captured above (previously the static-string
    # alias path) hold the same values.
    @test string(glob_cov_10373) == "Vector{<:FooCov10373}"
    @test string(glob_nested_10373) == "Dict{String, <:AbsCov10373}"

    # Subtype semantics through the dynamic route are unchanged.
    @test Vector{SubCov10373} <: Vector{<:AbsCov10373}
    @test !(Vector{RockCov10373} <: Vector{<:AbsCov10373})
    @test Type{SubCov10373} <: Type{<:AbsCov10373}
    @test !(Type{RockCov10373} <: Type{<:AbsCov10373})

    # A runtime type-valued global as the bound now resolves to its VALUE
    # (previously frozen as the phantom nominal name `B2`).
    B2 = typeof(1)
    @test string(Vector{<:B2}) == "Vector{<:Int64}"

    # Annotation-position (signature) covariant shorthand is a separate
    # lowering path and must be unaffected.
    len_cov_10373(x::Vector{<:Real}) = length(x)
    @test len_cov_10373([1, 2, 3]) == 3
    classify_10373(::Type{<:AbsCov10373}) = "animal"
    classify_10373(::Type) = "other"
    @test classify_10373(SubCov10373) == "animal"
    @test classify_10373(RockCov10373) == "other"
end

true
