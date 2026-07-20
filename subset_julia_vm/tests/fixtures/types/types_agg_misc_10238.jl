# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: types/array_return_type.jl =====
module Agg_array_return_type
# Test array return type annotations (::Array{T,N})

using Test

# Vector return type (1D array)
function make_vector(n::Int64)::Array{Int64,1}
    result = zeros(Int64, n)
    for i in 1:n
        result[i] = i * i
    end
    return result
end

# Matrix return type (2D array)
function make_matrix(rows::Int64, cols::Int64)::Array{Int64,2}
    result = zeros(Int64, rows, cols)
    for i in 1:rows
        for j in 1:cols
            result[i, j] = i + j
        end
    end
    return result
end

# Alternative syntax: Vector{T} and Matrix{T}
function make_vector_alt(n::Int64)::Vector{Float64}
    result = zeros(Float64, n)
    for i in 1:n
        result[i] = Float64(i) * 0.5
    end
    return result
end

function make_matrix_alt(n::Int64)::Matrix{Float64}
    result = zeros(Float64, n, n)
    for i in 1:n
        result[i, i] = 1.0
    end
    return result
end

@testset "Array return type annotations" begin
    v = make_vector(3)
    @test length(v) == 3
    @test v[1] == 1
    @test v[2] == 4
    @test v[3] == 9

    m = make_matrix(2, 3)
    @test size(m) == (2, 3)
    @test m[1, 1] == 2
    @test m[2, 3] == 5

    v2 = make_vector_alt(4)
    @test length(v2) == 4
    @test v2[2] == 1.0

    m2 = make_matrix_alt(3)
    @test m2[1, 1] == 1.0
    @test m2[2, 2] == 1.0
end
end # module Agg_array_return_type

# ===== source: types/bool_struct_fields.jl =====
module Agg_bool_struct_fields
# Test Bool struct field access
# This test ensures that Bool fields in structs can be accessed correctly
# (regression test for Issue #1612)

using Test

# Struct definitions must be at top level (outside @testset)
mutable struct BoolContainer
    flag::Bool
    count::Int64
end

mutable struct MultiBool
    a::Bool
    b::Bool
    c::Int64
end

struct ImmutableBool
    flag::Bool
    value::Int64
end

@testset "Bool struct field access" begin
    bc = BoolContainer(true, 42)
    @test bc.flag == true
    @test bc.count == 42

    bc.flag = false
    @test bc.flag == false

    # Bool can be used in numeric context (Bool <: Integer in Julia)
    @test bc.flag + 1 == 1
    @test true + bc.count == 43
end

@testset "Multiple Bool fields" begin
    mb = MultiBool(true, false, 10)
    @test mb.a == true
    @test mb.b == false
    @test mb.c == 10

    # Modify Bool fields
    mb.a = false
    mb.b = true
    @test mb.a == false
    @test mb.b == true
end

@testset "Bool field in immutable struct" begin
    ib = ImmutableBool(true, 100)
    @test ib.flag == true
    @test ib.value == 100
end
end # module Agg_bool_struct_fields

# ===== source: types/bottom_systematic_5065.jl =====
module Agg_bottom_systematic_5065
# Issue #5065: systematic treatment of Union{} (Bottom).
# Bottom is the empty type: a subtype of every type, the zero element of
# typeintersect, and the empty-Union normal form. `const Bottom = Union{}`
# (essentials.jl) names it. This fixture pins that behaviour against upstream
# Julia. A local `const Bottom = Union{}` keeps parity in both runtimes: it is
# defined in Base (not exported to Main upstream), so binding it locally is the
# portable way to reference the name identically under sjulia and julia.

using Test

const Bottom = Union{}

