# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: macros/doc_on_macro_def_9159.jl =====
module Agg_doc_on_macro_def_9159
# Issue #9159: `@doc` applied to a *macro* definition must define the macro
# (documentation is dropped, since the doc system is a no-op) instead of raising
# `lowering error ... macro expansion returned unsupported Expr head :macro`.
#
# `@doc(str, ex)` expands to `esc(ex)`, so the documented macro definition arrives
# at statement conversion as `Expr(:escape, Expr(:macro, sig, body))`. Before this
# fix `value_to_stmt` had no `:macro` arm and fell through to expression conversion,
# which rejected the `:macro` head. AbstractAlgebra's `@req` (in `Assertions.jl`)
# is written exactly this way, so `using AbstractAlgebra` failed to load.
using Test

@doc raw"""
    @req(cond, msg)

Throw an `ArgumentError` carrying `msg` when `cond` is false.
"""
macro req(cond, msg)
    quote
        if !($(esc(cond)))
            throw(ArgumentError($(esc(msg))))
        end
    end
end

# A second, varargs `@doc`'d macro, to cover splat params on the reconstructed
# signature.
@doc "sum the given expressions" macro sumall(xs...)
    expr = :(0)
    for x in xs
        expr = :($expr + $(esc(x)))
    end
    expr
end

@testset "@doc on a macro definition (Issue #9159)" begin
    # The documented macro is defined and usable.
    @req 1 == 1 "never thrown"
    @test_throws ArgumentError @req(1 == 2, "boom")

    # Escaped operands resolve in the caller scope.
    x = 10
    @req x > 0 "x must be positive"
    @test_throws ArgumentError (@req x < 0 "x must be negative")

    # Varargs @doc'd macro expands correctly.
    @test (@sumall 1 2 3) == 6
    @test (@sumall 4 5) == 9
end
end # module Agg_doc_on_macro_def_9159

# ===== source: macros/inference_meta_noop_4286.jl =====
module Agg_inference_meta_noop_4286
# Test inference metadata annotation macros compatibility wrappers (Issue #4286)

using Test

Base.@nospecializeinfer function f_4286(x)
    return x
end

@noinline function g_4286(x)
    return f_4286(x)
end

Base.@propagate_inbounds h_4286(x) = x + 1

@inline function inline_long_4286(x)
    return x + 3
end

Base.@inline inline_short_4286(x) = x + 4

Base.@propagate_inbounds @inline nested_inline_4286(x) = x + 5

Base.@constprop :aggressive function constprop_long_4286(x)
    return x + 6
end

Base.@constprop :none constprop_short_4286(x) = x + 7

Base.@assume_effects :foldable function assume_effects_long_4286(x)
    return x + 8
end

Base.@assume_effects :terminates_locally assume_effects_short_4286(x) = x + 9

callee_inline_expr_4286(x) = x + 1

function inline_expr_call_4286(x)
    y = @inline callee_inline_expr_4286(x)
    z = @noinline (callee_inline_expr_4286(x) + 1)
    y + z
end

function constprop_statement_4286(x)
    Base.@constprop :aggressive
    return x + 10
end

function assume_effects_statement_4286(x)
    Base.@assume_effects :foldable
    return x + 11
end

function assume_effects_callsite_4286(x)
    y = Base.@assume_effects :foldable assume_effects_long_4286(x)
    return y + 1
end

function inline_statement_marker_4286(x)
    @inline
    return x + 12
end

function noinline_statement_marker_4286(x)
    @noinline
    return x + 13
end

@inline function boundscheck_guard_4286()
    @boundscheck return 1
    return 2
end

boundscheck_caller_4286() = @inbounds boundscheck_guard_4286()

@inline function boundscheck_specialized_guard_4286(x)
    @boundscheck return x + 100
    return x + 1
end

boundscheck_specialized_caller_4286(x) = @inbounds boundscheck_specialized_guard_4286(x)

function nospecialize_param_4286(@nospecialize x)
    return x + 1
end

function specialize_param_4286(@specialize(x))
    return x + 2
end

function nospecialize_statement_4286(x, y)
    @nospecialize x y
    x + y
end

function specialize_statement_4286(x, y)
    @nospecialize x y
    @specialize
    x * y
end

