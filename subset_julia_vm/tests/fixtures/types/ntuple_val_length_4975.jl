using Test

# Issue #4975: ntuple(f, Val(N)) must extract the numeric value parameter N
# from the Val{N} struct passed directly as the length argument.

sq_4975(i) = i * i
captured_ntuple_val_4975(a) = ntuple(i -> i + a, Val(3))

@testset "ntuple with Val length (Issue #4975)" begin
    @test ntuple(identity, Val(3)) == (1, 2, 3)
    @test ntuple(i -> i, Val(3)) == (1, 2, 3)
    @test ntuple(identity, Val{3}()) == (1, 2, 3)
    @test ntuple(sq_4975, Val(3)) == (1, 4, 9)
    @test ntuple(i -> i^2, Val(4)) == (1, 4, 9, 16)
    @test ntuple(identity, Val(0)) == ()
    @test ntuple(identity, Val(1)) == (1,)
    @test captured_ntuple_val_4975(10) == (11, 12, 13)
end

true