@testset "Bottom (Union{}) systematic semantics" begin
    # 1. Bottom is the canonical name for the empty Union.
    @test Bottom === Union{}

    # 2. Bottom <: T holds for every type T (Bottom is the lattice bottom).
    @test (Bottom <: Int) == true
    @test (Bottom <: Number) == true
    @test (Bottom <: String) == true
    @test (Bottom <: Any) == true
    @test (Bottom <: Union{Int, Float64}) == true
    @test (Bottom <: Bottom) == true

    # 3. T <: Bottom holds only when T === Bottom.
    @test (Int <: Bottom) == false
    @test (Any <: Bottom) == false
    @test (Number <: Bottom) == false

    # 4. typeintersect: Bottom is the zero element; disjoint types meet at Bottom.
    @test typeintersect(Int, String) === Bottom
    @test typeintersect(Int, Bottom) === Bottom
    @test typeintersect(Bottom, Number) === Bottom
    @test typeintersect(Bottom, Bottom) === Bottom
    # Non-disjoint intersection is unaffected.
    @test typeintersect(Int, Integer) === Int

    # 5. Empty-Union normalization: Bottom is absorbed / collapsed.
    @test Union{Int} === Int
    @test Union{Bottom, Int} === Int
    @test Union{Int, Bottom} === Int
    @test Union{Bottom} === Bottom
    @test Union{Bottom, Bottom} === Bottom

    # 6. isa: no value is an instance of Bottom.
    @test isa(1, Bottom) == false
    @test isa("x", Bottom) == false
end
end # module Agg_bottom_systematic_5065

# ===== source: types/capture_avoiding_instantiate_5054.jl =====
module Agg_capture_avoiding_instantiate_5054
# Regression guard for capture-avoiding type-variable substitution (Issue #5054).
# The internal `instantiate`/`substitute` machinery was made capture-avoiding;
# this fixture confirms ordinary parametric instantiation (the common,
# non-capturing path) keeps producing the right concrete types and dispatch.
using Test

struct Box{T}
    value::T
end

# Parametric method whose body reuses the type variable.
unwrap(b::Box{T}) where {T} = b.value
boxtype(::Box{T}) where {T} = T

# Multi-parameter parametric struct.
struct Pair2{A,B}
    first::A
    second::B
end
firsttype(::Pair2{A,B}) where {A,B} = A
secondtype(::Pair2{A,B}) where {A,B} = B

@testset "parametric instantiation regression (Issue #5054)" begin
    # Builtin parametric instantiation.
    @test Vector{Int}([1, 2, 3]) == [1, 2, 3]
    @test typeof(Vector{Int}([1, 2, 3])) === Vector{Int}
    @test Dict{String,Int} <: AbstractDict
    @test Tuple{Int,String} <: Tuple

    # User parametric struct instantiation + parametric dispatch.
    b = Box{Int}(7)
    @test b.value == 7
    @test typeof(b) === Box{Int}
    @test unwrap(b) == 7
    @test boxtype(b) === Int

    # Distinct type-variable names on a multi-parameter struct must not collide.
    p = Pair2{Int,String}(1, "x")
    @test firsttype(p) === Int
    @test secondtype(p) === String
    @test typeof(p) === Pair2{Int,String}
end
end # module Agg_capture_avoiding_instantiate_5054

# ===== source: types/compound_field_assignment.jl =====
module Agg_compound_field_assignment
# Compound assignment on mutable struct fields (Issue #2140)
# Verifies that obj.field += expr, obj.field -= expr, etc. work correctly.

using Test

mutable struct Counter
    count::Int64
end

mutable struct Vec2
    x::Float64
    y::Float64
end

function increment(c::Counter)
    c.count += 1
    return c.count
end

@testset "compound += on struct field (Issue #2140)" begin
    c = Counter(0)
    c.count += 1
    @test c.count == 1
    c.count += 5
    @test c.count == 6
end

@testset "compound -= on struct field (Issue #2140)" begin
    c = Counter(10)
    c.count -= 3
    @test c.count == 7
end

@testset "compound *= on struct field (Issue #2140)" begin
    c = Counter(5)
    c.count *= 3
    @test c.count == 15
end

@testset "compound /= on struct field (Issue #2140)" begin
    v = Vec2(10.0, 20.0)
    v.x /= 2.0
    @test v.x == 5.0
end

@testset "compound ^= on struct field (Issue #2140)" begin
    v = Vec2(3.0, 0.0)
    v.x ^= 2
    @test v.x == 9.0
end

@testset "compound assignment inside function (Issue #2140)" begin
    c = Counter(0)
    @test increment(c) == 1
    @test increment(c) == 2
    @test c.count == 2
