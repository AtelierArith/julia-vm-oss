# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: types/collection_type_identity.jl =====
# Test type identity for NamedTuple, Dict, and Set
# Ensures typeof() returns the correct type for collection values
# Regression test for Issues #1894 and #4018


@testset "Dict type identity" begin
    d = Dict("a" => 1, "b" => 2)
    @test typeof(d) == Dict{String, Int64}
    @test isa(d, Dict)
end

@testset "Set type identity" begin
    s = Set([1, 2, 3])
    @test typeof(s) == Set{Int64}
    @test isa(s, Set)
end

@testset "NamedTuple type identity" begin
    nt = (a=1, b=2)
    @test isa(nt, NamedTuple)
end

# ===== source: types/eltype_test.jl =====
# Test eltype function
# Returns sum of successful checks (1 if eltype matches expected)


@testset "eltype function returns element type of collections" begin

    result = 0.0

    # Array element types - Float64 array
    if eltype([1.0, 2.0, 3.0]) === Float64
        result += 1.0
    end

    # Integer array
    if eltype([1, 2, 3]) === Int64
        result += 1.0
    end

    # Tuple element type (homogeneous)
    if eltype((1, 2, 3)) === Int64
        result += 1.0
    end

    # String element type
    if eltype("hello") === Char
        result += 1.0
    end

    @test (result) == 4.0
end

# ===== source: types/length_returns_int.jl =====
# Test that length() returns Int type for Array


@testset "length() returns Int64 type for Array" begin
    arr = [1, 2, 3, 4, 5]
    len = length(arr)
    @test (typeof(len) == Int64)
end

# ===== source: types/length_returns_int_range.jl =====
# Test that length() returns Int type for Range


@testset "length() returns Int64 type for Range" begin
    r = 1:10
    len = length(r)
    @test (typeof(len) == Int64)
end

# ===== source: types/length_returns_int_string.jl =====
# Test that length() returns Int type for String


@testset "length() returns Int64 type for String" begin
    s = "hello"
    len = length(s)
    @test (typeof(len) == Int64)
end

# ===== source: types/length_returns_int_tuple.jl =====
# Test that length() returns Int type for Tuple


@testset "length() returns Int64 type for Tuple" begin
    t = (1, 2, 3, 4)
    len = length(t)
    @test (typeof(len) == Int64)
end

# ===== source: types/sizeof_basic.jl =====
# Test sizeof function - get size of value in bytes


@testset "sizeof - get size of value in bytes" begin

    # Primitive types
    @assert sizeof(1) == 8          # Int64 is 8 bytes
    @assert sizeof(1.0) == 8        # Float64 is 8 bytes
    @assert sizeof(true) == 1       # Bool is 1 byte
    @assert sizeof('a') == 4        # Char is 4 bytes (Unicode)

    # String size is number of bytes
    @assert sizeof("hello") == 5
    @assert sizeof("") == 0

    # Array size is element_size * num_elements
    arr = [1.0, 2.0, 3.0]
    @assert sizeof(arr) == 24  # 3 elements * 8 bytes

    int8s = Vector{Int8}(undef, 4)
    int8s[1] = Int8(1)
    int8s[2] = Int8(2)
    int8s[3] = Int8(3)
    int8s[4] = Int8(4)
    @assert typeof(int8s) === Vector{Int8}
    @assert eltype(int8s) === Int8
    @assert sizeof(int8s) == 4

    int16s = Vector{Int16}(undef, 3)
    int16s[1] = Int16(1)
    int16s[2] = Int16(2)
    int16s[3] = Int16(3)
    @assert typeof(int16s) === Vector{Int16}
    @assert eltype(int16s) === Int16
    @assert sizeof(int16s) == 6

    int32s = Vector{Int32}(undef, 2)
    int32s[1] = Int32(1)
    int32s[2] = Int32(2)
    @assert typeof(int32s) === Vector{Int32}
    @assert eltype(int32s) === Int32
    @assert sizeof(int32s) == 8

    uint8s = Vector{UInt8}(undef, 4)
    uint8s[1] = UInt8(1)
    uint8s[2] = UInt8(2)
    uint8s[3] = UInt8(3)
    uint8s[4] = UInt8(4)
    @assert typeof(uint8s) === Vector{UInt8}
    @assert eltype(uint8s) === UInt8
    @assert sizeof(uint8s) == 4

    uint16s = Vector{UInt16}(undef, 3)
    uint16s[1] = UInt16(1)
    uint16s[2] = UInt16(2)
    uint16s[3] = UInt16(3)
    @assert typeof(uint16s) === Vector{UInt16}
    @assert eltype(uint16s) === UInt16
    @assert sizeof(uint16s) == 6

    float32s = Vector{Float32}(undef, 2)
    float32s[1] = Float32(1.0)
    float32s[2] = Float32(2.0)
    @assert sizeof(float32s) == 8

    bools = Bool[]
    push!(bools, true)
    push!(bools, false)
    push!(bools, true)
    @assert sizeof(bools) == 3

    chars = Char[]
    push!(chars, 'a')
    push!(chars, 'b')
    @assert sizeof(chars) == 8

    # Nothing has size 0
    @assert sizeof(nothing) == 0
    @assert sizeof(missing) == 0

    @test (true)
