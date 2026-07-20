using Test

@testset "Memory{Symbol} setindex preserves boxed Symbol values" begin
    m = Memory{Symbol}(undef, 2)
    m[1] = :x
    m[2] = :y

    @test eltype(m) == Symbol
    @test m[1] == :x
    @test m[2] == :y

    fill!(m, :z)
    @test m[1] == :z
    @test m[2] == :z
end

true
