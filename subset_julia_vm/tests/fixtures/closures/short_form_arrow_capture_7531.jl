using Test

makepred_7531(x) = y -> y == x

@testset "short-form arrow closure captures outer parameter (Issue #7531)" begin
    pred = makepred_7531(2)
    @test pred(2)
    @test !pred(3)
end

true
