using Test

@testset "flatten narrow scalar collect (Issues #4018/#4666)" begin
    signed_values = collect(Base.Iterators.flatten((Int8(1), Int16(2))))
    @test typeof(signed_values) === Vector{Signed}
    @test eltype(signed_values) === Signed
    @test signed_values[1] == Int8(1)
    @test signed_values[2] == Int16(2)

    unsigned_values = collect(Base.Iterators.flatten((UInt8(1), UInt16(2))))
    @test typeof(unsigned_values) === Vector{Unsigned}
    @test eltype(unsigned_values) === Unsigned
    @test unsigned_values[1] == UInt8(1)
    @test unsigned_values[2] == UInt16(2)

    float_values = collect(Base.Iterators.flatten((Float16(1), Float32(2))))
    @test typeof(float_values) === Vector{AbstractFloat}
    @test eltype(float_values) === AbstractFloat
    @test float_values[1] == Float16(1)
    @test float_values[2] == Float32(2)
end

true
