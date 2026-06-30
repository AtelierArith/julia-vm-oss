# Issue #7269: indexed compound assignment used as RHS of another assignment.
using Test

@testset "compound assignment index as expression (Issue #7269)" begin
    a = [10, 20, 30]
    b = (a[2] += 5)
    @test b == 25
    @test a == [10, 25, 30]
end

true
