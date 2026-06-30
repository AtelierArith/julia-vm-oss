using Test

struct ArrayStructValtypeBox3908
    x::Int64
end

@testset "array struct valtype uses logical element metadata (Issue #3908)" begin
    values = [ArrayStructValtypeBox3908(1), ArrayStructValtypeBox3908(2)]

    @test eltype(values) === ArrayStructValtypeBox3908
    @test valtype(values) === ArrayStructValtypeBox3908
    @test typeof(values) === Vector{ArrayStructValtypeBox3908}
end

true
