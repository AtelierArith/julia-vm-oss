# Issue #8240: two SubArray views with equal elements must compare element-wise.
# The runtime values already have the right SubArray type; this guards the
# compiler path that must infer `view(...)` as a concrete AbstractArray subtype
# so `==` routes to array equality instead of generic identity equality.

using Test

@testset "SubArray view equality (#8240)" begin
    w = view([1, 2, 3, 4], 1:3)
    w2 = view([0, 1, 2, 3], 2:4)
    @test w == w2
    @test !(w == view([1, 2, 4, 4], 1:3))
    @test w == [1, 2, 3]
    @test [1, 2, 3] == w

    fw = view([1.0, 2.0, 3.0, 4.0], 1:3)
    fw2 = view([0.0, 1.0, 2.0, 3.0], 2:4)
    @test fw == fw2

    bw = view([true, false, true, false], 1:3)
    bw2 = view([false, true, false, true], 2:4)
    @test bw == bw2
end

true
