# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: dispatch/abstract_multi_value_param_dispatch_7960.jl =====
module Agg_abstract_multi_value_param_dispatch_7960
using Test

# Issue #7960: method dispatch on an ABSTRACT type parameterized by integer
# VALUE parameters, called through a CONCRETE subtype, previously dropped every
# parameter to the bare family name (`AbsM{2,2,T}` -> `AbsM`). All value-parameter
# specializations therefore collapsed into one indistinguishable signature, so
# the last-defined one always won regardless of the actual values: `h(ConM{2,2})`
# wrongly selected the `AbsM{3,3,T}` method. The concrete subtype's value
# parameters are now projected up to the abstract supertype's instantiation
# (`ConM{2,2,Float64}` -> `AbsM{2,2,Float64}`) and compared, so the correct
# specialization is selected and the parametric method outranks the generic one.

abstract type AbsM{M,N,T} end
struct ConM{M,N,T} <: AbsM{M,N,T}
    data::Tuple
end

h(x::AbsM) = "generic"
h(x::AbsM{2,2,T}) where {T} = "spec-2x2"
h(x::AbsM{3,3,T}) where {T} = "spec-3x3"

@testset "concrete subtype selects the matching value-parameter specialization" begin
    @test h(ConM{2,2,Float64}((1.0,))) == "spec-2x2"
    @test h(ConM{3,3,Float64}((1.0,))) == "spec-3x3"
    # A size with no specialization falls back to the generic method.
    @test h(ConM{4,4,Float64}((1.0,))) == "generic"
end

# The specialization must outrank the generic method regardless of definition
# order (the fix ranks `AbsM{2,2,T}` strictly above the bare `AbsM`, rather than
# relying on "last defined wins").
abstract type AbsR{M,N,T} end
struct ConR{M,N,T} <: AbsR{M,N,T}
    data::Tuple
end

r(x::AbsR{2,2,T}) where {T} = "r-2x2"
r(x::AbsR{3,3,T}) where {T} = "r-3x3"
r(x::AbsR) = "r-generic"

@testset "specialization outranks the generic even when defined first" begin
    @test r(ConR{2,2,Float64}((1.0,))) == "r-2x2"
    @test r(ConR{3,3,Float64}((1.0,))) == "r-3x3"
    @test r(ConR{4,4,Float64}((1.0,))) == "r-generic"
end

# A single integer value parameter on the abstract supertype dispatches just as
# correctly as the multi-parameter case.
abstract type AbsV{N,T} end
struct ConV{N,T} <: AbsV{N,T}
    data::Tuple
end

g(x::AbsV) = "g-generic"
g(x::AbsV{2,T}) where {T} = "g-2"
g(x::AbsV{3,T}) where {T} = "g-3"

@testset "single value parameter on abstract supertype" begin
    @test g(ConV{2,Int}((1,))) == "g-2"
    @test g(ConV{3,Int}((1,))) == "g-3"
end
end # module Agg_abstract_multi_value_param_dispatch_7960

# ===== source: dispatch/abstract_type_dispatch.jl =====
module Agg_abstract_type_dispatch
# Test method dispatch with abstract type parameters (Issue #636)
# Methods with abstract type parameters should work regardless of definition order

using Test

# Test case 1: Method defined AFTER struct types
abstract type AbstractIrrational <: Real end
struct IrrationalPi <: AbstractIrrational end
struct IrrationalE <: AbstractIrrational end

# Method using abstract type parameter - defined after struct
to_float(x::AbstractIrrational) = 3.14
@test to_float(IrrationalPi()) == 3.14
@test to_float(IrrationalE()) == 3.14

# Test case 2: Multiple methods with different abstract types
abstract type Animal end
abstract type Pet <: Animal end
struct Dog <: Pet end
struct Cat <: Pet end

speak(::Animal) = "?"
speak(::Pet) = "hello"
speak(::Dog) = "woof"
speak(::Cat) = "meow"

# Most specific method should be selected
@test speak(Dog()) == "woof"
@test speak(Cat()) == "meow"

# Test case 3: Hierarchy with Real
abstract type MyNumber <: Real end
struct MyInt <: MyNumber end

double(x::MyNumber) = 2
@test double(MyInt()) == 2

# Return true to indicate success
end # module Agg_abstract_type_dispatch

