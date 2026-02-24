using Test

# Test @. / @__dot__ macro (Issue #2547)

@testset "@. macro" begin
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]

    # @. with binary operators
    r1 = @. x + y
    @test r1[1] == 5.0
    @test r1[2] == 7.0
    @test r1[3] == 9.0

    r2 = @. x * y
    @test r2[1] == 4.0
    @test r2[2] == 10.0
    @test r2[3] == 18.0

    r3 = @. x - y
    @test r3[1] == -3.0

    # @. with function call: sqrt(x) → sqrt.(x)
    z = [1.0, 4.0, 9.0]
    r4 = @. sqrt(z)
    @test r4[1] == 1.0
    @test r4[2] == 2.0
    @test r4[3] == 3.0

    # @__dot__ explicit form
    r5 = @__dot__ x + y
    @test r5[1] == 5.0
    @test r5[2] == 7.0
    @test r5[3] == 9.0

    # @. with compound expression: x + y * 2
    r6 = @. x + y * 2.0
    # y .* 2.0 = [8.0, 10.0, 12.0], then x .+ result = [9.0, 12.0, 15.0]
    @test r6[1] == 9.0
    @test r6[2] == 12.0
    @test r6[3] == 15.0
end

true
