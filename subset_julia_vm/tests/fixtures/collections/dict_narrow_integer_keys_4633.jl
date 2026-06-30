using Test

@testset "Dict narrow integer keys (#4633)" begin
    d = Dict(Int8(1) => Int16(2), Int8(3) => Int16(4))

    @test typeof(d) === Dict{Int8, Int16}
    @test keytype(d) === Int8
    @test valtype(d) === Int16
    @test typeof(d[Int8(3)]) === Int16
    @test d[Int8(3)] == Int16(4)
    @test haskey(d, Int8(1))

    ks = collect(keys(d))
    @test length(ks) == 2
    @test eltype(ks) === Int8
    @test (ks[1] == Int8(1) || ks[2] == Int8(1))
    @test (ks[1] == Int8(3) || ks[2] == Int8(3))

    d[Int8(5)] = Int16(6)
    @test typeof(d[Int8(5)]) === Int16
    @test d[Int8(5)] == Int16(6)
    @test keytype(d) === Int8
    @test valtype(d) === Int16
end

true
