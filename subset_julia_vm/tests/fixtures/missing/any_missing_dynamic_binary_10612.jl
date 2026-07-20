# Runtime missing propagation through Any-typed values (Issues #10612/#10693).

using Test

@testset "Any-carried missing dynamic binary propagation" begin
    a = Any[missing, 1]

    @test ismissing(a[1] == a[1])
    @test ismissing(a[1] == a[2])
    @test ismissing(a[2] == a[1])
    @test ismissing(a[1] != a[1])
    @test ismissing(a[1] != a[2])
    @test ismissing(a[2] != a[1])

    @test ismissing(a[1] < a[2])
    @test ismissing(a[2] < a[1])
    @test ismissing(a[1] <= a[2])
    @test ismissing(a[2] <= a[1])
    @test ismissing(a[1] > a[2])
    @test ismissing(a[2] > a[1])
    @test ismissing(a[1] >= a[2])
    @test ismissing(a[2] >= a[1])

    @test ismissing(a[1] + a[2])
    @test ismissing(a[2] + a[1])
    @test ismissing(a[1] - a[2])
    @test ismissing(a[2] - a[1])
    @test ismissing(a[1] * a[2])
    @test ismissing(a[2] * a[1])
    @test ismissing(a[1] / a[2])
    @test ismissing(a[2] / a[1])
    @test ismissing(a[1] ÷ a[2])
    @test ismissing(a[2] ÷ a[1])
    @test ismissing(a[1] % a[2])
    @test ismissing(a[2] % a[1])
    @test ismissing(a[1] ^ a[2])
    @test ismissing(a[2] ^ a[1])

    @test isequal(a[1], a[1])
    @test !isequal(a[1], a[2])
end

true