end

# ===== source: types/sizeof_logical_array_element_3908.jl =====

@testset "sizeof uses logical array element type (Issue #3908)" begin
    complex_values = zeros(Complex{Float64}, 2)

    @test sizeof(complex_values) == 32
    @test typeof(complex_values) == Vector{Complex{Float64}}

    empty_complex = Vector{Complex{Float64}}(undef, 0)

    @test sizeof(empty_complex) == 0
    @test typeof(empty_complex) == Vector{Complex{Float64}}
end

# ===== source: types/sizeof_value_narrow_type_6766.jl =====
# Issue #6766: sizeof(x) on a value must return the logical type size,
# not the boxed Value representation size (8). It should equal
# sizeof(typeof(x)) for every bits type.


@testset "sizeof(value) matches sizeof(typeof(value)) - Issue #6766" begin
    # Signed integers
    @assert sizeof(Int8(1)) == 1
    @assert sizeof(Int16(1)) == 2
    @assert sizeof(Int32(4)) == 4
    @assert sizeof(Int64(1)) == 8
    @assert sizeof(Int128(1)) == 16

    # Unsigned integers
    @assert sizeof(UInt8(1)) == 1
    @assert sizeof(UInt16(1)) == 2
    @assert sizeof(UInt32(1)) == 4
    @assert sizeof(UInt64(1)) == 8
    @assert sizeof(UInt128(1)) == 16

    # Floating point
    @assert sizeof(Float16(1.0)) == 2
    @assert sizeof(Float32(1.0f0)) == 4
    @assert sizeof(Float64(1.0)) == 8

    # Bool and Char
    @assert sizeof(true) == 1
    @assert sizeof(false) == 1
    @assert sizeof('a') == 4

    # The value version must agree with the type version for every bits type.
    @assert sizeof(Int8(1)) == sizeof(typeof(Int8(1)))
    @assert sizeof(Int16(1)) == sizeof(typeof(Int16(1)))
    @assert sizeof(Int32(4)) == sizeof(typeof(Int32(4)))
    @assert sizeof(Int64(1)) == sizeof(typeof(Int64(1)))
    @assert sizeof(Int128(1)) == sizeof(typeof(Int128(1)))
    @assert sizeof(UInt8(1)) == sizeof(typeof(UInt8(1)))
    @assert sizeof(UInt16(1)) == sizeof(typeof(UInt16(1)))
    @assert sizeof(UInt32(1)) == sizeof(typeof(UInt32(1)))
    @assert sizeof(UInt64(1)) == sizeof(typeof(UInt64(1)))
    @assert sizeof(UInt128(1)) == sizeof(typeof(UInt128(1)))
    @assert sizeof(Float16(1.0)) == sizeof(typeof(Float16(1.0)))
    @assert sizeof(Float32(1.0f0)) == sizeof(typeof(Float32(1.0f0)))
    @assert sizeof(Float64(1.0)) == sizeof(typeof(Float64(1.0)))
    @assert sizeof(true) == sizeof(typeof(true))
    @assert sizeof('a') == sizeof(typeof('a'))

    # sizeof(::Type) must stay correct (regression guard).
    @assert sizeof(Int8) == 1
    @assert sizeof(Int16) == 2
    @assert sizeof(Int32) == 4
    @assert sizeof(Int64) == 8
    @assert sizeof(Int128) == 16
    @assert sizeof(UInt8) == 1
    @assert sizeof(UInt16) == 2
    @assert sizeof(UInt32) == 4
    @assert sizeof(UInt64) == 8
    @assert sizeof(UInt128) == 16
    @assert sizeof(Float16) == 2
    @assert sizeof(Float32) == 4
    @assert sizeof(Float64) == 8
    @assert sizeof(Bool) == 1
    @assert sizeof(Char) == 4

    @test (true)
end

# ===== source: types/test_eltype_pure_julia.jl =====
# Test eltype function (Issue #2570)
# Verifies eltype works correctly for arrays and other types

@testset "eltype Pure Julia" begin
    # Array element types
    @test eltype([1, 2, 3]) == Int64
    @test eltype([1.0, 2.0, 3.0]) == Float64
    @test eltype([true, false]) == Bool

    # eltype returns DataType, verify with typeof
    @test typeof(eltype([1, 2, 3])) == DataType
end

# ===== source: types/test_index_style.jl =====
# Test IndexStyle abstract type and subtypes (IndexLinear, IndexCartesian)


