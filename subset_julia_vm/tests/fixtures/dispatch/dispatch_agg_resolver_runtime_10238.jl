# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: dispatch/call_dynamic_binary_both_resolver_3910.jl =====
module Agg_call_dynamic_binary_both_resolver_3910
using Test

struct BinaryBothBox3910
    value::Int64
end

Base.:+(box::BinaryBothBox3910, x::Any) = box.value + 1000
Base.:+(box::BinaryBothBox3910, x::Float64) = box.value + 100
Base.:+(box::BinaryBothBox3910, x::Int64) = box.value + x

function binary_both_any_dispatch_3910(box::Any, x::Any)
    return box + x
end

@testset "CallDynamicBinaryBoth shared resolver (Issue #3910)" begin
    box = BinaryBothBox3910(10)

    @test binary_both_any_dispatch_3910(box, 5) == 15
    @test binary_both_any_dispatch_3910(box, 2.5) == 110
    @test binary_both_any_dispatch_3910(box, "fallback") == 1010
end
end # module Agg_call_dynamic_binary_both_resolver_3910

# ===== source: dispatch/call_dynamic_binary_one_any_resolver_3910.jl =====
module Agg_call_dynamic_binary_one_any_resolver_3910
using Test

struct BinaryOneAnyBox3910
    value::Int64
end

Base.:+(box::BinaryOneAnyBox3910, x::Any) = box.value + 1000
Base.:+(box::BinaryOneAnyBox3910, x::Real) = box.value + 100
Base.:+(box::BinaryOneAnyBox3910, x::Int64) = box.value + x

function binary_one_any_dispatch_3910(box::BinaryOneAnyBox3910, x::Any)
    box + x
end

box = BinaryOneAnyBox3910(10)

@test binary_one_any_dispatch_3910(box, 5) == 15
@test binary_one_any_dispatch_3910(box, 2.5) == 110
@test binary_one_any_dispatch_3910(box, "fallback") == 1010
end # module Agg_call_dynamic_binary_one_any_resolver_3910

# ===== source: dispatch/call_dynamic_or_builtin_resolver_3910.jl =====
module Agg_call_dynamic_or_builtin_resolver_3910
using Test

function floor_dynamic_or_builtin_via_any_3910(x)
    y::Any = x
    floor(y)
end

function ceil_dynamic_or_builtin_via_any_3910(x)
    y::Any = x
    ceil(y)
end

@testset "CallDynamicOrBuiltin shared resolver (Issue #3910)" begin
    r = 7 // 3
    @test floor_dynamic_or_builtin_via_any_3910(r) == 2.0
    @test ceil_dynamic_or_builtin_via_any_3910(r) == 3.0

    @test floor_dynamic_or_builtin_via_any_3910(3.7) == 3.0
    @test ceil_dynamic_or_builtin_via_any_3910(3.2) == 4.0
end
end # module Agg_call_dynamic_or_builtin_resolver_3910

# ===== source: dispatch/call_dynamic_resolver_scoring_3910.jl =====
module Agg_call_dynamic_resolver_scoring_3910
using Test

struct CallDynamicBox3910{T}
    x::T
end

function call_dynamic_resolver_scoring_3910(x::Any)
    :any
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910)
    :bare
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910{T}) where {T}
    :parametric
end

function call_dynamic_resolver_scoring_3910(x::CallDynamicBox3910{Int64})
    :exact
end

function call_dynamic_resolver_scoring_3910_via_any(x)
    y::Any = x
    call_dynamic_resolver_scoring_3910(y)
end

struct CallDynamicBare3910
    x::Int64
end

function call_dynamic_resolver_bare_3910(x::Any)
    :any
end

function call_dynamic_resolver_bare_3910(x::CallDynamicBare3910)
    :bare
end

function call_dynamic_resolver_bare_via_any_3910(x)
    y::Any = x
    call_dynamic_resolver_bare_3910(y)
end

@testset "CallDynamic resolver scoring (Issue #3910)" begin
    @test call_dynamic_resolver_scoring_3910_via_any(CallDynamicBox3910{Int64}(1)) == :exact
    @test call_dynamic_resolver_scoring_3910_via_any(CallDynamicBox3910{Float64}(1.0)) == :parametric
    @test call_dynamic_resolver_bare_via_any_3910(CallDynamicBare3910(1)) == :bare
end
end # module Agg_call_dynamic_resolver_scoring_3910

# ===== source: dispatch/compile_runtime_dispatch_parity_6836.jl =====
module Agg_compile_runtime_dispatch_parity_6836
using Test

# Issue #6836: the compile-time dispatcher (statically-typed call sites,
# resolved to `CallResolved` / typed dispatch when argument types are known at
# compile time) and the runtime dispatcher (`Vm::find_best_method_index`, used
# when an argument's type is only known at run time — e.g. when it flows through
# an `Any` container) must select the SAME method for identical inputs. Both
# sides route through the shared `inference_core` selection core; this fixture
# pins that contract end to end.
#
# Each scenario calls a method once with a statically-typed argument (compile
# path) and once with the same value pulled from an `Any` container (runtime
# path). A divergence in method selection between the two paths would make a
# pair of results disagree and fail a `@test`.

