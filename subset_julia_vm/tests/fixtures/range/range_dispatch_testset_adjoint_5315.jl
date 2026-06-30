# Range dispatch regression: @testset cache pollution + adjoint(::AbstractRange) (Issue #5315)
#
# When a VM-native range (StepRangeLen from a colon / range(...)) flows into a
# call site previously specialized — in another @testset — to a LinRange/
# StepRangeLen *struct* method, first/length/collect must re-dispatch on the
# runtime value type instead of mis-dispatching into the struct method
# (GetField errors). adjoint/transpose of a VM-native range must materialize a
# 1xN row. Resolved on current main; this fixture locks the behavior in.

using Test

@testset "LinRange struct then VM-native range (cache pollution)" begin
    rs = LinRange(1.0, 10.0, 5)          # pure-Julia LinRange struct
    @test first(rs) == 1.0
    @test length(rs) == 5

    rv = 1.0 : 0.5 : 2.5                  # VM-native StepRangeLen range value
    @test first(rv) == 1.0
    @test length(rv) == 4
    @test collect(rv) == [1.0, 1.5, 2.0, 2.5]
end

@testset "adjoint/transpose of a range(...) result" begin
    r = range(1.0, step=0.5, length=4)   # VM-native StepRangeLen
    a = adjoint(r)
    @test size(a) == (1, 4)
    @test a[1, 2] == 1.5
    t = transpose(r)
    @test size(t) == (1, 4)
end

true