end
end # module Agg_compound_field_assignment

# ===== source: types/eltype_number_dynamic_dispatch_4665.jl =====
module Agg_eltype_number_dynamic_dispatch_4665
using Test

function dynamic_eltype(x)
    eltype(x)
end

function dynamic_type_eltype(T)
    eltype(T)
end

@testset "number eltype dynamic dispatch (Issue #4665)" begin
    @test eltype(1) === Int64
    @test dynamic_eltype(1) === Int64
    @test dynamic_eltype(Int8(1)) === Int8
    @test dynamic_eltype(UInt8(1)) === UInt8
    @test dynamic_eltype(1.0) === Float64
    @test dynamic_eltype(Float32(1)) === Float32
    @test dynamic_eltype(true) === Bool

    @test eltype(Int64) === Int64
    @test dynamic_type_eltype(Int64) === Int64
    @test dynamic_type_eltype(Int8) === Int8
    @test dynamic_type_eltype(Float64) === Float64
end
end # module Agg_eltype_number_dynamic_dispatch_4665

# ===== source: types/float_field_type_preservation.jl =====
module Agg_float_field_type_preservation
# Float field type preservation test
# Ensures that parametric struct fields preserve their float type (F16, F32, F64)
# Prevention test for Issue #1651 / #1655

using Test

struct FloatHolder{T}
    value::T
end

@testset "Float32 field type preservation" begin
    h = FloatHolder{Float32}(Float32(1.5))
    @test typeof(h.value) === Float32
    @test h.value == Float32(1.5)
end

@testset "Float64 field type preservation" begin
    h = FloatHolder{Float64}(1.5)
    @test typeof(h.value) === Float64
    @test h.value == 1.5
end

@testset "Float16 field type preservation" begin
    h = FloatHolder{Float16}(Float16(1.5))
    @test typeof(h.value) === Float16
    @test h.value == Float16(1.5)
end
end # module Agg_float_field_type_preservation

# ===== source: types/single_letter_struct_datatype_5252.jl =====
module Agg_single_letter_struct_datatype_5252
# Single uppercase-letter struct/abstract names must classify as DataType,
# not TypeVar (Issue #5252). Multi-letter names already worked; this guards
# the disambiguation that a *declared* type name resolves to a DataType
# regardless of length, while undefined single letters stay type variables.

using Test

struct P; x::Int64; y::Int64; end
struct T; v::Int64; end
struct AB; x::Int64; end
abstract type N end
struct M <: N; w::Int64; end

@testset "single-letter struct names are DataType, not TypeVar (Issue #5252)" begin
    # Classification: typeof / isa
    @assert isa(P, DataType)
    @assert isa(T, DataType)
    @assert isa(AB, DataType)
    @assert typeof(P) === DataType
    @assert typeof(T) === DataType
    @assert typeof(AB) === DataType
    @assert !isa(P, TypeVar)
    @assert !isa(T, TypeVar)

    # Concreteness / bits-layout reflection
    @assert isconcretetype(P)
    @assert isconcretetype(T)
    @assert isbitstype(P)
    @assert isbitstype(T)
    @assert sizeof(P) == 16
    @assert sizeof(T) == 8
    @assert fieldnames(P) == (:x, :y)
    @assert fieldnames(T) == (:v,)

    # Single-letter abstract type stays abstract; its concrete child is concrete
    @assert isabstracttype(N)
    @assert !isconcretetype(N)
    @assert isconcretetype(M)
    @assert isbitstype(M)

    # Construction, field access, and instance typing
    p = P(3, 4)
    @assert typeof(p) === P
    @assert p.x == 3
    @assert p.y == 4
    @assert isa(p, P)
    @assert p isa P

    t = T(7)
    @assert typeof(t) === T
    @assert t.v == 7
    @assert isa(t, T)

    m = M(9)
    @assert typeof(m) === M
    @assert isa(m, N)
    @assert m isa N

    # Dispatch on single-letter concrete types and abstract supertype
    f(::P) = 1
    f(::T) = 2
    f(::Int) = 3
    g(::N) = 100
    @assert f(p) == 1
    @assert f(t) == 2
    @assert f(5) == 3
    @assert g(m) == 100

    @test (true)
