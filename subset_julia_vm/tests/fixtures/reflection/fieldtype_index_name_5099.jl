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

true
