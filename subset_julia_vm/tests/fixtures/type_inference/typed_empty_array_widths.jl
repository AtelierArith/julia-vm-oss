# Test: Typed empty arrays preserve exact element widths (Issue #3532)
using Test

function int32_array_eltype()
    xs = Int32[]
    push!(xs, Int32(1))
    return eltype(xs)
end

function float32_array_eltype()
    xs = Float32[]
    push!(xs, 1.0f0)
    return eltype(xs)
end

function uint8_array_eltype()
    xs = UInt8[]
    push!(xs, 0x01)
    return eltype(xs)
end

@testset "Typed empty arrays preserve element widths" begin
    @test int32_array_eltype() == Int32
    @test float32_array_eltype() == Float32
    @test uint8_array_eltype() == UInt8
end

true