end
end # module Agg_single_letter_struct_datatype_5252

# ===== source: types/sizeof_datatype_layout_3909.jl =====
module Agg_sizeof_datatype_layout_3909
using Test

struct LayoutBits3909
    x::Int64
    y::Bool
end

struct LayoutRefs3909
    name::String
    x::Int64
end

mutable struct MutableLayout3909
    x::Int64
end

struct EmptyLayout3909
end

@testset "sizeof(::DataType) uses runtime layout metadata (Issue #3909)" begin
    @test sizeof(Bool) == 1
    @test sizeof(Char) == 4
    @test sizeof(Int8) == 1
    @test sizeof(Nothing) == 0
    @test sizeof(Missing) == 0

    @test sizeof(LayoutBits3909) == 16
    @test sizeof(LayoutRefs3909) == 16
    @test sizeof(MutableLayout3909) == 8
    @test sizeof(EmptyLayout3909) == 0

    @test isbitstype(LayoutBits3909)
    @test !isbitstype(LayoutRefs3909)
    @test !isbitstype(MutableLayout3909)
end
end # module Agg_sizeof_datatype_layout_3909

# ===== source: types/test_contravariant_type_params.jl =====
module Agg_test_contravariant_type_params
# Test contravariant type parameters (Issue #465)
# Contravariant syntax {>:T} represents a set of types (UnionAll)

using Test

# Type definitions must be outside @testset
abstract type Shape{T} end
struct Circle{T} <: Shape{T}
    radius::T
end

@testset "Contravariant type parameters" begin
    # Test 1: Contravariant type relationships
    # Shape{>:Int64} is a UnionAll type representing all Shape{T} where T >: Int64
    @test Circle{Real} <: Shape{>:Int64}
    @test Circle{Number} <: Shape{>:Int64}
    @test Circle{Any} <: Shape{>:Int64}
    @test Circle{Int64} <: Shape{>:Int64}

    # Test 2: Contravariant does not match subtypes
    @test !(Circle{Int32} <: Shape{>:Int64})

    # Test 3: Array contravariant types
    # Array{>:Int64} represents all Array{T} where T >: Int64
    @test Array{Real} <: Array{>:Int64}
    @test Array{Number} <: Array{>:Int64}
    @test Array{Any} <: Array{>:Int64}
    @test Array{Int64} <: Array{>:Int64}

    # Test 4: Vector contravariant types
    @test Vector{Real} <: Vector{>:Integer}
    @test Vector{Number} <: Vector{>:Integer}
    @test !(Vector{Int64} <: Vector{>:Integer})  # Int64 is not a supertype of Integer
end
end # module Agg_test_contravariant_type_params

# ===== source: types/test_covariant_type_params.jl =====
module Agg_test_covariant_type_params
# Test covariant type parameters in function dispatch (Issue #834)

using Test

# Function definitions must be outside @testset

# Test 1: Function with Array{<:Number} constraint
function sum_numbers(arr::Array{<:Number})
    total = 0.0
    for x in arr
        total += x
    end
    total
end

# Test 2: Function with Vector{<:Integer} constraint
function count_positive(arr::Vector{<:Integer})
    count = 0
    for x in arr
        if x > 0
            count += 1
        end
    end
    count
end

# Test 3: Function overloading with covariant types
function process_array(arr::Array{<:Integer})
    "integer array"
end

function process_array(arr::Array{<:AbstractFloat})
    "float array"
end

@testset "Covariant type parameters in dispatch" begin
    # Test 1: Array{<:Number} should accept Int64 arrays
    int_arr = [1, 2, 3, 4, 5]
    @test sum_numbers(int_arr) == 15.0

    # Test 2: Array{<:Number} should accept Float64 arrays
    float_arr = [1.5, 2.5, 3.0]
    @test sum_numbers(float_arr) == 7.0

    # Test 3: Vector{<:Integer} should accept Int64 arrays
    @test count_positive([1, -2, 3, -4, 5]) == 3
    @test count_positive([-1, -2, -3]) == 0

    # Test 4: Dispatch based on element type constraint
    @test process_array([1, 2, 3]) == "integer array"
    @test process_array([1.0, 2.0, 3.0]) == "float array"
