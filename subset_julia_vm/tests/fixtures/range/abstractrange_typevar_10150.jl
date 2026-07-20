using Test

range_eltype_10150(r::AbstractRange{T}) where {T} = T

@testset "AbstractRange{T} binds T for native and struct-backed ranges (Issue #10150)" begin
    @test range_eltype_10150(1:3) == Int64
    @test range_eltype_10150(1:2:5) == Int64
    @test range_eltype_10150(UnitRange(1, 3)) == Int64
    @test range_eltype_10150(StepRange(1, 2, 5)) == Int64
    @test range_eltype_10150(big(1):2:big(5)) == BigInt
    @test range_eltype_10150(UInt8(1):UInt16(3)) == UInt16
    @test range_eltype_10150(Char(97):Char(99)) == Char
    @test range_eltype_10150(Float32(0):Float32(0.5):Float32(1)) == Float32
end

true