@testset "IndexStyle types" begin
    # Test IndexStyle is an abstract type
    @test isabstracttype(IndexStyle)

    # Test IndexLinear and IndexCartesian exist
    @test isa(IndexLinear(), IndexLinear)
    @test isa(IndexCartesian(), IndexCartesian)

    # Test subtype relationships
    @test IndexLinear <: IndexStyle
    @test IndexCartesian <: IndexStyle

    # Test they are concrete types (not abstract)
    @test isconcretetype(IndexLinear)
    @test isconcretetype(IndexCartesian)
end

# ===== source: types/test_typejoin.jl =====
# Test typejoin function - compute smallest common supertype
# typejoin(A, B) walks both supertype chains to find the first common ancestor


@testset "typejoin - smallest common supertype" begin
    # Same type returns itself
    @test typejoin(Int64, Int64) === Int64
    @test typejoin(Float64, Float64) === Float64
    @test typejoin(String, String) === String

    # Numeric type hierarchy
    @test typejoin(Int64, Float64) === Real
    @test typejoin(Int64, UInt64) === Integer
    @test typejoin(Bool, Int64) === Integer
    @test typejoin(Bool, UInt8) === Integer

    # Unrelated types -> Any
    @test typejoin(Int64, String) === Any
    @test typejoin(Float64, String) === Any

    # Any with anything -> Any
    @test typejoin(Any, Int64) === Any
    @test typejoin(Int64, Any) === Any
    @test typejoin(Any, Any) === Any
end

# ===== source: types/typeof_type_name_string_9741.jl =====
# typeof on strings that look like type names must not create DataType values.


@testset "typeof type-name strings remain String - Issue #9741" begin
    @test typeof("Float64") === String
    @test typeof("Int64") === String
    @test typeof("Vector{Int64}") === String
    @test typeof("@NamedTuple{a::Int64}") === String

    @test typeof(Float64) === DataType
    @test typeof(Vector{Int64}) === DataType
    @test typeof(@NamedTuple{a::Int64}) === DataType

    bottom_type() = Union{}
    # Direct `Core.TypeofBottom` references are tracked separately (Issue #9967).
    @test string(typeof(Union{})) == "Core.TypeofBottom"
    @test string(typeof(bottom_type())) == "Core.TypeofBottom"
    @test typeof(Union{}) !== DataType
    @test typeof(bottom_type()) !== DataType
end

# ===== source: types/vector_matrix_alias_ndims_6814.jl =====
# Bare array aliases `Vector` (= `Array{T,1} where T`) and `Matrix`
# (= `Array{T,2} where T`) must keep their fixed rank in `isa` / `<:`.
#
# sjulia previously short-circuited `struct_params_are_subtype` to `true` for any
# array-family pair whose supertype was written without parameters, so a bare
# alias that PINS a rank (`Vector`/`Matrix`/`AbstractVector`/`AbstractMatrix`)
# was treated as rank-free. That made `Vector <: Matrix`, `Array{Int64,1} <:
# Matrix`, and `[1,2,3] isa Matrix` all spuriously true (Issue #6814).
#
# Only the genuinely rank-free names (`Array`/`AbstractArray`/`DenseArray`/
# `BitArray`) match any rank when written bare. All expectations below were
# verified against upstream Julia 1.12.


@testset "bare Vector/Matrix aliases keep ndims in isa/<: (Issue #6814)" begin
    # --- isa: a value's rank must match the bare alias's fixed rank ---
    @test [1, 2, 3] isa Vector
    @test !([1, 2, 3] isa Matrix)
    m = [1 2; 3 4]
    @test m isa Matrix
    @test !(m isa Vector)
    # Parameterized forms were already correct; keep them so.
    @test !(m isa Array{Int64,1})
    @test m isa Array{Int64,2}

    # --- type-level <:: bare alias rank is invariant ---
    @test !(Matrix{Int64} <: Vector)
    @test !(Vector <: Matrix)
    @test !(Matrix <: Vector)
    @test !(Array{Int64,1} <: Matrix)
    @test Array{Int64,2} <: Matrix
    @test Vector{Int64} <: Vector
    @test Matrix{Int64} <: Matrix

    # --- rank-free supertypes still match any rank when bare ---
    @test Vector <: Array
    @test Matrix <: Array
    @test Vector <: AbstractArray
    @test Matrix <: AbstractArray
    @test Array <: AbstractArray
    # ...but a rank-free type is NOT a subtype of a rank-pinned bare alias.
    @test !(Array <: Matrix)
    @test !(Array <: Vector)

    # --- abstract rank-pinned aliases keep their rank ---
    @test Vector{Int64} <: AbstractVector
    @test !(Vector{Int64} <: AbstractMatrix)
    @test Matrix{Int64} <: AbstractMatrix
    @test !(Matrix{Int64} <: AbstractVector)
end

true
