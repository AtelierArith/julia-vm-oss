# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: dispatch/assignform_operator_where_bounds_6537.jl =====
module Agg_assignform_operator_where_bounds_6537
# Issue #6537: an operator method defined in ASSIGNMENT form with a braced
# `where` clause lost its typevar bound: `*(a::Wrap{T}, b::Wrap{T}) where
# {T<:Real} = ...` lowered as if it were `where {T}`. The function-form
# operator path and the assignment-form non-operator path both kept the bound.
#
# Root cause: `lower_operator_method` (lowering/function/short_form.rs) had a
# hand-rolled where-clause loop that did not recognize the
# BinaryExpression/SubtypeConstraint shapes the pure parser emits for braced
# bounds, so bounded entries were silently dropped from `type_params`. The fix
# routes both the long form and the assignment-form operator path through the
# shared `parse_where_clause_type_params` helper (and converts param
# annotations to TypeVars like the non-operator path does).

using Test

# Separate import lines: `import Base: *, ==, +` fails to parse when a
# comparison operator appears mid-list (Issue #6544).
import Base: *
import Base: ==
import Base: +

struct Wrap6537{T}
    x::T
end

# Assignment-form operator methods (the buggy path).
*(a::Wrap6537{T}, b::Wrap6537{T}) where {T<:Real} = "wrap-real"
*(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = "wrap-generic"

# Multi-typevar braces with bounds on both.
+(a::Wrap6537{T}, b::Wrap6537{S}) where {T<:Real,S<:Real} = "plus-real"
+(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = "plus-generic"

# `==` spelling.
==(a::Wrap6537{T}, b::Wrap6537{T}) where {T<:Real} = true
==(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = false

# Function-form control: same methods, long-form spelling (already worked).
struct WrapCtl6537{T}
    x::T
end
function Base.:*(a::WrapCtl6537{T}, b::WrapCtl6537{T}) where {T<:Real}
    return "ctl-real"
end
function Base.:*(a::WrapCtl6537{T}, b::WrapCtl6537{S}) where {T,S}
    return "ctl-generic"
end

@testset "assignment-form operator keeps braced where bounds (Issue #6537)" begin
    # Runtime dispatch via Any[] so the bound must be enforced at run time.
    wf = Any[Wrap6537("a"), Wrap6537("b")]
    @test wf[1] * wf[2] == "wrap-generic"
    wr = Any[Wrap6537(1), Wrap6537(2)]
    @test wr[1] * wr[2] == "wrap-real"

    # Compile-time (typed) dispatch too.
    @test Wrap6537("a") * Wrap6537("b") == "wrap-generic"
    @test Wrap6537(1) * Wrap6537(2) == "wrap-real"

    # Multi-typevar bounds: both must hold for the bounded method to apply.
    @test Wrap6537(1) + Wrap6537(2.5) == "plus-real"
    @test Wrap6537(1) + Wrap6537("s") == "plus-generic"
    @test Wrap6537("s") + Wrap6537("t") == "plus-generic"

    # `==` spelling.
    @test (Wrap6537(1) == Wrap6537(2)) == true
    @test (Wrap6537("a") == Wrap6537("b")) == false
end

@testset "function-form operator control (Issue #6537)" begin
    cf = Any[WrapCtl6537("a"), WrapCtl6537("b")]
    @test cf[1] * cf[2] == "ctl-generic"
    cr = Any[WrapCtl6537(1), WrapCtl6537(2)]
    @test cr[1] * cr[2] == "ctl-real"
end

# Unbounded braces must stay unbounded (no invented bound).
struct Box6537{T}
    v::T
end
*(a::Box6537{T}, b::Box6537{S}) where {T,S} = "box-any"
@testset "unbounded braced where still matches everything (Issue #6537)" begin
    @test Box6537("x") * Box6537(1) == "box-any"
end

# The UNBRACED form previously failed to parse entirely (`expected Eq`): the
# where-clause constraint was parsed with the general expression parser, which
# swallowed `= body` as an Assignment (Issue #6537).
struct UWrap6537{T}
    x::T
end
*(a::UWrap6537{T}, b::UWrap6537{T}) where T<:Real = "uw-real"
*(a::UWrap6537{T}, b::UWrap6537{S}) where {T,S} = "uw-generic"

@testset "unbraced where on assignment-form operator (Issue #6537)" begin
    @test UWrap6537(1) * UWrap6537(2) == "uw-real"
    @test UWrap6537("a") * UWrap6537("b") == "uw-generic"
    uw = Any[UWrap6537("a"), UWrap6537("b")]
    @test uw[1] * uw[2] == "uw-generic"
end
end # module Agg_assignform_operator_where_bounds_6537

# ===== source: dispatch/bounded_typevar_outranks_untyped_5375.jl =====
module Agg_bounded_typevar_outranks_untyped_5375
# Issue #5375: a value-position bounded type-variable method
# `f(x::T) where {T<:Number}` must be ranked MORE specific than an untyped
# fallback `f(x)`, so `f(5)` selects the bounded method regardless of definition
# order.
#
# Root cause: `CoreType::specificity()` scored every type variable as 0 (the
# upper bound was ignored), while an untyped `Any` parameter earned the
# `type_reuse_bonus` in `score_julia_signature_with_binding_count`. The untyped
# fallback therefore out-scored the bounded method (1 vs 0). The fix scores a
# bounded `T<:B` from its bound `B` so it stays as specific as a concrete `B`
# and strictly above `Any`. Reproduces with both the short and long forms (the
# bound is lowered correctly for both), so this is a specificity bug, distinct
# from the long-form bound-drop fixed in #5374.

using Test

# Bounded only (no fallback): already worked, kept as a control.
g(x::T) where {T<:Number} = :g_num

# Bounded defined first, untyped fallback second.
h(x::T) where {T<:Number} = :h_num
h(x) = :h_any

# Untyped fallback first, bounded second (definition order must not matter).
k(x) = :k_any
k(x::T) where {T<:Number} = :k_num

# Long-form spelling of the same competition.
function classify(x::T) where {T<:Number}
    return :num
end
classify(x) = :nonnum

# Short-form spelling.
sclassify(x::T) where {T<:Number} = :num
sclassify(x) = :nonnum

@testset "bounded typevar method outranks untyped fallback (Issue #5375)" begin
    @test g(5) == :g_num
    @test h(5) == :h_num
    @test k(5) == :k_num
    @test classify(5) == :num
    @test sclassify(5) == :num

    # The untyped fallback still wins for non-Number arguments.
    @test h("x") == :h_any
    @test k("x") == :k_any
    @test classify("x") == :nonnum
    @test classify(:sym) == :nonnum
end

# A tighter bound must win over a looser bound for an argument that satisfies
# both: Integer ⊂ Real ⊂ Number.
rank(x::T) where {T<:Number} = :number
rank(x::T) where {T<:Real} = :real
rank(x::T) where {T<:Integer} = :integer

@testset "tighter bound outranks looser bound (Issue #5375)" begin
    @test rank(5) == :integer
    @test rank(2.5) == :real
    @test rank(1 + 2im) == :number
end

# A concrete parameter must still win over a bounded type variable that only
# constrains the argument abstractly: Int64 is strictly more specific than
# `T<:Number` for an Int64 argument.
pick(x::T) where {T<:Number} = :bounded
pick(x::Int64) = :concrete

@testset "concrete parameter still outranks bounded typevar (Issue #5375)" begin
    @test pick(5) == :concrete
    @test pick(2.5) == :bounded
end

# Runtime dispatch: an `Any`-typed element forces method selection at run time
# rather than compile time, exercising the runtime resolver path too.
@testset "bounded typevar wins under runtime dispatch (Issue #5375)" begin
    vals = Any[5, "x", 2.5]
    @test classify(vals[1]) == :num
    @test classify(vals[2]) == :nonnum
    @test classify(vals[3]) == :num
end
end # module Agg_bounded_typevar_outranks_untyped_5375

# ===== source: dispatch/callable_value_where_bound_test_inline_6539.jl =====
module Agg_callable_value_where_bound_test_inline_6539
# Issue #6539: where-clause bounds must be enforced on every evaluation
# channel that an expression inlined into `@test` (or a callable value bound
# to a variable) can take, matching upstream Julia:
#
# 1. The callable-value channel (`f = abs; f(x)` via
#    resolve_callable_value_candidates) ignored `where` bounds, so
#    `abs(h::Holder{T}) where {T<:Real}` matched `Holder{String}`.
# 2. `abs(hs[2]) == "..."` inlined into `@test` (or any expression position)
#    constant-folded to `false` at compile time: return-type inference
#    assumed `abs(::Any)::Float64`, and the String-vs-non-String equality
#    shortcut folded the comparison.
# 3. `(a == b) == "..."` with a user `==` returning a non-Bool mis-folded the
#    same way (equality result inference assumed Bool unconditionally).
#
# Verified against julia 1.12 (all tests pass upstream).

using Test

import Base: abs
import Base: ==

struct Holder6539{T}
    v::T
end

function abs(h::Holder6539{T}) where {T<:Real}
    return "holder-real"
end
abs(h::Holder6539) = "holder-any"

hs6539 = Any[Holder6539(3), Holder6539("s")]

@testset "inline @test call enforces where bound (Issue #6539)" begin
    # Variable-bound control (CallDynamic channel, fixed by #6536/#6543).
    r = abs(hs6539[2])
    @test r == "holder-any"
    # Inline form: previously constant-folded to `false` at compile time.
    @test abs(hs6539[2]) == "holder-any"
    @test abs(hs6539[1]) == "holder-real"
    # Inline comparison outside @test as well.
    @test (abs(hs6539[2]) == "holder-any") == true
end

@testset "callable-value channel enforces where bound (Issue #6539)" begin
    f = abs
    # `f(...)` routes through resolve_callable_value_candidates: the bounded
    # holder-real method must be rejected for Holder6539{String}.
    @test f(hs6539[2]) == "holder-any"
    @test f(hs6539[1]) == "holder-real"
end

struct Box6539{T}
    v::T
end

==(a::Box6539, b::Box6539) = "box-any"

bb6539 = Any[Box6539(1), Box6539(2)]

@testset "nested comparison with user non-Bool == (Issue #6539)" begin
    # Variable-bound control.
    r = bb6539[1] == bb6539[2]
    @test r == "box-any"
    # Inline nested form: previously constant-folded to `false` because the
    # inner `==` was unconditionally inferred Bool.
    @test (bb6539[1] == bb6539[2]) == "box-any"
end
end # module Agg_callable_value_where_bound_test_inline_6539

# ===== source: dispatch/invariant_vector_element_dispatch_8806.jl =====
module Agg_invariant_vector_element_dispatch_8806
using Test

vector_number_8806(x::Vector{Number}) = :vecnum
vector_number_8806(x) = :any

abstract_vector_number_8806(x::AbstractVector{Number}) = :absvecnum
abstract_vector_number_8806(x) = :any

bounded_vector_8806(x::Vector{<:Number}) = :covvec
bounded_vector_8806(x) = :any

bounded_abstract_vector_8806(x::AbstractVector{<:Number}) = :covabsvec
bounded_abstract_vector_8806(x) = :any

nested_plain_element_8806(x::Vector{Complex{Real}}) = :complexreal
nested_plain_element_8806(x) = :any

vector_where_8806(x::Vector{T}) where {T<:Number} = T
vector_where_8806(x) = Nothing

nested_where_8806(x::Vector{Complex{T}}) where {T<:Real} = T
nested_where_8806(x) = Nothing

vector_number_any_route_8806(x::Any) = vector_number_8806(x)
nested_plain_any_route_8806(x::Any) = nested_plain_element_8806(x)

@testset "invariant vector element dispatch (Issue #8806)" begin
    ints = [1, 2]
    complexes = [1 + 2im]

    @test vector_number_8806(ints) == :any
    @test vector_number_any_route_8806(ints) == :any
    @test abstract_vector_number_8806(ints) == :any

    @test bounded_vector_8806(ints) == :covvec
    @test bounded_abstract_vector_8806(ints) == :covabsvec
    @test vector_where_8806(ints) == Int64

    @test nested_plain_element_8806(complexes) == :any
    @test nested_plain_any_route_8806(complexes) == :any
    @test nested_where_8806(complexes) == Int64

    @test !(Tuple{Vector{Int64}} <: Tuple{Vector{Number}})
end
end # module Agg_invariant_vector_element_dispatch_8806

# ===== source: dispatch/nested_parametric_slot_where_bind_8853.jl =====
module Agg_nested_parametric_slot_where_bind_8853
using Test

# Regression test for Issue #8853:
# A where-clause type variable nested inside a parametric slot (e.g. T in
# Box{Wrap{T}}) was not bound in the method body because extract_type_bindings
# in comparison.rs used parametric_slot_matches (bool only, discards bindings)
# instead of recursing into extract_type_bindings for nested patterns.
# After fix: recursive extraction propagates bindings from nested slots.

struct Box{T}
    value::T
end

struct Wrap{T}
    inner::T
end

# T is nested inside Wrap{T} inside Box{Wrap{T}}
function unwrap_box(b::Box{Wrap{T}}) where {T}
    return b.value.inner
end

w = Box(Wrap(42))
@test unwrap_box(w) == 42

w2 = Box(Wrap("hello"))
@test unwrap_box(w2) == "hello"

# Two-level nesting
struct Triple{T}
    v::T
end

function deep(b::Box{Wrap{Triple{T}}}) where {T}
    return b.value.inner.v
end

@test deep(Box(Wrap(Triple(99)))) == 99
@test deep(Box(Wrap(Triple(3.14)))) == 3.14
end # module Agg_nested_parametric_slot_where_bind_8853

# ===== source: dispatch/parametric_array_dispatch.jl =====
module Agg_parametric_array_dispatch
# Test parametric array type dispatch (Issue #1237)
# Tests that Vector{Int64} and Vector{Float64} are properly distinguished
# for multiple dispatch purposes.

using Test

# Define functions with parametric array types
function test_dispatch(a::Vector{Int64})
    return "Int64"
end

function test_dispatch(a::Vector{Float64})
    return "Float64"
end

@testset "Parametric array type dispatch" begin
    # Test 1: collect from range should produce Vector{Int64}
    a = collect(1:5)
    @test typeof(a) == Vector{Int64}

    # Test 2: dispatch should work correctly for Vector{Int64}
    result = test_dispatch(a)
    @test result == "Int64"

    # Test 3: Float64 array dispatch
    b = [1.0, 2.0, 3.0]
    @test test_dispatch(b) == "Float64"

    # Test 4: literal Int64 array dispatch
    c = [1, 2, 3]
    @test test_dispatch(c) == "Int64"
end
end # module Agg_parametric_array_dispatch

# ===== source: dispatch/parametric_invariance_dispatch_8849.jl =====
module Agg_parametric_invariance_dispatch_8849
using Test

struct Box8849{T}
    value::T
end

struct Wrap8849{T}
    value::T
end

box_plain_8849(x::Box8849{Number}) = :boxnum
box_plain_8849(x) = :any

box_any_plain_8849(x::Box8849{Any}) = :boxany
box_any_plain_8849(x) = :any

box_bounded_8849(x::Box8849{<:Number}) = :covbox
box_bounded_8849(x) = :any

box_where_8849(x::Box8849{T}) where {T<:Number} = T
box_where_8849(x) = Nothing

nested_plain_8849(x::Box8849{Wrap8849{Number}}) = :nestednum
nested_plain_8849(x) = :any

complex_plain_8849(x::Complex{Real}) = :complexreal
complex_plain_8849(x) = :any

complex_bounded_8849(x::Complex{<:Real}) = :covcomplex
complex_bounded_8849(x) = :any

complex_where_8849(x::Complex{T}) where {T<:Real} = T
complex_where_8849(x) = Nothing

box_plain_any_route_8849(x::Any) = box_plain_8849(x)
nested_plain_any_route_8849(x::Any) = nested_plain_8849(x)
complex_plain_any_route_8849(x::Any) = complex_plain_8849(x)

@testset "parametric invariance dispatch (Issue #8849)" begin
    box_int = Box8849(1)
    nested_int = Box8849(Wrap8849(1))
    complex_int = 1 + 2im

    @test box_plain_8849(box_int) == :any
    @test box_plain_any_route_8849(box_int) == :any
    @test box_any_plain_8849(box_int) == :any
    @test box_bounded_8849(box_int) == :covbox
    @test box_where_8849(box_int) == Int64

    @test nested_plain_8849(nested_int) == :any
    @test nested_plain_any_route_8849(nested_int) == :any

    @test complex_plain_8849(complex_int) == :any
    @test complex_plain_any_route_8849(complex_int) == :any
    @test complex_bounded_8849(complex_int) == :covcomplex
    @test complex_where_8849(complex_int) == Int64

    @test !(Box8849{Int64} <: Box8849{Number})
    @test !(Box8849{Int64} <: Box8849{Any})
    @test !(Complex{Int64} <: Complex{Real})
end
end # module Agg_parametric_invariance_dispatch_8849

# ===== source: dispatch/symbol_type_param_dispatch.jl =====
module Agg_symbol_type_param_dispatch
# Test symbol type parameters in parametric structs and Float64 dispatch (Issue #633)
# Float64() should dispatch to user-defined methods for struct types with symbol type parameters

using Test

# Test: Custom type with symbol type parameter (using AbstractIrrational from Base)
struct MyIrrational{sym} <: AbstractIrrational end
Float64(::MyIrrational{:tau}) = 6.283185307179586  # 2*pi
Float64(::MyIrrational{:sqrt2}) = 1.4142135623730951

const tau_val = MyIrrational{:tau}()
const sqrt2_val = MyIrrational{:sqrt2}()

# Test direct calls work
@test Float64(tau_val) == 6.283185307179586
@test Float64(sqrt2_val) == 1.4142135623730951

# Test calls through wrapper function with Any-typed parameter
function convert_to_float(x)
    return Float64(x)
end

@test convert_to_float(tau_val) == 6.283185307179586
@test convert_to_float(sqrt2_val) == 1.4142135623730951

# Test builtin Float64 still works for numeric types
@test Float64(42) == 42.0
@test isapprox(Float64(3.14f0), 3.14; atol=1e-6)

# Test Base pi constant (Float64)
@test Float64(pi) == 3.141592653589793
@test 2.0 * pi == 6.283185307179586

# Return true to indicate success
end # module Agg_symbol_type_param_dispatch

# ===== source: dispatch/test_parametric_abstract_dispatch.jl =====
module Agg_test_parametric_abstract_dispatch
# Test parametric abstract type dispatch (Issue #2523)
# abstract type Container{T} end should preserve type params at runtime

using Test

# Parametric abstract type hierarchy
abstract type Container{T} end

# Non-parametric structs with parametric abstract parent
struct IntBox <: Container{Int64}
    value::Int64
end

struct FloatBox <: Container{Float64}
    value::Float64
end

# Dispatch on parametric abstract type
describe(::Container{Int64}) = "int container"
describe(::Container{Float64}) = "float container"

# Dispatch on base abstract type (no params)
is_container(::Container) = true

@testset "Parametric abstract type dispatch" begin
    b1 = IntBox(42)
    b2 = FloatBox(3.14)

    # Test 1: Struct construction works
    @test b1.value == 42
    @test b2.value == 3.14

    # Test 2: Dispatch on parametric abstract types
    @test describe(b1) == "int container"
    @test describe(b2) == "float container"

    # Test 3: Dispatch on base abstract type
    @test is_container(b1) == true
    @test is_container(b2) == true

    # Test 4: Subtype relationships
    @test IntBox <: Container{Int64}
    @test FloatBox <: Container{Float64}
    @test IntBox <: Container
    @test FloatBox <: Container
end
end # module Agg_test_parametric_abstract_dispatch

# ===== source: dispatch/test_vararg_typed_dispatch.jl =====
module Agg_test_vararg_typed_dispatch
# Test Vararg{T} and Vararg{T,N} dispatch (Issue #2525)
using Test

# Basic Vararg{T} — equivalent to args::T...
function sum_ints(args::Vararg{Int64})
    s = 0
    for x in args
        s = s + x
    end
    s
end

# Vararg{T,N} — fixed count varargs
function pair(a::Vararg{Int64, 2})
    a[1] + a[2]
end

# Dispatch between specific-count and any-count varargs
function vfunc(x::Vararg{Int64, 1})
    "one"
end

function vfunc(x::Int64, y::Int64)
    "two"
end

@testset "Vararg{T} and Vararg{T,N} dispatch" begin
    # Vararg{Int64} collects any number of Int64 args
    @test sum_ints(1, 2, 3) == 6
    @test sum_ints(10, 20) == 30
    @test sum_ints(5) == 5

    # Vararg{Int64, 2} requires exactly 2 Int64 args
    @test pair(3, 4) == 7
    @test pair(10, 20) == 30

    # vfunc(x::Vararg{Int64, 1}) matches 1 arg, vfunc(x, y) matches 2 args
    @test vfunc(42) == "one"
    @test vfunc(1, 2) == "two"
end
end # module Agg_test_vararg_typed_dispatch

# ===== source: dispatch/type_parametric_singleton_dispatch.jl =====
module Agg_type_parametric_singleton_dispatch
# Parametric type expressions dispatch as their Type{T} singleton.
# Mirrors Julia's selection of the Type{Complex{Float64}} method over
# the generic Type{T} fallback, while keeping ordinary Complex values separate
# from type objects (Issues #4039/#4044).

using Test

function dispatch_parametric_type_probe(::Type{Complex{Float64}}, d1)
    99
end

function dispatch_parametric_type_probe(::Type{Int64}, d1)
    33
end

function dispatch_parametric_type_probe(::Type{T}, d1) where T
    11
end

function dispatch_parametric_type_probe_from_var(T)
    dispatch_parametric_type_probe(T, 2)
end

function dispatch_type_or_value_probe(::Type{Complex{Float64}})
    1
end

function dispatch_type_or_value_probe(x::Complex{Float64})
    2
end

@testset "parametric Type singleton dispatch (Issues #4039/#4044)" begin
    @test dispatch_parametric_type_probe(Complex{Float64}, 2) == 99
    @test dispatch_parametric_type_probe(Int64, 2) == 33
    @test dispatch_parametric_type_probe(Float64, 2) == 11
    @test dispatch_parametric_type_probe_from_var(Complex{Float64}) == 99
    @test dispatch_type_or_value_probe(Complex{Float64}) == 1
    @test dispatch_type_or_value_probe(Complex{Float64}(3.0, 4.0)) == 2
end
end # module Agg_type_parametric_singleton_dispatch

# ===== source: dispatch/typed_varargs_diagonal_8565.jl =====
module Agg_typed_varargs_diagonal_8565
# Issue #8565: typed diagonal varargs must outrank the untyped varargs fallback
# for homogeneous argument tuples, including the empty tuple.

using Test

diag8565(xs::T...) where {T} = "diag"
diag8565(xs...) = "any"
diag8565(x::Int, y::Int) = "pair"

function diagfull8565(xs::T...) where {T}
    "diag"
end
diagfull8565(xs...) = "any"

unusedwhere8565(xs...) where {T} = "where"
unusedwhere8565(xs...) = "plain"

@testset "typed varargs diagonal specificity (Issue #8565)" begin
    @test diag8565(1, 2) == "pair"
    @test diag8565(1, 2, 3) == "diag"
    @test diag8565(1.0, 2.0) == "diag"
    @test diag8565(1) == "diag"
    @test diag8565() == "diag"
    @test diag8565(1, 2.0, 3) == "any"

    @test diagfull8565(1, 2) == "diag"
    @test diagfull8565() == "diag"
    @test unusedwhere8565(1, 2) == "plain"
end
end # module Agg_typed_varargs_diagonal_8565

# ===== source: dispatch/typeobject_where_bound_parametric_9839.jl =====
module Agg_typeobject_where_bound_parametric_9839
using Test

struct TypeObjectBoundQ9839{T}
    v::T
end

bound_typeobject_9839(::Type{T}) where {T<:AbstractFloat} = :float
bound_typeobject_9839(::Type{T}) where {T<:Integer} = :int

bound_typeobject_fallback_9839(::Type{T}) where {T<:AbstractFloat} = :float
bound_typeobject_fallback_9839(::Type{T}) where {T<:Integer} = :int
bound_typeobject_fallback_9839(::Type{T}) where {T} = :any

bound_typeobject_uses_t_9839(::Type{T}) where {T<:AbstractFloat} = T

@testset "Type{T} where-bound rejects excluded parametric type objects (Issue #9839)" begin
    @test_throws MethodError bound_typeobject_9839(TypeObjectBoundQ9839{Int64})
    @test_throws MethodError bound_typeobject_uses_t_9839(Rational{Int64})

    @test bound_typeobject_fallback_9839(TypeObjectBoundQ9839{Int64}) === :any
    @test bound_typeobject_9839(Float16) === :float
    @test bound_typeobject_9839(Int32) === :int
end
end # module Agg_typeobject_where_bound_parametric_9839

# ===== source: dispatch/val_symbol_dispatch_5291.jl =====
module Agg_val_symbol_dispatch_5291
# Val{:sym} multiple dispatch must route by the symbol value parameter (Issue #5291)
#
# f(::Val{:up}) / f(::Val{:down}) previously always dispatched to the first method
# because the runtime type of Val(:up) rendered as Val{Symbol("up")} while the
# method parameter Val{:up} used the colon spelling, so isa/dispatch never matched.

using Test

f(::Val{:up}) = "went up"
f(::Val{:down}) = "went down"

@testset "Val{:sym} multiple dispatch (Issue #5291)" begin
    @test f(Val(:up)) == "went up"
    @test f(Val(:down)) == "went down"

    # isa against a symbol value parameter
    @test Val(:up) isa Val{:up}
    @test !(Val(:up) isa Val{:down})

    # typeof renders an identifier-symbol parameter in colon form (matching upstream)
    @test string(typeof(Val(:up))) == "Val{:up}"
    @test string(typeof(Val(:func!))) == "Val{:func!}"

    # a non-identifier symbol keeps the Symbol("...") spelling
    @test string(typeof(Val(Symbol("a b")))) == "Val{Symbol(\"a b\")}"
end
end # module Agg_val_symbol_dispatch_5291

# ===== source: dispatch/vector_any_erased_element_invariance_8848.jl =====
module Agg_vector_any_erased_element_invariance_8848
using Test

# Regression test for Issue #8848:
# Vector{Any} was treated as an erased-element catch-all so
# Vector{String} <: Vector{Any} was incorrectly true in dispatch.
# After fix: arrays follow invariant element-slot semantics (like upstream Julia).

# Basic invariance: typed vectors should NOT match Vector{Any} methods
function process(arr::Vector{String})
    return "String vector"
end
function process(arr::Vector{Any})
    return "Any vector"
end

strs = ["hello", "world"]
@test process(strs) == "String vector"

anys = Any["hello", 42]
@test process(anys) == "Any vector"

# Subtype check at the type level
@test !(Vector{String} <: Vector{Any})
@test !(Vector{Int64} <: Vector{Any})
@test (Vector{Any} <: Vector{Any})

# Base set functions: union, intersect on typed vectors should work without
# falling into any erased-element catch-all
a = [1, 2, 3]
b = [2, 3, 4]
@test union(a, b) == [1, 2, 3, 4]
@test intersect(a, b) == [2, 3]
@test setdiff(a, b) == [1]
@test symdiff(a, b) == [1, 4]
@test unique([1, 2, 1, 3]) == [1, 2, 3]

# String vectors still work through AbstractVector fallbacks
sa = ["a", "b", "c"]
sb = ["b", "c", "d"]
@test union(sa, sb) == ["a", "b", "c", "d"]
@test intersect(sa, sb) == ["b", "c"]
end # module Agg_vector_any_erased_element_invariance_8848

# ===== source: dispatch/where_context_dispatch.jl =====
module Agg_where_context_dispatch
# Test type inference for function dispatch inside where T context
# Issue #2556: TypeVar upper bounds should be used for compile-time dispatch

using Test

# Integer constraint - div should use integer division
function safe_div(x::T, y::T) where {T<:Integer}
    return div(x, y)
end

function safe_rem(x::T, y::T) where {T<:Integer}
    return rem(x, y)
end

function safe_mod(x::T, y::T) where {T<:Integer}
    return mod(x, y)
end

function safe_gcd(x::T, y::T) where {T<:Integer}
    return gcd(x, y)
end

# Real constraint - arithmetic should work
function safe_add(x::T, y::T) where {T<:Real}
    return x + y
end

# Unconstrained TypeVar - runtime dispatch
function identity_op(x::T) where T
    return x
end

@testset "Where context dispatch (Issue #2556)" begin
    @testset "Integer-bounded div dispatch" begin
        @test safe_div(10, 3) == 3
        @test safe_div(7, 2) == 3
        @test typeof(safe_div(10, 3)) == Int64
        # Issue #5398: a `where {T<:Integer}` method calling `div` must
        # runtime-dispatch to the concrete integer method rather than
        # statically binding the generic `floor(x / y)` fallback (Float64).
        # Concrete integer types must be preserved end-to-end.
        @test safe_div(Int32(10), Int32(3)) == 3
        @test typeof(safe_div(Int32(10), Int32(3))) == Int32
        @test safe_div(big(10), big(3)) == 3
        @test typeof(safe_div(big(10), big(3))) == BigInt
    end

    @testset "Integer-bounded builtin dispatch matrix" begin
        @test safe_rem(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_rem(Int32(10), Int32(3))) == Int32
        @test safe_rem(big(10), big(3)) == 1
        @test typeof(safe_rem(big(10), big(3))) == BigInt

        @test safe_mod(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_mod(Int32(10), Int32(3))) == Int32
        @test safe_mod(big(10), big(3)) == 1
        @test typeof(safe_mod(big(10), big(3))) == BigInt

        @test safe_gcd(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_gcd(Int32(10), Int32(3))) == Int32
        @test safe_gcd(big(10), big(3)) == 1
        @test typeof(safe_gcd(big(10), big(3))) == BigInt
    end

    @testset "Real-bounded addition" begin
        @test safe_add(1.5, 2.5) == 4.0
        @test safe_add(1, 2) == 3
    end

    @testset "Unconstrained TypeVar" begin
        @test identity_op(42) == 42
        @test identity_op("hello") == "hello"
    end
end
end # module Agg_where_context_dispatch

true