# ===== source: dispatch/bare_abstract_numeric_param_type_generic_5076.jl =====
module Agg_bare_abstract_numeric_param_type_generic_5076
# Regression: bare abstract-numeric parameter annotations (`x::Real`,
# `x::Number`, `x::Integer`, `x::Signed`, ...) must NOT widen type-generic
# results (`zero`, `one`, `oneunit`) to Float64/Int64; they must preserve the
# concrete argument type, matching upstream and the `where {T<:Real}` form
# (Issue #5076).
#
# Before the fix, `f(x::Real)=zero(x)` widened `x` to `ValueType::F64` at
# compile time (`type_helpers::julia_type_to_value_type` maps Real/Number to
# F64, Integer to I64), so `infer_julia_type` reported `Float64` and statically
# bound `zero(x)` to `zero(::Float64)`. `f(3)` then ran the Float64 body and
# errored ("expected I64, got Float64"); `f(Int8(3))` returned `0.0::Float64`.
# The fix makes `infer_julia_type` report `Any` for params already tracked in
# `abstract_numeric_params` (which already load via `LoadAny`), so type-generic
# calls dispatch on the concrete runtime value, like the untyped/`where` forms.
#
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

fR(x::Real) = zero(x)
fN(x::Number) = zero(x)
fI(x::Integer) = zero(x)
fS(x::Signed) = zero(x)
oR(x::Real) = one(x)
oN(x::Number) = one(x)
oI(x::Integer) = one(x)
uR(x::Real) = oneunit(x)
uN(x::Number) = oneunit(x)
uI(x::Integer) = oneunit(x)

@testset "zero via bare abstract annotation" begin
    @test fR(3) === 0
    @test fR(Int8(3)) === Int8(0)
    @test fR(Int16(3)) === Int16(0)
    @test fR(Int32(3)) === Int32(0)
    @test fR(3.0) === 0.0
    @test fN(3) === 0
    @test fN(Int8(3)) === Int8(0)
    @test fN(3.0) === 0.0
    @test fI(3) === 0
    @test fI(Int8(3)) === Int8(0)
    @test fS(3) === 0
    @test fS(Int8(3)) === Int8(0)
    @test typeof(fR(3)) === Int64
    @test typeof(fR(Int8(3))) === Int8
    @test typeof(fR(3.0)) === Float64
end

@testset "one via bare abstract annotation" begin
    @test oR(3) === 1
    @test oR(Int8(3)) === Int8(1)
    @test oR(Int32(3)) === Int32(1)
    @test oR(3.0) === 1.0
    @test oN(3) === 1
    @test oN(Int8(3)) === Int8(1)
    @test oI(3) === 1
    @test oI(Int8(3)) === Int8(1)
    @test typeof(oR(3)) === Int64
    @test typeof(oR(Int8(3))) === Int8
end

@testset "oneunit via bare abstract annotation" begin
    @test uR(3) === 1
    @test uR(Int8(3)) === Int8(1)
    @test uR(3.0) === 1.0
    @test uN(3) === 1
    @test uI(3) === 1
    @test uI(Int8(3)) === Int8(1)
    @test typeof(uR(Int8(3))) === Int8
end

@testset "bare abstract matches where-form and untyped" begin
    wR(x::T) where {T<:Real} = zero(x)
    nA(x) = zero(x)
    @test fR(3) === wR(3) === nA(3)
    @test fR(Int8(3)) === wR(Int8(3)) === nA(Int8(3))
    @test fR(3.0) === wR(3.0) === nA(3.0)
end
end # module Agg_bare_abstract_numeric_param_type_generic_5076

# ===== source: dispatch/callable_abstract_parent_dispatch_8264.jl =====
module Agg_callable_abstract_parent_dispatch_8264
using Test

abstract type AbstractCallableParent8264 end

struct ConcreteCallableParent8264{T} <: AbstractCallableParent8264
    tag::T
end

struct CallableArg8264{T}
    value::T
end

function (parent::AbstractCallableParent8264)(x::CallableArg8264{T}, y::CallableArg8264{T}) where T
    return CallableArg8264{T}(parent.tag + x.value - y.value)
end

parent = ConcreteCallableParent8264{Int}(10)
result = parent(CallableArg8264{Int}(7), CallableArg8264{Int}(3))

@test result.value == 14
@test typeof(result) === CallableArg8264{Int}
end # module Agg_callable_abstract_parent_dispatch_8264

# ===== source: dispatch/dynamic_callable_datatype.jl =====
module Agg_dynamic_callable_datatype
using Test
import Base: Float64

