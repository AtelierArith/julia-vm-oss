using Test

@testset "count predicate over Memory direct storage" begin
    m = Memory{Int64}(undef, 4)
    m[1] = 1
    m[2] = 2
    m[3] = 3
    m[4] = 4

    @test count(x -> x > 2, m) == 2
    @test count(x -> x isa Int64, m) == 4

    empty = Memory{Int64}(undef, 0)
    @test count(x -> true, empty) == 0

    mf = Memory{Float64}(undef, 3)
    mf[1] = 1.0
    mf[2] = 2.5
    mf[3] = 3.5
    @test count(x -> x > 2.0, mf) == 2
end

true
