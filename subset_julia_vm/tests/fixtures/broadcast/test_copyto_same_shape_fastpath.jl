using Test

@testset "copyto! same-shape binary fast path" begin
    a = [1, 2, 3, 4]
    b = [10, 20, 30, 40]
    dest = [0, 0, 0, 0]
    bc = instantiate(broadcasted(+, a, b))

    applied = _copyto_fastpath_same_shape_binary!(dest, bc)
    @test applied == true
    @test dest == [11, 22, 33, 44]
end

@testset "copyto! same-shape fast path falls back on aliasing" begin
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    bc = instantiate(broadcasted(+, a, b))

    applied = _copyto_fastpath_same_shape_binary!(a, bc)
    @test applied == false
end

true
