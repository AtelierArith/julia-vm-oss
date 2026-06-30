using Test

struct RuntimeKindBox3909{T}
    x::T
end

@testset "runtime type object kind typeof (Issue #3909)" begin
    @test typeof(Int64) === DataType
    @test typeof(Vector{Int64}) === DataType
    @test typeof(RuntimeKindBox3909{Int64}) === DataType

    @test typeof(Vector) === UnionAll
    @test typeof(Dict) === UnionAll
    @test typeof(RuntimeKindBox3909) === UnionAll

    @test typeof(Vector.var) === TypeVar
    @test typeof(TypeVar(:T)) === TypeVar

    @test Vector.var === Vector.body.parameters[1]
end

true