@testset "inference metadata annotations parse and execute" begin
    @test g_4286(3) == 3
    Base.@boundscheck @test h_4286(2) == 3
    @test inline_long_4286(2) == 5
    @test inline_short_4286(2) == 6
    @test nested_inline_4286(2) == 7
    @test constprop_long_4286(2) == 8
    @test constprop_short_4286(2) == 9
    @test assume_effects_long_4286(2) == 10
    @test assume_effects_short_4286(2) == 11
    @test inline_expr_call_4286(2) == 7
    @test constprop_statement_4286(2) == 12
    @test assume_effects_statement_4286(2) == 13
    @test assume_effects_callsite_4286(2) == 11
    @test inline_statement_marker_4286(2) == 14
    @test noinline_statement_marker_4286(2) == 15
    @test boundscheck_guard_4286() == 1
    @test boundscheck_caller_4286() == 2
    @test boundscheck_specialized_guard_4286(1) == 101
    @test boundscheck_specialized_caller_4286(1) == 2
    @test nospecialize_param_4286(2) == 3
    @test specialize_param_4286(2) == 4
    @test nospecialize_statement_4286(2, 3) == 5
    @test specialize_statement_4286(2, 3) == 6
end
end # module Agg_inference_meta_noop_4286

# ===== source: macros/isdefined.jl =====
module Agg_isdefined
# Test @isdefined macro (Issue #451)
# - Check if variable is defined in current scope
# - Works with undefined variables
# - Works inside functions

using Test

# Function definition must be outside @testset
function test_local_isdefined()
    a = 10
    result1 = @isdefined(a)
    result2 = @isdefined(nonexistent)
    (result1, result2)
end

@testset "@isdefined basic" begin
    # Undefined variable - should be false
    @test !@isdefined(undefined_var)

    # Defined variable - should be true
    x = 42
    @test @isdefined(x)

    # After assignment - should be true
    y = 0
    @test @isdefined(y)
end

@testset "@isdefined in function" begin
    result = test_local_isdefined()
    @test result[1]   # a is defined
    @test !result[2]  # nonexistent is not defined
end

@testset "@isdefined with global" begin
    global_var = 100
    @test @isdefined(global_var)
end
end # module Agg_isdefined

# ===== source: macros/macro_call_in_do_block_9598.jl =====
module Agg_macro_call_in_do_block_9598
# Statement-position macros inside do-block closure bodies should lower with the active macro context.

using Test

setprecision(BigFloat, 64) do
    @test precision(BigFloat(1)) == 64
end

observed_precision = setprecision(BigFloat, 96) do
    @test precision(BigFloat(1)) == 96
    precision(BigFloat(1) + BigFloat(3))
end

@test observed_precision == 96
end # module Agg_macro_call_in_do_block_9598

# ===== source: macros/macro_expr_function_7634.jl =====
module Agg_macro_expr_function_7634
using Test

macro generated_function()
    esc(Expr(:function, Expr(:call, :foo7634, :x), Expr(:block, :(x + 2))))
end

@generated_function

@test foo7634(10) == 12
@test foo7634(-2) == 0
end # module Agg_macro_expr_function_7634

# ===== source: macros/macro_pair_call_7639.jl =====
module Agg_macro_pair_call_7639
using Test

macro macro_pair_call_7639()
    esc(:(Dict(:a => 1)))
end

@testset "macro-expanded Pair calls lower as Pair expressions (Issue #7639)" begin
    d = @macro_pair_call_7639()
    @test d[:a] == 1
end
end # module Agg_macro_pair_call_7639

# ===== source: macros/macro_spliced_typearg_binding_7835.jl =====
module Agg_macro_spliced_typearg_binding_7835
using Test

macro vector_of(T)
    esc(:(Vector{$T}))
end

GlobalElementType = Int64

function vector_from_local_type()
    LocalElementType = Float64
    @vector_of(LocalElementType)
end

@testset "macro-spliced parametric type arguments evaluate caller bindings" begin
    @test (@vector_of(GlobalElementType)) == Vector{Int64}
    @test vector_from_local_type() == Vector{Float64}
end
end # module Agg_macro_spliced_typearg_binding_7835

# ===== source: macros/macroexpand_show.jl =====
module Agg_macroexpand_show
# Test @macroexpand for @show macro
# Verifies that macro expansion produces correct structure

using Test

f(x) = x + 1

@testset "@macroexpand @show structure" begin
    # Get the expanded form of @show f(4)
    expanded = @macroexpand @show f(4)
    
    # The expansion should be an Expr
    @test typeof(expanded) == Expr
    
    # Convert to string for inspection
    expanded_str = string(expanded)
    
    # The expansion should contain the literal string "f(4)"
    # This verifies that string(ex) was evaluated at expansion time
    @test contains(expanded_str, "f(4)")
end
end # module Agg_macroexpand_show

