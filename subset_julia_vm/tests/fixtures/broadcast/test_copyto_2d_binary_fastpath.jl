using Test

@testset "copyto! 2D binary fast path same-shape arrays" begin
    a = reshape([1, 2, 3, 4], 2, 2)
    b = reshape([10, 20, 30, 40], 2, 2)
    dest = reshape([0, 0, 0, 0], 2, 2)
    bc = instantiate(broadcasted(+, a, b))

    applied = _copyto_fastpath_2d_binary!(dest, bc)
    @test applied == true
    @test dest == reshape([11, 22, 33, 44], 2, 2)
end

@testset "copyto! 2D binary fast path with broadcasted singleton dims" begin
    row = reshape([10, 20, 30], 1, 3)
    col = reshape([1, 2], 2, 1)
    dest = reshape([0, 0, 0, 0, 0, 0], 2, 3)
    bc = instantiate(broadcasted(+, row, col))

    applied = _copyto_fastpath_2d_binary!(dest, bc)
    @test applied == true
    @test dest == reshape([11, 12, 21, 22, 31, 32], 2, 3)
end

@testset "copyto! 2D binary fast path with nested Broadcasted argument" begin
    row = reshape([1, 2, 3], 1, 3)
    col = reshape([1, 2], 2, 1)
    nested = broadcasted(*, 2, col)
    dest = reshape([0, 0, 0, 0, 0, 0], 2, 3)
    bc = instantiate(broadcasted(+, row, nested))

    applied = _copyto_fastpath_2d_binary!(dest, bc)
    @test applied == true
    @test dest == reshape([3, 5, 4, 6, 5, 7], 2, 3)
end

@testset "copyto! 2D binary fast path falls back on aliasing" begin
    a = reshape([1.0, 2.0, 3.0, 4.0], 2, 2)
    b = reshape([10.0, 20.0, 30.0, 40.0], 2, 2)
    bc = instantiate(broadcasted(+, a, b))

    applied = _copyto_fastpath_2d_binary!(a, bc)
    @test applied == false
end

true