kind(::Int64) = :int
kind(::Float64) = :float
kind(::String) = :string
kind(::Bool) = :bool
kind(::Number) = :number        # abstract-supertype fallback (e.g. Rational)

combine(::Int64, ::Int64) = :ii
combine(::Int64, ::Float64) = :if_
combine(::Number, ::Number) = :nn

elt(::Vector{Int64}) = :vint
elt(::Vector{Float64}) = :vfloat
elt(::AbstractVector) = :vabstract

@testset "single-arg concrete + abstract dispatch parity" begin
    box = Any[7, 3.5, "hi", true, 1//2]
    # static (typed literal) vs dynamic (Any-container element) must agree.
    @test kind(7) === kind(box[1]) === :int
    @test kind(3.5) === kind(box[2]) === :float
    @test kind("hi") === kind(box[3]) === :string
    @test kind(true) === kind(box[4]) === :bool
    @test kind(1//2) === kind(box[5]) === :number
end

@testset "multi-arg dispatch parity" begin
    box = Any[3, 4, 2.0, 1//1]
    @test combine(3, 4) === combine(box[1], box[2]) === :ii
    @test combine(3, 2.0) === combine(box[1], box[3]) === :if_
    @test combine(1//1, 1//1) === combine(box[4], box[4]) === :nn
end

@testset "parametric container dispatch parity" begin
    vi = Int64[1, 2]
    vf = Float64[1.0, 2.0]
    box = Any[vi, vf, 1:3]
    @test elt(vi) === elt(box[1]) === :vint
    @test elt(vf) === elt(box[2]) === :vfloat
    @test elt(1:3) === elt(box[3]) === :vabstract
end
end # module Agg_compile_runtime_dispatch_parity_6836

# ===== source: dispatch/direct_static_no_method_methoderror_6007.jl =====
module Agg_direct_static_no_method_methoderror_6007
using Test

h_direct_6007(x::String) = "got string: " * x

@testset "direct static method miss raises runtime MethodError (Issue #6007)" begin
    @test h_direct_6007("ok") == "got string: ok"
    @test_throws MethodError h_direct_6007(42)
end
end # module Agg_direct_static_no_method_methoderror_6007

# ===== source: dispatch/method_ambiguity_runtime_5071.jl =====
module Agg_method_ambiguity_runtime_5071
# Issue #5071: an ambiguous method call must raise a *catchable* runtime
# MethodError (matching upstream Julia), not abort compilation.
#
# Upstream Julia raises `MethodError: f(::Int64, ::Int64) is ambiguous`
# at runtime, which is catchable via try/catch and `@test_throws MethodError`.
# Previously sjulia raised a hard `CompileError::Dispatch(AmbiguousMethod{..})`
# that exited the process (exit code 1) and was NOT catchable.

using Test

f(x::Int, y::Number) = "Int,Number"
f(x::Number, y::Int) = "Number,Int"

@testset "ambiguous dispatch throws catchable MethodError" begin
    # The ambiguous call must throw a catchable MethodError at runtime.
    @test_throws MethodError f(1, 2)

    # And it must be catchable via try/catch (process does NOT abort).
    caught = false
    try
        f(1, 2)
    catch e
        caught = true
    end
    @test caught
end

# Adding a most-specific resolver method makes the call unambiguous again;
# this already worked and must keep working.
g(x::Int, y::Number) = "Int,Number"
g(x::Number, y::Int) = "Number,Int"
g(x::Int, y::Int) = "Int,Int"

@testset "resolver method disambiguates" begin
    @test g(1, 2) == "Int,Int"
    # Non-ambiguous calls still pick the unique best method.
    @test g(1, 2.0) == "Int,Number"
    @test g(1.0, 2) == "Number,Int"
end
end # module Agg_method_ambiguity_runtime_5071

# ===== source: dispatch/runtime_bounded_dispatch_from_any_6202.jl =====
module Agg_runtime_bounded_dispatch_from_any_6202
using Test

# Issue #6202 (part of #5926 / #5072): runtime dispatch from an imprecise
# container element must still rank bounded `where` methods by their actual
# runtime value type. The static path already picked the tighter bound; the
# `Any` container path previously fell back to the looser method.

type_bound(::Type{T}) where {T<:Real} = :real
type_bound(::Type{T}) where {T<:Integer} = :integer

vector_bound(::Vector{T}) where {T<:Real} = :real
vector_bound(::Vector{T}) where {T<:Integer} = :integer

@testset "bounded Type{T} dispatch from Any container" begin
    xs = Any[Int64, Float64]
    @test type_bound(Int64) === :integer
    @test type_bound(Float64) === :real
    @test type_bound(xs[1]) === :integer
    @test type_bound(xs[2]) === :real
end

@testset "bounded Vector{T} dispatch from Any container" begin
    xs = Any[[1, 2], Float64[1.0, 2.0]]
    @test vector_bound([1, 2]) === :integer
    @test vector_bound(Float64[1.0, 2.0]) === :real
    @test vector_bound(xs[1]) === :integer
    @test vector_bound(xs[2]) === :real
end
end # module Agg_runtime_bounded_dispatch_from_any_6202

true