struct DynamicCallableWrap
    x::Int64
end

struct DynamicCallablePair
    x::Int64
    y::Int64
end

struct DynamicCallableFloat64Input3910
    x::Int64
end

function call_one_arg_type(T, x)
    return T(x)
end

function call_two_arg_type(T, x, y)
    return T(x, y)
end

function call_one_arg_function(f, x)
    return f(x)
end

function call_two_arg_function(f, x, y)
    return f(x, y)
end

dynamic_inc(x::Int64) = x + 1
dynamic_add(x::Int64, y::Int64) = x + y
Float64(x::DynamicCallableFloat64Input3910) = 3910.0

@testset "Any-typed callable values use runtime callable dispatch" begin
    w = call_one_arg_type(DynamicCallableWrap, 41)
    @test w.x == 41

    p = call_two_arg_type(DynamicCallablePair, 20, 22)
    @test p.x == 20
    @test p.y == 22

    @test call_one_arg_type(Float64, 7) == 7.0
    @test call_one_arg_type(Float64, DynamicCallableFloat64Input3910(1)) == 3910.0
    @test call_one_arg_function(dynamic_inc, 41) == 42
    @test call_two_arg_function(dynamic_add, 20, 22) == 42
end
end # module Agg_dynamic_callable_datatype

# ===== source: dispatch/interproc_forward_abstract_numeric_type_generic_5167.jl =====
module Agg_interproc_forward_abstract_numeric_type_generic_5167
# Regression: a bare abstract-numeric parameter (`x::Real`, `x::Number`,
# `x::Integer`, ...) FORWARDED into another user function whose body performs a
# type-generic call (`zero`, `one`, `oneunit`) must preserve the concrete
# argument type — matching upstream Julia 1.12 and the untyped/`where {T<:Real}`
# forms (Issue #5167 part 2; follow-up to #5076/#5169).
#
# Before the fix `g(y)=zero(y); f(x::Real)=g(x); f(3)` returned `0.0::Float64`
# instead of `0::Int64`. Root cause: `f`'s param `x::Real` is stored in the
# compiler's `locals` as `ValueType::F64` (Real/Number → F64). `f` always loads
# `x` via `LoadAny`, so the value reaching `g` is the correct concrete `Int64`,
# and `g`'s body `zero(y)` correctly produced `0::Int64` at runtime. But the
# call site `g(x)` in `f` re-inferred `g`'s return type with the *static* arg
# ValueType `F64` (via `infer_expr_type(x)` → F64 →
# `infer_function_return_type_v2_with_arg_types(g, [F64])` → `zero(::Float64)`
# → F64), so `f` coerced `g(x)`'s runtime `Int64` result to `Float64` on return.
# The fix makes `infer_expr_type` report `Any` for `abstract_numeric_params`
# (mirroring `infer_julia_type` / the `LoadAny` representation), so the
# speculative re-inference is skipped and the forwarded type-generic call
# dispatches on the concrete runtime value.
#
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

gz(y) = zero(y)
go(y) = one(y)
gu(y) = oneunit(y)

fzR(x::Real) = gz(x)
foR(x::Real) = go(x)
fuR(x::Real) = gu(x)
fzN(x::Number) = gz(x)
fzI(x::Integer) = gz(x)

@testset "zero forwarded through ::Real" begin
    @test fzR(3) === 0
    @test fzR(Int8(3)) === Int8(0)
    @test fzR(Int16(3)) === Int16(0)
    @test fzR(Int32(3)) === Int32(0)
    @test fzR(Int64(3)) === Int64(0)
    @test fzR(2.0f0) === 0.0f0
    @test fzR(2.0) === 0.0
end

@testset "one forwarded through ::Real" begin
    @test foR(3) === 1
    @test foR(Int8(3)) === Int8(1)
    @test foR(Int16(3)) === Int16(1)
    @test foR(2.0f0) === 1.0f0
    @test foR(2.0) === 1.0
end

@testset "oneunit forwarded through ::Real" begin
    @test fuR(3) === 1
    @test fuR(Int8(3)) === Int8(1)
    @test fuR(2.0f0) === 1.0f0
    @test fuR(2.0) === 1.0
end

@testset "zero forwarded through ::Number / ::Integer" begin
    @test fzN(Int8(7)) === Int8(0)
    @test fzN(2.0f0) === 0.0f0
    @test fzI(Int16(7)) === Int16(0)
    @test fzI(Int32(7)) === Int32(0)
