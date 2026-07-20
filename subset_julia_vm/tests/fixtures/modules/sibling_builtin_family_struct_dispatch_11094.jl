# Sibling-module structs whose bare name COLLIDES with an internal
# builtin-container family name (`Vector`, `Dict`, `Pair`, ...) must still be
# two distinct declarations with two distinct methods (Issue #11094).
#
# This is the one shape of #11094's registration collapse that
# `sibling_same_named_struct_dispatch_11076.jl` does not cover. It is a
# different gate from the plain sibling case: `CoreType::from_julia_name`'s
# `is_known_struct_family` arm recognizes these bare names as builtin families
# and used to strip the module qualification at `MethodSig` construction time,
# so `MethodTable::add_method`'s same-signature dedup treated the second
# definition as a REDEFINITION and silently dropped the first — a lost method,
# not an ambiguity. Fixed by the owner-preserving dispatch projection of
# Issue #11076 (PR #11138); pinned here so the builtin-family gate cannot
# regress independently of the plain sibling case.
#
# A module struct named `Dict` used to hit a different defect: its constructor
# call was routed to Base's `Dict()` (Issue #11153). Keep that constructor-owner
# collision in this fixture alongside the builtin-family registration case.

using Test

module ShadowVec11094
struct Vector
    x::Int
end
end

module ShadowVec211094
struct Vector
    x::Int
end
end

module ShadowDict11153
struct Dict
    k::Int
end
Dict(s::String) = Dict(length(s))
Dict(xs::Vararg{Int}) = Dict(sum(xs))
make_dict(k) = Dict(k)
make_dict_splat(xs) = Dict(xs...)
make_base_dict() = Base.Dict(:a => 1)
make_typed_base_dict() = Base.Dict{Symbol,Int}(:a => 1)
end

module ShadowInnerDict11153
struct Dict
    k::Int
    Dict(k::Int) = new(k + 1)
end
end

module ShadowParamDict11153
struct Dict{T}
    k::T
end
make_explicit_dict_splat(xs) = Dict{Int}(xs...)
end

module ShadowSet11367
struct Set
    k::Int
end

make_set(k) = Set(k)
make_set_splat(xs) = Set(xs...)
make_base_set() = Base.Set([1, 2, 2])
make_typed_base_set() = Base.Set{Int}([1, 2, 2])
end

module BaseSetExtension11367
struct Token end
Base.Set(::Token) = :base_set_extension_11367
end

module ShadowSetExtensionCaller11367
struct Set
    k::Int
end
call_base_set(x) = Base.Set(x)
end

module ShadowKwDictA11368
struct Dict
    k::Int
end
Dict(; k::Int) = Dict(k)
end

module ShadowKwDictB11368
struct Dict
    k::Int
end
end

module ShadowOuterDefaultSplat11371
struct Dict
    k::Int
end
Dict(s::String) = Dict(length(s))
make(xs) = Dict(xs...)
end

module ShadowLexicalParam11373
struct Dict{T}
    k::T
end
call(Dict, xs) = Dict{Int}(xs...)
capture(Dict) = xs -> Dict{Int}(xs...)
call_kw(Dict) = Dict{Int}(; k = 1)
capture_kw(Dict) = () -> Dict{Int}(; k = 2)
capture_kw_splat(Dict) = nt -> Dict{Int}(; nt...)
end

module ShadowForeignParam11373
struct Dict{T}
    k::T
end
Dict{T}(; k) where {T} = Dict{T}(k + 10)
end

module ShadowDynamicOrder11375
struct Pair{T,S}
    k::Int
end

struct Outer{T}
    a::T
    b::T
end
Outer{T}(x) where {T} = Outer{T}(x, x)
end

# Declare the SECOND module's method first: if registration collapsed, the
# survivor would be whichever was defined last, so source order must not
# decide the answer.
vec_owner(::ShadowVec211094.Vector) = :vec2
vec_owner(::ShadowVec11094.Vector) = :vec1

# Force a dynamic (CallDynamic) call too, so a warmed dispatch cache cannot
# paper over a collapsed method table.
dynamic_vec_owner(x) = vec_owner(x)

