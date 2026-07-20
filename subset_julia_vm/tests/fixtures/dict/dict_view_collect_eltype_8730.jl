using Test

@testset "collect(keys/values(::Dict)) preserves view eltype (Issue #8730)" begin
    d = Dict(:x => 10)

    ks = collect(keys(d))
    vs = collect(values(d))

    @test ks == [:x]
    @test vs == [10]
    @test eltype(ks) === Symbol
    @test eltype(vs) === Int64
    @test string(ks) == "[:x]"
    @test string(vs) == "[10]"
end

true
