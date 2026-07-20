# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: macros/macro_return_catch_only_try_7832.jl =====
module Agg_macro_return_catch_only_try_7832
# Runtime-expanded macros that return a catch-only `try` (no `finally`) must
# lower correctly. Upstream Julia stores `Expr(:try)` as the 3-arg shape
# `[try_block, catch_var_or_false, catch_block_or_false]`; sjulia previously
# rejected it ("malformed Expr(:try, ...)" / "unsupported Expr head :try").
# Issue #7832 (sibling shape of #7806).

using Test

# catch with no variable, value position
macro catch_only_try()
    esc(:(try
        error("x")
    catch
        42
    end))
end

# catch binding a variable, value position
macro catch_var_try()
    esc(:(try
        error("boom")
    catch e
        string(e)
    end))
end

@testset "catch-only try (no finally) from runtime macro" begin
    @test (@catch_only_try) == 42
    @test (@catch_var_try) == "ErrorException(\"boom\")"
end

# success path of a catch-only try yields the try body value
macro catch_only_ok()
    esc(:(try
        100 + 1
    catch
        -1
    end))
end

@testset "catch-only try success path" begin
    @test (@catch_only_ok) == 101
    println(@catch_only_try)
end
end # module Agg_macro_return_catch_only_try_7832

# ===== source: macros/macro_return_quote_expr_args_7898.jl =====
module Agg_macro_return_quote_expr_args_7898
using Test

macro quote_expr_args_7898()
    ex = :(f(x))
    esc(Expr(:quote, ex.args))
end

@testset "macro-returned quote can rematerialize Expr.args arrays (Issue #7898)" begin
    args = @quote_expr_args_7898()
    @test length(args) == 2
    @test args[1] === :f
    @test args[2] === :x
    @test typeof(args) == Vector{Any}
end
end # module Agg_macro_return_quote_expr_args_7898

# ===== source: macros/macro_return_typed_expr_7628.jl =====
module Agg_macro_return_typed_expr_7628
using Test

macro macro_return_typed_expr_7628()
    esc(:(x::Int))
end

function macro_return_typed_expr_7628_f(x)
    @macro_return_typed_expr_7628
end

@testset "macro expansion lowers Expr(::) in value position (Issue #7628)" begin
    @test macro_return_typed_expr_7628_f(1) == 1
    @test_throws Exception macro_return_typed_expr_7628_f(1.5)
end
end # module Agg_macro_return_typed_expr_7628

# ===== source: macros/macro_return_typevar_curly_7830.jl =====
module Agg_macro_return_typevar_curly_7830
using Test

# Issue #7830: a macro that returns a parametric type expression containing a
# caller `where` type parameter (e.g. Expr(:curly, :Vector, :T)) must resolve T
# from the caller method's instantiation at runtime, not stringify it into a
# static TypeOf("Vector{T}") literal. The macro-return curly converter therefore
# routes through curly_expr_from_values -> DynamicTypeConstruct rather than the
# static type-name fast path.

macro caller_vector_type()
    esc(Expr(:curly, :Vector, :T))
end

function vector_type_for(x::T) where {T}
    @caller_vector_type()
end

# The quote form must behave identically.
macro quoted_vector_type()
    esc(:(Vector{T}))
end

quoted_vector_type_for(x::T) where {T} = @quoted_vector_type()

# A multi-parameter type that uses the same caller param twice.
macro caller_dict_type()
    esc(Expr(:curly, :Dict, :T, :T))
end

dict_type_for(x::T) where {T} = @caller_dict_type()

@testset "macro-returned curly resolves caller where type param (Issue #7830)" begin
    @test vector_type_for(1) == Vector{Int64}
    @test vector_type_for(1.0) == Vector{Float64}
    @test vector_type_for("a") == Vector{String}

    @test quoted_vector_type_for(1) == Vector{Int64}
    @test quoted_vector_type_for(1.0) == Vector{Float64}

    @test dict_type_for(1) == Dict{Int64, Int64}
    @test dict_type_for("a") == Dict{String, String}
end
end # module Agg_macro_return_typevar_curly_7830

# ===== source: macros/macro_return_where_bound_typevar_7924.jl =====
module Agg_macro_return_where_bound_typevar_7924
using Test

macro where_bound_typevar_7924()
    esc(:(Tuple{S} where {T, S<:T}))
end

macro where_bound_typevar_expr_7924()
    esc(Expr(:where, Expr(:curly, :Tuple, :S), :T, Expr(:<:, :S, :T)))
end

@testset "macro-returned where keeps typevars referenced by bounds (Issue #7924)" begin
    @test string(@where_bound_typevar_7924()) == "Tuple{S} where {T, S<:T}"
    @test string(@where_bound_typevar_expr_7924()) == "Tuple{S} where {T, S<:T}"