@testset "sibling builtin-family-named structs keep distinct methods (Issue #11094)" begin
    v1 = ShadowVec11094.Vector(1)
    v2 = ShadowVec211094.Vector(2)

    # Both methods still exist: the registration dedup must not have collapsed
    # them into one.
    @test vec_owner(v1) == :vec1
    @test vec_owner(v2) == :vec2
    @test dynamic_vec_owner(v1) == :vec1
    @test dynamic_vec_owner(v2) == :vec2

    # The user structs shadow the bare Base name only inside their own module;
    # they are unrelated declarations with distinct identities.
    @test typeof(v1) !== typeof(v2)

    # ...and the real Base containers are untouched by the shadowing.
    @test [1, 2, 3] isa Base.Vector
    @test length([1, 2, 3]) == 3
end

@testset "module-owned Dict constructors preserve their owner (Issue #11153)" begin
    default_dict = ShadowDict11153.Dict(1)
    outer_dict = ShadowDict11153.Dict("abc")
    outer_splat_dict = ShadowDict11153.Dict((2, 3)...)
    bare_dict = ShadowDict11153.make_dict(4)
    bare_splat_dict = ShadowDict11153.make_dict_splat((5, 6))
    inner_dict = ShadowInnerDict11153.Dict(1)
    param_dict = ShadowParamDict11153.Dict(4)
    explicit_param_dict = ShadowParamDict11153.Dict{Int}(7)
    explicit_param_splat_dict = ShadowParamDict11153.make_explicit_dict_splat((8,))
    eval_events = Symbol[]
    ordered_param_dict = ShadowParamDict11153.Dict{
        (push!(eval_events, :type); Int)
    }(((push!(eval_events, :arg); 9),)...)

    @test default_dict.k == 1
    @test outer_dict.k == 3
    @test outer_splat_dict.k == 5
    @test bare_dict.k == 4
    @test bare_splat_dict.k == 11
    @test inner_dict.k == 2
    @test param_dict.k == 4
    @test explicit_param_dict.k == 7
    @test explicit_param_splat_dict.k == 8
    @test ordered_param_dict.k == 9
    @test eval_events == [:type, :arg]
    @test default_dict isa ShadowDict11153.Dict
    @test outer_splat_dict isa ShadowDict11153.Dict
    @test bare_splat_dict isa ShadowDict11153.Dict
    @test inner_dict isa ShadowInnerDict11153.Dict
    @test param_dict isa ShadowParamDict11153.Dict{Int}
    @test explicit_param_dict isa ShadowParamDict11153.Dict{Int}
    @test explicit_param_splat_dict isa ShadowParamDict11153.Dict{Int}

    # Explicit Base ownership must survive both bare and parameterized
    # constructor paths even from inside the shadowing module (Issue #11369).
    base_dict = Base.Dict(:a => 1)
    nested_base_dict = ShadowDict11153.make_base_dict()
    typed_base_dict = Base.Dict{Symbol,Int}(:b => 2)
    nested_typed_base_dict = ShadowDict11153.make_typed_base_dict()
    @test base_dict[:a] == 1
    @test nested_base_dict[:a] == 1
    @test typed_base_dict isa Base.Dict{Symbol,Int}
    @test typed_base_dict[:b] == 2
    @test nested_typed_base_dict isa Base.Dict{Symbol,Int}
    @test nested_typed_base_dict[:a] == 1
end

