using Test

@testset "copyto! array-scalar fast path with Complex scalar" begin
    a = [1.0, 2.0, 3.0]
    dest = [Complex(0.0, 0.0), Complex(0.0, 0.0), Complex(0.0, 0.0)]
    bc = instantiate(broadcasted(*, im, a))

    applied = _copyto_fastpath_array_scalar!(dest, bc)
    @test applied == true
    @test dest[1] == Complex(0.0, 1.0)
    @test dest[2] == Complex(0.0, 2.0)
    @test dest[3] == Complex(0.0, 3.0)
end

@testset "copyto! array-scalar fast path with Ref scalar" begin
    C = [Complex(1.0, 0.0), Complex(0.0, 1.0), Complex(2.0, -1.0)]
    dest = [0, 0, 0]
    maxiter = 7
    f(c, m) = m + (abs2(c) > 1.0 ? 1 : 0)
    bc = instantiate(broadcasted(f, C, Ref(maxiter)))

    applied = _copyto_fastpath_array_scalar!(dest, bc)
    @test applied == true
    @test dest == [7, 7, 8]
end

true
