# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: reflection/fieldname_index_errors_5098.jl =====
module Agg_fieldname_index_errors_5098
# fieldname(T, i) by 1-based index on user-defined and parametric struct
# types, plus the upstream-matching error behavior for abstract types and
# out-of-range / non-positive indices (ArgumentError). Also re-checks that
# fieldnames / fieldcount stay correct (no regression to #3909 / #5099 /
# #5101). Matches upstream Julia Base.fieldname (Issue #5098).

using Test

struct FieldName5098Foo
    x::Int
    y::Float64
end

struct FieldName5098Box{T}
    val::T
end

struct FieldName5098Pair{A, B}
    first::A
    second::B
end

struct FieldName5098Single
    only::Int
end

abstract type FieldName5098Abstract end

@testset "fieldname by index on a plain struct (Issue #5098)" begin
    @test fieldname(FieldName5098Foo, 1) === :x
    @test fieldname(FieldName5098Foo, 2) === :y
    @test isa(fieldname(FieldName5098Foo, 1), Symbol)
end

@testset "fieldname on concrete parametric instantiations (Issue #5098)" begin
    @test fieldname(FieldName5098Box{Int}, 1) === :val
    @test fieldname(FieldName5098Box{Float64}, 1) === :val
    @test fieldname(FieldName5098Pair{Int, String}, 1) === :first
    @test fieldname(FieldName5098Pair{Int, String}, 2) === :second
end

@testset "fieldname out-of-range index throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Foo, 3)
    @test_throws ArgumentError fieldname(FieldName5098Single, 2)
end

@testset "fieldname non-positive index throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Foo, 0)
    @test_throws ArgumentError fieldname(FieldName5098Foo, -1)
end

@testset "fieldname on abstract type throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Abstract, 1)
end

@testset "fieldnames / fieldcount stay correct (Issue #5098 regression)" begin
    @test fieldnames(FieldName5098Foo) === (:x, :y)
    @test fieldcount(FieldName5098Foo) == 2
    @test fieldnames(FieldName5098Box{Int}) === (:val,)
    @test fieldcount(FieldName5098Box{Int}) == 1
    @test fieldnames(FieldName5098Pair{Int, String}) === (:first, :second)
    @test fieldcount(FieldName5098Pair{Int, String}) == 2
    # fieldname is consistent with fieldnames at every index.
    @test fieldname(FieldName5098Pair{Int, String}, 1) === fieldnames(FieldName5098Pair{Int, String})[1]
    @test fieldname(FieldName5098Pair{Int, String}, 2) === fieldnames(FieldName5098Pair{Int, String})[2]
end
end # module Agg_fieldname_index_errors_5098

# ===== source: reflection/fieldoffset_layout_3909.jl =====
module Agg_fieldoffset_layout_3909
using Test

struct FieldOffsetBits3909
    x::Int64
    y::Bool
    z::Int16
end

struct FieldOffsetRefs3909
    name::String
    flag::Bool
    value::Int64
end

mutable struct FieldOffsetMutable3909
    x::Int64
    y::Bool
end

@testset "fieldoffset uses runtime type layout metadata (Issue #3909)" begin
    @test fieldoffset(FieldOffsetBits3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetBits3909, 2) == UInt64(8)
    @test fieldoffset(FieldOffsetBits3909, 3) == UInt64(10)

    @test fieldoffset(FieldOffsetRefs3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetRefs3909, 2) == UInt64(8)
    @test fieldoffset(FieldOffsetRefs3909, 3) == UInt64(16)

    @test fieldoffset(FieldOffsetMutable3909, 1) == UInt64(0)
    @test fieldoffset(FieldOffsetMutable3909, 2) == UInt64(8)

    @test fieldoffset(LineNumberNode, 2) == UInt64(8)
    @test fieldoffset(GlobalRef, 3) == UInt64(16)
    @test typeof(fieldoffset(FieldOffsetBits3909, 1)) === UInt64
end
end # module Agg_fieldoffset_layout_3909

# ===== source: reflection/fieldtype_index_name_5099.jl =====
module Agg_fieldtype_index_name_5099
# fieldtype(T, i) / fieldtype(T, name) by 1-based index or Symbol name.
# Covers plain structs, concrete parametric instantiations, the bare
# UnionAll (type-variable upper bound), and out-of-range / bad-name errors,
# matching upstream Julia (Issue #5099).

using Test

struct FieldType5099Foo
    x::Int
    y::String
end

struct FieldType5099Box{T}
    val::T
end

struct FieldType5099Pair{A, B}
    first::A
    second::B
end

struct FieldType5099Bounded{T <: Number}
    v::T
end

@testset "fieldtype by index on a plain struct (Issue #5099)" begin
    @test fieldtype(FieldType5099Foo, 1) === Int
    @test fieldtype(FieldType5099Foo, 2) === String
    @test fieldtype(FieldType5099Foo, 1) === Int64
end

