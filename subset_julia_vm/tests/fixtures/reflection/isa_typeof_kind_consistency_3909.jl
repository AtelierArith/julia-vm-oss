using Test

struct IsaKindBox3909{T}
    x::T
end

@testset "isa agrees with typeof for runtime type object kinds (Issue #3909)" begin
    # DataType-kind values: concrete and parametric concrete types
    @test isa(Int64, DataType)
    @test isa(Vector{Int64}, DataType)
    @test isa(IsaKindBox3909{Int64}, DataType)
    @test !isa(Int64, UnionAll)
    @test !isa(Vector{Int64}, UnionAll)

    # UnionAll-kind values: parametric type schemas
    @test isa(Vector, UnionAll)
    @test isa(Dict, UnionAll)
    @test isa(IsaKindBox3909, UnionAll)
    @test !isa(Vector, DataType)
    @test !isa(Dict, DataType)
    @test !isa(IsaKindBox3909, DataType)

    # TypeVar-kind values
    @test isa(TypeVar(:T), TypeVar)
    @test !isa(TypeVar(:T), DataType)
    @test !isa(TypeVar(:T), UnionAll)

    # Both DataType and UnionAll are subtypes of Type
    @test isa(Int64, Type)
    @test isa(Vector, Type)
    @test isa(Vector{Int64}, Type)
    @test isa(Dict, Type)
    # TypeVar is not a subtype of Type in Julia
    @test !isa(TypeVar(:T), Type)
end

@testset "Base.unwrap_unionall iterates through UnionAll bodies (Issue #3909)" begin
    @test isa(Base.unwrap_unionall(Vector), DataType)
    @test isa(Base.unwrap_unionall(Dict), DataType)
    @test isa(Base.unwrap_unionall(IsaKindBox3909), DataType)

    # Non-UnionAll inputs are returned unchanged
    @test Base.unwrap_unionall(Int64) === Int64
    @test Base.unwrap_unionall(Vector{Int64}) === Vector{Int64}
end

true
