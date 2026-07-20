using Test

@testset "primitive operator routes survive promote-fallback guards (Issue #9677)" begin
    pow = ^
    @test pow(2, 2) === 4
    @test [2, 3] .^ [2, 2] == [4, 9]

    int8_sum = Int8[1, 2] .+ Int8[3, 4]
    @test int8_sum == Int8[4, 6]
    @test eltype(int8_sum) === Int8

    @test mod(Float16(3), Float16(2)) === Float16(1)
    @test rem(Float32(3), Float32(2)) === Float32(1)

    @test !(pi < pi)
    @test pi <= pi
end

true