end
end # module Agg_macro_return_where_bound_typevar_7924

# ===== source: macros/macro_return_where_typevar_7844.jl =====
module Agg_macro_return_where_typevar_7844
using Test

# Issue #7844: a macro that returns a `where` type (Expr(:where, body, var...))
# must bind each introduced inner type variable as a runtime TypeVar(:var) fed to
# UnionAll, while still resolving caller-bound type params in the body
# dynamically. Previously the macro-return AST->IR converter had no Expr(:where,
# ...) arm, so the introduced inner S was lowered as an ordinary variable
# (UndefVarError) instead of being bound as TypeVar(:S). The fix routes the body
# through the curly/DynamicTypeConstruct path (so caller-bound T resolves) and
# binds each introduced var in a `let` to TypeVar(:S) passed to UnionAll.

macro tuple_where()
    esc(:(Tuple{T,S} where S))
end

function tuple_type_for(x::T) where T
    @tuple_where()
end

# Constructed-Expr form must behave identically to the quote form.
macro tuple_where_expr()
    esc(Expr(:where, Expr(:curly, :Tuple, :T, :S), :S))
end

tuple_type_for_expr(x::T) where T = @tuple_where_expr()

# A `where` whose only variable is the introduced one (no caller param in the
# body) — the body still references a caller binding via a concrete element.
macro vector_where()
    esc(:(Tuple{Vector{V}} where V))
end

vector_where_for(x::T) where T = @vector_where()

# NOTE: deliberately no top-level `T = Int64` here — a global binding named the
# same as a method `where` parameter collides in DynamicTypeConstruct
# (Issue #7847), which is independent of this macro-return fix.

@testset "macro-returned where binds inner TypeVar (Issue #7844)" begin
    @test tuple_type_for(1) == (Tuple{Int64, S} where S)
    @test tuple_type_for(1.0) == (Tuple{Float64, S} where S)
    @test string(tuple_type_for(1)) == "Tuple{Int64, S} where S"
    @test string(tuple_type_for(1.0)) == "Tuple{Float64, S} where S"

    @test tuple_type_for_expr(1) == (Tuple{Int64, S} where S)
    @test tuple_type_for_expr(1.0) == (Tuple{Float64, S} where S)

    @test vector_where_for(1) == (Tuple{Vector{V}} where V)
    @test string(vector_where_for(1)) == "Tuple{Vector{V}} where V"
end
end # module Agg_macro_return_where_typevar_7844

# ===== source: macros/nested_quote_call_interpolation_7507.jl =====
module Agg_nested_quote_call_interpolation_7507
using Test

struct TypeBind
    name::Symbol
    ts::Set{Any}
end

@testset "nested quote interpolation in call arguments (Issue #7507)" begin
    name = :x
    ts = [:call]
    ex = Expr(:$, :($TypeBind($(Expr(:quote, name)), Set{Any}([$(ts...)]))))
    inner = ex.args[1]
    set_call = inner.args[3]
    vect = set_call.args[2]

    @test ex isa Expr
    @test ex.head == :$
    @test inner isa Expr
    @test inner.head == :call
    @test inner.args[2] == Expr(:quote, :x)
    @test set_call isa Expr
    @test set_call.head == :call
    @test vect isa Expr
    @test vect.head == :vect
    @test vect.args[1] == :call
end
end # module Agg_nested_quote_call_interpolation_7507

# ===== source: macros/quote_const_declaration_5578.jl =====
module Agg_quote_const_declaration_5578
using Test

macro quote_const_declaration_5578(sym)
    quote
        const $(esc(sym)) = 5578
    end
end

@quote_const_declaration_5578 quote_const_value_5578

@testset "quote const declaration in macro expansion (Issue #5578)" begin
    @test quote_const_value_5578 == 5578
end
end # module Agg_quote_const_declaration_5578

# ===== source: macros/quote_global_declaration_9692.jl =====
module Agg_quote_global_declaration_9692
# Global declarations inside a macro quote lower as Expr(:global, ...).

using Test

macro macro_quote_global_declaration_9692()
    quote
        global _macro_global_hygiene_probe_9692
        _macro_global_hygiene_probe_9692 = 11
        _macro_global_hygiene_probe_9692
    end
end

macro macro_quote_multiple_globals_9692()
    quote
        global _macro_global_left_9692, _macro_global_right_9692
        _macro_global_left_9692 = 2
        _macro_global_right_9692 = 3
        _macro_global_left_9692 + _macro_global_right_9692
    end
end