@testset "fieldtype by Symbol name on a plain struct (Issue #5099)" begin
    @test fieldtype(FieldType5099Foo, :x) === Int
    @test fieldtype(FieldType5099Foo, :y) === String
end

@testset "fieldtype on concrete parametric instantiations (Issue #5099)" begin
    @test fieldtype(FieldType5099Box{Int}, 1) === Int
    @test fieldtype(FieldType5099Box{Int}, :val) === Int
    @test fieldtype(FieldType5099Box{Float64}, 1) === Float64
    @test fieldtype(FieldType5099Pair{Int, String}, 1) === Int
    @test fieldtype(FieldType5099Pair{Int, String}, 2) === String
    @test fieldtype(FieldType5099Pair{Int, String}, :second) === String
end

@testset "fieldtype consistent with fieldtypes (Issue #5099)" begin
    @test fieldtype(FieldType5099Foo, 1) === fieldtypes(FieldType5099Foo)[1]
    @test fieldtype(FieldType5099Foo, 2) === fieldtypes(FieldType5099Foo)[2]
    @test fieldtype(FieldType5099Box{Int}, 1) === fieldtypes(FieldType5099Box{Int})[1]
end

@testset "fieldtype on bare UnionAll uses the type-var bound (Issue #5099)" begin
    # Unbounded type variable -> Any.
    @test fieldtype(FieldType5099Box, 1) === Any
    @test fieldtype(FieldType5099Box, :val) === Any
    @test fieldtypes(FieldType5099Box) === (Any,)
    # Bounded type variable -> its upper bound.
    @test fieldtype(FieldType5099Bounded, 1) === Number
    @test fieldtypes(FieldType5099Bounded) === (Number,)
end

@testset "fieldtype out-of-range index throws BoundsError (Issue #5099)" begin
    @test_throws BoundsError fieldtype(FieldType5099Foo, 3)
    @test_throws BoundsError fieldtype(FieldType5099Foo, 0)
end

@testset "fieldtype bad field name throws (Issue #5099)" begin
    @test_throws Exception fieldtype(FieldType5099Foo, :z)
end
end # module Agg_fieldtype_index_name_5099

# ===== source: reflection/getfield_basic.jl =====
module Agg_getfield_basic
# Test getfield() for field access by name (Symbol) and index (Int)

using Test

# Define test structs outside @testset
struct Point
    x::Float64
    y::Float64
end

struct MyStruct
    a::Int64
    b::String
    c::Tuple{Int64, Int64}
end

@testset "getfield() basic functionality" begin
    # Test getfield with Symbol for struct
    p = Point(1.0, 2.0)
    @test getfield(p, :x) == 1.0
    @test getfield(p, :y) == 2.0

    # Test getfield with integer index for struct (1-based)
    @test getfield(p, 1) == 1.0
    @test getfield(p, 2) == 2.0

    # Test getfield with more complex struct
    s = MyStruct(42, "hello", (1, 2))
    @test getfield(s, :a) == 42
    @test getfield(s, :b) == "hello"
    @test getfield(s, :c) == (1, 2)

    # Test getfield with integer index for complex struct
    @test getfield(s, 1) == 42
    @test getfield(s, 2) == "hello"
    @test getfield(s, 3) == (1, 2)

    # Test getfield with tuple (integer index only)
    t = (10, 20, 30)
    @test getfield(t, 1) == 10
    @test getfield(t, 2) == 20
    @test getfield(t, 3) == 30
end
end # module Agg_getfield_basic

# ===== source: reflection/hasfield_hasproperty_propertynames_5101.jl =====
module Agg_hasfield_hasproperty_propertynames_5101
# Test hasfield / hasproperty / propertynames for user types (Issue #5101)
#
# Upstream Julia (base/runtime_internals.jl):
#   hasfield(T::Type, name::Symbol) = fieldindex(T, name, false) > 0
#   propertynames(x) = fieldnames(typeof(x))
#   hasproperty(x, s::Symbol) = s in propertynames(x)
#
# The key behavior verified here is that `hasproperty` routes through the
# (overridable) `propertynames`, so a custom `propertynames` overload is
# honored even for property names that are not real fields.
#
# A single flat @testset is used so the testset summary matches upstream
# Test.jl for the fixture parity check (scripts/fixture_julia_parity.sh).

using Test

struct Foo
    x::Int
    y::Float64
end

struct Box{T}
    val::T
end

struct Empty
end

# Custom propertynames overload: hasproperty must honor it (Issue #5101 points 2/4)
struct Custom
    a::Int
    b::Int
end
Base.propertynames(::Custom) = (:a, :b, :virtual)

