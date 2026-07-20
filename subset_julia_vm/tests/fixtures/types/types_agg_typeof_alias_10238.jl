# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: types/apply_type_5112.jl =====
module Agg_apply_type_5112
# Core.apply_type + splat in parametric type construction (Issue #5112)
# Construct parametric types from computed/splatted type values.

using Test

struct AT5112Box{T}
    x::T
end

@testset "Core.apply_type with computed type values" begin
    @test Core.apply_type(Tuple, Int, Real) === Tuple{Int64,Real}
    @test Core.apply_type(Tuple, Int) === Tuple{Int64}
    @test Core.apply_type(Array, Int, 1) === Vector{Int64}
    @test Core.apply_type(Vector, Int) === Vector{Int64}
    @test Core.apply_type(AT5112Box, Int) === AT5112Box{Int64}
end

@testset "splat in T{xs...}" begin
    ts = [Int, Real]
    @test Tuple{ts...} === Tuple{Int64,Real}

    tt = (Int, Real)
    @test Tuple{tt...} === Tuple{Int64,Real}

    # Leading static arg followed by a splat
    gg = [Real]
    @test Tuple{Int,gg...} === Tuple{Int64,Real}

    # Variable-arity through a vararg function
    f(args...) = Tuple{args...}
    @test f(Int, Real) === Tuple{Int64,Real}
    @test f(Int) === Tuple{Int64}
end
end # module Agg_apply_type_5112

# ===== source: types/primitive_type_decl_5058.jl =====
module Agg_primitive_type_decl_5058
using Test

# Issue #5058: user `primitive type Name Bits end` declarations integrate with
# type reflection (isprimitivetype / isbitstype / sizeof / supertype / <: / isa).
# Value construction (MyBits(0x01), reinterpret) is explicitly out of scope.

primitive type MyBits 8 end
primitive type MyU8 <: Unsigned 8 end
primitive type Big512 512 end

@testset "primitive type declarations" begin
    # Bare primitive type (implicit Any supertype)
    @test isprimitivetype(MyBits) == true
    @test isbitstype(MyBits) == true
    @test sizeof(MyBits) == 1
    @test supertype(MyBits) === Any
    @test (MyBits isa Type) == true
    @test MyBits === MyBits

    # Primitive type with an explicit abstract supertype
    @test supertype(MyU8) === Unsigned
    @test (MyU8 <: Unsigned) == true
    # Transitive subtyping through the abstract hierarchy
    @test (MyU8 <: Integer) == true

    # Larger bit width
    @test sizeof(Big512) == 64
end
end # module Agg_primitive_type_decl_5058

# ===== source: types/primitive_type_expr_bits_9050.jl =====
module Agg_primitive_type_expr_bits_9050
# Primitive type bit-size expressions (Issue #9050)

using Test

primitive type ExprBits9050 (18 * 8) end
primitive type ExprBitsParen9050 ((9 + 9) * 8) end

@testset "primitive type expression bit sizes (Issue #9050)" begin
    @test isprimitivetype(ExprBits9050)
    @test sizeof(ExprBits9050) == 18
    @test sizeof(ExprBitsParen9050) == 18
end
end # module Agg_primitive_type_expr_bits_9050

# ===== source: types/test_type_alias.jl =====
module Agg_test_type_alias
# Test type aliases with const (Issue #465)

using Test

# Type aliases must be outside @testset
const MyInt = Int64
const MyFloat = Float64

@testset "Type aliases" begin
    # Test 1: Type alias equality - aliases resolve to the same type
    @test MyInt === Int64
    @test MyFloat === Float64

    # Test 2: Type aliases can be used in typeof comparisons
    x = 42
    @test typeof(x) == MyInt

    y = 3.14
    @test typeof(y) == MyFloat

    # Test 3: Type aliases work with isa checks
    @test 100 isa MyInt
    @test 2.5 isa MyFloat
end
end # module Agg_test_type_alias

# ===== source: types/test_type_as_value.jl =====
module Agg_test_type_as_value
# Test Type{T} - types as first-class values (Issue #993)
# Tests the Type hierarchy: DataType <: Type <: Any

using Test

# Function definitions must be outside @testset

# Function dispatch on Type{T}
function get_zero(::Type{Int64})
    0
end

function get_zero(::Type{Float64})
    0.0
end

@testset "Type{T} as first-class values" begin
    # Test 1: Type{T} dispatch with concrete types
    @test get_zero(Int64) == 0
    @test get_zero(Float64) == 0.0

    # Test 2: typeof on types returns DataType
    @test typeof(Int64) == DataType
    @test typeof(Float64) == DataType

    # Test 3: Type identity with ===
    @test Int64 === Int64
    @test Float64 === Float64

    # Test 4: DataType <: Type hierarchy
    # In Julia, all type objects are instances of Type
    @test DataType <: Type
    @test Type <: Any
end
end # module Agg_test_type_as_value

# ===== source: types/test_type_t_pattern.jl =====
module Agg_test_type_t_pattern
# Test Type{T} pattern matching in dispatch (Issue #465)

using Test

# Function definitions must be outside @testset

# Test 1: Function dispatch on Type{T}
function create_default(::Type{Int64})
    0
end

function create_default(::Type{Float64})
    0.0
end

# Test 2: Type{T} as return value in subtype checks
function is_numeric_type(::Type{Int64})
    true
end

function is_numeric_type(::Type{Float64})
    true
end

function is_numeric_type(::Type{T}) where T
    false
end

@testset "Type{T} pattern matching" begin
    # Test 1: Basic Type{T} dispatch
    @test create_default(Int64) == 0
    @test create_default(Float64) == 0.0

    # Test 2: Type{T} dispatch with where clause fallback
    @test is_numeric_type(Int64) == true
    @test is_numeric_type(Float64) == true
    @test is_numeric_type(String) == false

    # Test 3: typeof returns Type
    @test typeof(Int64) <: Type
    @test typeof(Float64) <: Type
end
end # module Agg_test_type_t_pattern

# ===== source: types/typeof_set.jl =====
module Agg_typeof_set
# Test typeof(Set) returns upstream-compatible Set element types (Issues #527, #4018)
# Previously typeof(Set(...)) returned Any instead of a concrete Set type

using Test

# Test case 1: typeof empty Set
s1 = Set()
@test typeof(s1) == Set{Any}

# Test case 2: typeof Set with integer elements
s2 = Set([1, 2, 3])
@test typeof(s2) == Set{Int64}

# Test case 3: typeof Set with string elements
s3 = Set(["a", "b", "c"])
@test typeof(s3) == Set{String}

# Return true to indicate success
end # module Agg_typeof_set

true