end

# Two-hop forwarding: ::Real → untyped → untyped type-generic call.
k1(y) = zero(y)
k2(z) = k1(z)
k3(x::Real) = k2(x)

@testset "zero forwarded through two hops" begin
    @test k3(Int8(9)) === Int8(0)
    @test k3(Int16(9)) === Int16(0)
    @test k3(2.0f0) === 0.0f0
    @test k3(2.0) === 0.0
end

# Cross-check: the bare ::Real forward agrees with the untyped and
# `where {T<:Real}` forms (both already correct before the fix).
nz(x) = gz(x)
wz(x::T) where {T<:Real} = gz(x)

@testset "bare ::Real forward matches untyped and where forms" begin
    for v in (3, Int8(3), Int16(3), Int32(3), 2.0f0, 2.0)
        @test fzR(v) === nz(v) === wz(v)
    end
end
end # module Agg_interproc_forward_abstract_numeric_type_generic_5167

# ===== source: dispatch/multi_level_abstract_dispatch.jl =====
module Agg_multi_level_abstract_dispatch
# Test multi-level abstract type hierarchy dispatch (Issue #3147)
# Dispatch should correctly route through intermediate abstract type levels.

using Test

# Multi-level hierarchy with sibling abstract types
abstract type Vehicle end
abstract type MotorVehicle <: Vehicle end
abstract type NonMotorVehicle <: Vehicle end
struct Car <: MotorVehicle end
struct Bicycle <: NonMotorVehicle end

f(::MotorVehicle) = "motor"
f(::NonMotorVehicle) = "non-motor"

@test f(Car()) == "motor"
@test f(Bicycle()) == "non-motor"
end # module Agg_multi_level_abstract_dispatch

# ===== source: dispatch/subtype_isa_first_class_5115.jl =====
module Agg_subtype_isa_first_class_5115
# Issue #5115: <: / >: / isa as first-class function/operator values
# Upstream Julia treats <:, >:, isa as ordinary functions (julia/base/operators.jl).
# They must be usable as referenceable callables: (<:)(A, B), bound to a variable,
# qualified as Base.:(<:), and used inside higher-order predicates.
using Test

# --- 2-arg calls as first-class function values ---
@test (<:)(Int, Number) == true
@test (<:)(Number, Int) == false
@test (>:)(Number, Int) == true
@test (>:)(Int, Number) == false
@test isa(3, Int) == true
@test isa(3, String) == false

# --- bound to a variable, then called ---
sub = (<:)
@test sub(Int, Number) == true
@test sub(String, Number) == false

sup = (>:)
@test sup(Number, Int) == true

isa_fn = isa
@test isa_fn(3, Int) == true
@test isa_fn("x", Int) == false

# --- Base.:(op) qualified references ---
@test Base.:(<:)(Int, Number) == true
@test Base.:(>:)(Number, Int) == true
@test Base.:(isa)(3, Int) == true

# --- higher-order: filter over a vector of types ---
@test filter(t -> t <: Real, [Int, String, Float64]) == [Int, Float64]
@test filter(t -> t <: Real, Any[Int, String, Float64]) == Any[Int, Float64]

# --- higher-order: map with isa ---
@test map(x -> isa(x, Int), Any[1, "a", 2.0]) == Bool[true, false, false]

println("all 5115 checks passed")
end # module Agg_subtype_isa_first_class_5115

# ===== source: dispatch/typed_dispatch_covariant_resolver_3910.jl =====
module Agg_typed_dispatch_covariant_resolver_3910
using Test

abstract type DispatchAnimal3910 end
struct DispatchDog3910 <: DispatchAnimal3910 end
struct DispatchCat3910 <: DispatchAnimal3910 end

typed_dispatch_covariant_resolver_3910(::Type{<:DispatchAnimal3910}) = "animal"
typed_dispatch_covariant_resolver_3910(::Type{DispatchDog3910}) = "dog"

function typed_dispatch_covariant_resolver_via_any_3910(t)
    u::Any = t
    typed_dispatch_covariant_resolver_3910(u)
end

@testset "typed dispatch covariant resolver (Issue #3910)" begin
    @test typed_dispatch_covariant_resolver_via_any_3910(DispatchDog3910) == "dog"
    @test typed_dispatch_covariant_resolver_via_any_3910(DispatchCat3910) == "animal"
end
end # module Agg_typed_dispatch_covariant_resolver_3910

true