@testset "hasfield / hasproperty / propertynames (Issue #5101)" begin
    # plain struct
    foo = Foo(1, 2.0)
    @test hasfield(Foo, :x) === true
    @test hasfield(Foo, :y) === true
    @test hasfield(Foo, :z) === false
    @test propertynames(foo) === (:x, :y)
    @test hasproperty(foo, :x) === true
    @test hasproperty(foo, :y) === true
    @test hasproperty(foo, :z) === false

    # parametric struct
    b = Box{Int}(5)
    @test hasfield(Box{Int}, :val) === true
    @test hasfield(Box{Int}, :missing) === false
    @test propertynames(b) === (:val,)
    @test hasproperty(b, :val) === true
    @test hasproperty(b, :nope) === false

    # empty struct
    e = Empty()
    @test hasfield(Empty, :anything) === false
    @test propertynames(e) === ()
    @test hasproperty(e, :x) === false

    # custom propertynames overload honored by hasproperty
    c = Custom(1, 2)
    @test propertynames(c) === (:a, :b, :virtual)
    @test hasproperty(c, :a) === true
    @test hasproperty(c, :virtual) === true    # not a real field, but a property
    @test hasfield(Custom, :virtual) === false # still not a field
    @test hasproperty(c, :missing) === false
end
end # module Agg_hasfield_hasproperty_propertynames_5101

# ===== source: reflection/property_functions.jl =====
module Agg_property_functions
# Test property functions: getproperty, setproperty!, propertynames
# Issue #1450: Implement property functions
# Issue #1451: Implement setfield! builtin

using Test

struct ImmutablePoint
    x::Float64
    y::Float64
end

mutable struct MutablePoint
    x::Float64
    y::Float64
end

@testset "Property functions" begin
    @testset "getproperty" begin
        p = ImmutablePoint(1.0, 2.0)

        # getproperty should return field values
        @test getproperty(p, :x) == 1.0
        @test getproperty(p, :y) == 2.0

        # should be equivalent to p.x
        @test p.x == getproperty(p, :x)
        @test p.y == getproperty(p, :y)
    end

    @testset "setfield!" begin
        # Test setfield! builtin function
        p = MutablePoint(1.0, 2.0)

        # setfield! by Symbol
        setfield!(p, :x, 3.0)
        @test p.x == 3.0

        # setfield! by index
        setfield!(p, 2, 4.0)
        @test p.y == 4.0

        # setfield! returns the assigned value
        result = setfield!(p, :x, 5.0)
        @test result == 5.0
        @test p.x == 5.0
    end

    @testset "setproperty!" begin
        p = MutablePoint(1.0, 2.0)

        # setproperty! should modify field values
        setproperty!(p, :x, 3.0)
        @test p.x == 3.0

        setproperty!(p, :y, 4.0)
        @test p.y == 4.0

        # should be equivalent to p.x = value
        p.x = 5.0
        @test getproperty(p, :x) == 5.0
    end

    @testset "setproperty! with type conversion" begin
        p = MutablePoint(1.0, 2.0)

        # setproperty! should convert Int to Float64
        setproperty!(p, :x, 10)
        @test p.x == 10.0
        @test typeof(p.x) == Float64
    end

    @testset "propertynames" begin
        p = ImmutablePoint(1.0, 2.0)

        # propertynames should return tuple of field names
        pnames = propertynames(p)
        @test length(pnames) == 2
        # Note: fieldnames returns strings in SubsetJuliaVM, so convert to Symbol for comparison
        @test Symbol(pnames[1]) == :x
        @test Symbol(pnames[2]) == :y
    end

    @testset "hasproperty with propertynames" begin
        p = ImmutablePoint(1.0, 2.0)

        # hasproperty should work with property names
        @test hasproperty(p, :x) == true
        @test hasproperty(p, :y) == true
        @test hasproperty(p, :z) == false
    end
end
end # module Agg_property_functions

# ===== source: reflection/user_struct_layout_metadata_3909.jl =====
module Agg_user_struct_layout_metadata_3909
using Test

struct RegistryLayout3909
    x::Int64
    y::Bool
end

mutable struct RegistryMutable3909
    x::Int64
end

struct RegistryReference3909
    name::String
    x::Int64
end

@testset "user struct layout metadata is registry-backed (Issue #3909)" begin
    @test fieldnames(RegistryLayout3909) == (:x, :y)
    @test fieldtypes(RegistryLayout3909) == (Int64, Bool)
    @test fieldcount(RegistryLayout3909) == 2
    @test isbitstype(RegistryLayout3909)
    @test sizeof(RegistryLayout3909) == 16

    @test fieldnames(RegistryMutable3909) == (:x,)
    @test fieldtypes(RegistryMutable3909) == (Int64,)
    @test fieldcount(RegistryMutable3909) == 1
    @test !isbitstype(RegistryMutable3909)
    @test sizeof(RegistryMutable3909) == 8

    @test fieldnames(RegistryReference3909) == (:name, :x)
    @test fieldtypes(RegistryReference3909) == (String, Int64)
    @test !isbitstype(RegistryReference3909)
    @test sizeof(RegistryReference3909) == 16
end
end # module Agg_user_struct_layout_metadata_3909

true