end
end # module Agg_test_covariant_type_params

# ===== source: types/typed_assignment_convert_5148.jl =====
module Agg_typed_assignment_convert_5148
# Typed variable declarations `x::T = v` must `convert(T, v)` on assignment
# (Issue #5148). A bare `x::T` in expression position is a type assertion.

using Test

# `x::Float64 = 3` converts the Int literal 3 to the Float64 value 3.0.
function decl_int_to_float()
    x::Float64 = 3
    return (x, typeof(x))
end

# Converting a non-integral Float64 to Int must throw InexactError, exactly
# like `convert(Int, 3.7)`.
function decl_inexact()
    x::Int = 3.7
    return x
end

# `x::Float64 = x + y` converts the (Int) sum through Float64.
function decl_convert_expr()
    a = 2
    b = 3
    s::Float64 = a + b
    return (s, typeof(s))
end

# A bare `x::T` used as an expression is a type assertion: it returns the
# value when the runtime type matches.
function assertion_match()
    x = 5.0
    return x::Float64
end

# ... and throws a TypeError when the runtime type does not match.
function assertion_mismatch()
    x = 5
    return x::Float64
end

# `global g::T = v` enforces the declared type on the (correctly typed) value.
global g5148::Int = 5

@testset "typed assignment convert (Issue 5148)" begin
    @test decl_int_to_float() == (3.0, Float64)
    @test_throws InexactError decl_inexact()
    @test decl_convert_expr() == (5.0, Float64)
    @test assertion_match() == 5.0
    @test_throws TypeError assertion_mismatch()
    @test g5148 == 5
    @test typeof(g5148) == Int
end
end # module Agg_typed_assignment_convert_5148

# ===== source: types/typed_f32_f16_instructions.jl =====
module Agg_typed_f32_f16_instructions
# Test typed Return/Store/Load instructions for Float32 and Float16
# Issue #1893: F32/F16 previously fell through to ReturnAny/StoreAny/LoadAny

using Test

# Float32 function return
function return_f32()
    Float32(3.14)
end

# Float16 function return
function return_f16()
    Float16(2.71)
end

# Float32 local variable store/load roundtrip
function f32_store_load()
    x = Float32(1.5)
    y = Float32(2.5)
    x + y
end

# Float32 roundtrip through function
function f32_identity(x)
    x
end

# Float16 roundtrip through function
function f16_identity(x)
    x
end

@testset "Typed F32/F16 instructions" begin
    @test typeof(return_f32()) == Float32
    @test return_f32() == Float32(3.14)

    @test typeof(return_f16()) == Float16
    @test return_f16() == Float16(2.71)

    @test typeof(f32_store_load()) == Float32
    @test f32_store_load() == Float32(4.0)

    @test typeof(f32_identity(Float32(5.0))) == Float32
    @test f32_identity(Float32(5.0)) == Float32(5.0)

    @test typeof(f16_identity(Float16(3.0))) == Float16
    @test f16_identity(Float16(3.0)) == Float16(3.0)
end
end # module Agg_typed_f32_f16_instructions

# ===== source: types/typed_local_var.jl =====
module Agg_typed_local_var
# Test typed local variable declarations (x::Type = value)

using Test

function test_typed_locals()::Float64
    x::Float64 = 1.0
    y::Float64 = 2.0
    z::Float64 = x + y
    return z
end

function mandelbrot_escape(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr::Float64 = 0.0
    zi::Float64 = 0.0
    for k in 1:maxiter
        if (zr * zr + zi * zi) > 4.0
            return k
        end
        new_zr::Float64 = zr * zr - zi * zi + cr
        new_zi::Float64 = 2.0 * zr * zi + ci
        zr = new_zr
        zi = new_zi
    end
    return maxiter
end

@testset "Typed local variable declarations" begin
    @test test_typed_locals() == 3.0
    @test mandelbrot_escape(0.0, 0.0, 100) == 100  # Origin, inside set
    @test mandelbrot_escape(2.0, 0.0, 100) < 100   # Outside set, escapes
end
end # module Agg_typed_local_var

true
