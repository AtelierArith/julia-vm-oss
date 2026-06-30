using Test

function dynamic_eltype(x)
    eltype(x)
end

function dynamic_type_eltype(T)
    eltype(T)
end

@testset "number eltype dynamic dispatch (Issue #4665)" begin
    @test eltype(1) === Int64
    @test dynamic_eltype(1) === Int64
    @test dynamic_eltype(Int8(1)) === Int8
    @test dynamic_eltype(UInt8(1)) === UInt8
    @test dynamic_eltype(1.0) === Float64
    @test dynamic_eltype(Float32(1)) === Float32
    @test dynamic_eltype(true) === Bool

    @test eltype(Int64) === Int64
    @test dynamic_type_eltype(Int64) === Int64
    @test dynamic_type_eltype(Int8) === Int8
    @test dynamic_type_eltype(Float64) === Float64
end

true
