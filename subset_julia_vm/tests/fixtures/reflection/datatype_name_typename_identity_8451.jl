using Test

module DataTypeNameA8451
struct T end
end

module DataTypeNameB8451
struct T end
end

@testset "DataType.name exposes TypeName identity (Issue #8451)" begin
    @test !(DataTypeNameA8451.T.name === DataTypeNameB8451.T.name)
    @test (Vector{Int}).name === (Vector{Float64}).name
end

true