# ===== source: macros/nospecialize_param_short_form_5122.jl =====
module Agg_nospecialize_param_short_form_5122
# Test @nospecialize / @specialize in argument position of short-form
# function definitions: f(@nospecialize(x)) = ... (Issue #5122).
#
# Upstream Julia accepts @nospecialize(x) (and @specialize(x)) as an argument
# annotation that suppresses type specialization while the parameter still binds
# the value with its declared type. SubsetJuliaVM has no JIT/specialization, so
# the annotation is a no-op that must simply pass the argument through.
#
# The full-form `function f(@nospecialize x) ... end` already worked; this
# fixture covers the short-form `f(@nospecialize(x)) = expr` path, including the
# type-annotated `@nospecialize(x::T)` form and a leading nospecialized argument
# followed by a typed one.

using Test

# Bare nospecialized argument, short form.
f_short_5122(@nospecialize(x)) = x + 1

# Nospecialized argument with a declared type.
g_short_5122(@nospecialize(x::Number)) = x * 2

# Leading nospecialized argument followed by an ordinary typed parameter.
h_short_5122(@nospecialize(x), y::Int) = (x, y)

# @specialize in argument position is also accepted (and is a no-op here).
k_short_5122(@specialize(x)) = x - 1

@testset "@nospecialize argument annotation (short form)" begin
    # Same definition is reused for different argument types without error
    # (no per-type re-specialization is observable; the value passes through).
    @test f_short_5122(2) == 3
    @test f_short_5122(2.5) == 3.5

    @test g_short_5122(4) == 8
    @test g_short_5122(4.0) == 8.0

    @test h_short_5122("a", 3) == ("a", 3)
    @test h_short_5122(1.0, 5) == (1.0, 5)

    @test k_short_5122(10) == 9
end
end # module Agg_nospecialize_param_short_form_5122

# ===== source: macros/operator_macro_definition_9744.jl =====
module Agg_operator_macro_definition_9744
using Test

macro >(exs...)
    return :(42)
end

@testset "operator macro definition (Issue #9744)" begin
    x = @> 1 2 3
    @test x == 42
end
end # module Agg_operator_macro_definition_9744

# ===== source: macros/sprintf_value_position_5683.jl =====
module Agg_sprintf_value_position_5683
using Test
using Printf
@testset "@sprintf in value position passes all arguments (Issue #5683)" begin
    @test @sprintf("%d", 42) == "42"
    @test @sprintf("%d-%d", 1, 2) == "1-2"
    @test @sprintf("%d,%d,%d", 1, 2, 3) == "1,2,3"
    @test @sprintf("%s", "hi") == "hi"
    @test @sprintf("%s and %s", "a", "b") == "a and b"
    @test @sprintf("hello") == "hello"
    # assigned and used
    s = @sprintf("%d", 7)
    @test s == "7"
    @test s * "!" == "7!"
    # nested in expressions
    @test length(@sprintf("%d%d", 1, 2)) == 2
    @test uppercase(@sprintf("%s", "x")) == "X"
end
end # module Agg_sprintf_value_position_5683

# ===== source: macros/test_base_macro_in_function.jl =====
module Agg_test_base_macro_in_function
# Prevention test: Base macros work inside function bodies (Issue #2604)
# The no-context lowering path (lower_macro_expr) must handle Base macros
# that the context path (lower_macro_expr_with_ctx) handles.
# If the paths diverge, macros will work at top-level but fail inside functions.

using Test

# @inbounds inside a function body (no-context path)
function sum_inbounds(arr)
    s = 0
    @inbounds for i in 1:length(arr)
        s = s + arr[i]
    end
    s
end

# @simd inside a function body (no-context path)
function sum_simd(arr)
    s = 0
    @simd for i in 1:length(arr)
        s = s + arr[i]
    end
    s
end

# @assert inside a function body (no-context path)
function checked_add(a, b)
    @assert a >= 0
    a + b
end

@testset "base macros in function bodies" begin
    @test sum_inbounds([1, 2, 3]) == 6
    @test sum_simd([1, 2, 3]) == 6
    @test checked_add(1, 2) == 3
end
end # module Agg_test_base_macro_in_function

# ===== source: macros/test_macro_in_let_7189.jl =====
module Agg_test_macro_in_let_7189
using Test

let a = 1
    @test a == 1
end

let a = 1, b = 2
    @test a + b == 3
end

@testset "let test macro context" begin
    let value = 4
        @test value == 4
    end
end
end # module Agg_test_macro_in_let_7189

true