@testset "macro quote global declaration (Issue #9692)" begin
    global _macro_global_hygiene_probe_9692 = 0
    global _macro_global_left_9692 = 0
    global _macro_global_right_9692 = 0

    seen = @macro_quote_global_declaration_9692
    pair_sum = @macro_quote_multiple_globals_9692

    @test seen == 11
    @test _macro_global_hygiene_probe_9692 == 11
    @test pair_sum == 5
    @test _macro_global_left_9692 == 2
    @test _macro_global_right_9692 == 3
end
end # module Agg_quote_global_declaration_9692

# ===== source: macros/quote_local_variable_hygiene_9619.jl =====
module Agg_quote_local_variable_hygiene_9619
# Quote-local variables introduced by a macro's bare quote are hygienic even
# when an escaped sibling loop mutates a caller global with the same name.

using Test

macro loopit_hygiene(forloop)
    newbody = Expr(
        :block,
        forloop.args[2],
        Expr(:global, :_c_hygiene),
        :(_c_hygiene += 1),
    )
    newloop = Expr(forloop.head, forloop.args[1], newbody)
    quote
        _c_hygiene = 1
        $(esc(newloop))
        _c_hygiene
    end
end

@testset "quote-local variable hygiene with escaped sibling loop" begin
    global _c_hygiene = 0
    local_seen = @loopit_hygiene for i in 1:3
        nothing
    end

    @test local_seen == 1
    @test _c_hygiene == 3
end

@testset "Base @time preserves caller assignment target" begin
    @time grid_9619 = 1
    @test grid_9619 == 1
end
end # module Agg_quote_local_variable_hygiene_9619

# ===== source: macros/quote_macro_definition_9134.jl =====
module Agg_quote_macro_definition_9134
using Test

ex = quote
    macro quoted_macro_9134(x)
        x
    end
end

macro_expr = ex.args[2]
signature = macro_expr.args[1]
body = macro_expr.args[2]

@testset "quoted macro definitions lower to Expr(:macro, ...) (Issue #9134)" begin
    @test ex.head == :block
    @test macro_expr.head == :macro
    @test length(macro_expr.args) == 2
    @test signature.head == :call
    @test signature.args[1] == :quoted_macro_9134
    @test signature.args[2] == :x
    @test body.head == :block
end
end # module Agg_quote_macro_definition_9134

# ===== source: macros/quote_splat_4904.jl =====
module Agg_quote_splat_4904
# Issue #4904: `:(f(args...))` previously fell through the
# quote-lowering catch-all with
# `UnsupportedExpression("quote for splat_expression not yet supported")`.
# Upstream Julia lowers `x...` to `Expr(:..., x)` — head is the
# three-dot Symbol literally named "...".
#
# Companion to #4899 (PR #4903, field access) — both surfaced from
# the audit fixture for #4893 (PR #4898) and live in the same file.
#
# Fix: in `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::SplatExpression` arm that emits
# `Expr(:..., inner)` where `inner` is the recursively-quoted child.

using Test

# Symbol for the `...` head — `:...` cannot currently be parsed by
# sjulia as a Symbol literal (`ParseFailed("unexpected token 'end of
# input'")`), so we construct it explicitly.
const SPLAT_HEAD = Symbol("...")

@testset "quoted splat lowers to Expr(:..., x) (Issue #4904)" begin
    ex = :(f(args...))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :f
    @test ex.args[2] isa Expr
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] === :args
end

@testset "quoted splat preserves position (Issue #4904)" begin
    # `g(a, xs..., b)` — splat in the middle of the arg list.
    ex = :(g(a, xs..., b))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :g
    @test ex.args[2] === :a
    @test ex.args[3] isa Expr
    @test ex.args[3].head === SPLAT_HEAD
    @test ex.args[3].args[1] === :xs
    @test ex.args[4] === :b
end

@testset "leading splat (Issue #4904)" begin
    # `h(xs...)` — splat is the only arg.
    ex = :(h(xs...))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :h
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] === :xs
end

@testset "splat of an arbitrary expression (Issue #4904)" begin
    # The inner expression is recursively quoted, so any Expr works.
    ex = :(f((a + b)...))
    @test ex.args[2] isa Expr
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] isa Expr   # recursively quoted (a + b)
end
end # module Agg_quote_splat_4904

# ===== source: macros/quoted_field_assignment_7630.jl =====
module Agg_quoted_field_assignment_7630
using Test

macro quoted_field_assignment_7630(x, f, v)
    QuoteNode(:($x.$f = $v))
end

@testset "macro expansion can return quoted field assignment Expr (Issue #7630)" begin
    ex = @quoted_field_assignment_7630 obj field 3

    @test ex isa Expr
    @test ex.head == :(=)
    @test string(ex.args[1]) == "obj.field"
    @test ex.args[2] == 3
    @test string(ex) == "obj.field = 3"
end
end # module Agg_quoted_field_assignment_7630

true