@testset "module-owned Set constructor preserves its owner (Issue #11367)" begin
    user_set = ShadowSet11367.Set(5)
    bare_set = ShadowSet11367.make_set(6)
    bare_splat_set = ShadowSet11367.make_set_splat((7,))
    @test user_set.k == 5
    @test bare_set.k == 6
    @test bare_splat_set.k == 7
    @test user_set isa ShadowSet11367.Set
    @test bare_splat_set isa ShadowSet11367.Set

    base_set = Base.Set([1, 2, 2])
    nested_base_set = ShadowSet11367.make_base_set()
    typed_base_set = Base.Set{Int}([2, 3, 3])
    nested_typed_base_set = ShadowSet11367.make_typed_base_set()
    @test length(base_set) == 2
    @test length(nested_base_set) == 2
    @test typed_base_set isa Base.Set{Int}
    @test length(typed_base_set) == 2
    @test nested_typed_base_set isa Base.Set{Int}
    @test length(nested_typed_base_set) == 2

    # A shadowing owner must not make the Base-qualified constructor snapshot
    # discard legitimate user extensions of the Base family.
    @test ShadowSetExtensionCaller11367.call_base_set(BaseSetExtension11367.Token()) ==
          :base_set_extension_11367
end

@testset "constructor keyword dispatch preserves its owner (Issue #11368)" begin
    keyword_dict = ShadowKwDictA11368.Dict(k = 9)
    keyword_args = (k = 10,)
    keyword_splat_dict = ShadowKwDictA11368.Dict(; keyword_args...)

    @test keyword_dict.k == 9
    @test keyword_splat_dict.k == 10
    @test keyword_dict isa ShadowKwDictA11368.Dict
    @test keyword_splat_dict isa ShadowKwDictA11368.Dict
    @test_throws MethodError ShadowKwDictB11368.Dict(k = 11)
    @test_throws MethodError ShadowKwDictB11368.Dict(; keyword_args...)
end

@testset "splat dispatch retains outer/default precedence (Issue #11371)" begin
    default_dict = ShadowOuterDefaultSplat11371.Dict((12,)...)
    bare_default_dict = ShadowOuterDefaultSplat11371.make((13,))
    outer_dict = ShadowOuterDefaultSplat11371.Dict(("abcd",)...)

    @test default_dict.k == 12
    @test bare_default_dict.k == 13
    @test outer_dict.k == 4
    @test default_dict isa ShadowOuterDefaultSplat11371.Dict
    @test bare_default_dict isa ShadowOuterDefaultSplat11371.Dict
end

@testset "lexical parametric DataType calls retain the callee (Issue #11373)" begin
    local_dict = ShadowLexicalParam11373.call(ShadowForeignParam11373.Dict, (14,))
    captured_dict = ShadowLexicalParam11373.capture(ShadowForeignParam11373.Dict)((15,))
    keyword_dict = ShadowLexicalParam11373.call_kw(ShadowForeignParam11373.Dict)
    captured_keyword_dict = ShadowLexicalParam11373.capture_kw(ShadowForeignParam11373.Dict)()
    captured_keyword_splat_dict =
        ShadowLexicalParam11373.capture_kw_splat(ShadowForeignParam11373.Dict)((k = 3,))

    @test local_dict.k == 14
    @test captured_dict.k == 15
    @test keyword_dict.k == 11
    @test captured_keyword_dict.k == 12
    @test captured_keyword_splat_dict.k == 13
    @test typeof(local_dict) === ShadowForeignParam11373.Dict{Int}
    @test typeof(captured_dict) === ShadowForeignParam11373.Dict{Int}
    @test typeof(keyword_dict) === ShadowForeignParam11373.Dict{Int}
    @test typeof(captured_keyword_dict) === ShadowForeignParam11373.Dict{Int}
    @test typeof(captured_keyword_splat_dict) === ShadowForeignParam11373.Dict{Int}
end

@testset "dynamic parametric callees precede value arguments (Issue #11375)" begin
    events = Symbol[]
    pair = ShadowDynamicOrder11375.Pair{
        (push!(events, :type1); Int),
        (push!(events, :type2); Float64),
    }((push!(events, :arg); 4))
    outer = ShadowDynamicOrder11375.Outer{
        (push!(events, :outer_type); Int),
    }((push!(events, :outer_arg); 5))

    @test events == [:type1, :type2, :arg, :outer_type, :outer_arg]
    @test pair.k == 4
    @test typeof(pair) === ShadowDynamicOrder11375.Pair{Int,Float64}
    @test (outer.a, outer.b) == (5, 5)
    @test typeof(outer) === ShadowDynamicOrder11375.Outer{Int}
end

true
