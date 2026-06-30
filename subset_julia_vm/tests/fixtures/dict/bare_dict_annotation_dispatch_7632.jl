using Test

@testset "Pure-Julia Dict matches bare Dict annotations (Issue #7632)" begin
    f(d::Dict) = (length(d), d[:a])

    d = Dict(:a => 42)
    any_d = Any[d][1]

    @test f(d) == (1, 42)
    @test f(any_d) == (1, 42)
end

true
