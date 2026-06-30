# Issue #7269: simple-variable compound assignment as RHS of another assignment,
# and as a function argument.
using Test

@testset "compound assignment variable as expression (Issue #7269)" begin
    x = 0
    y = (x += 1)
    @test y == 1
    @test x == 1

    a = 10
    b = (a *= 3)
    @test b == 30
    @test a == 30
end

true
