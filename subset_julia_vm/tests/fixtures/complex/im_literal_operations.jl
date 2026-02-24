# Test Complex{Int64} operations with im literal syntax (Issue #920)
# This tests that 1+2im creates Complex{Int64} and operations work correctly.

using Test

@testset "Complex{Int64} with im literal syntax" begin
    # Test im literal creates complex numbers
    z = 1 + 2im
    @test typeof(z) == Complex{Int64}
    @test z.re == 1
    @test z.im == 2

    # Test adjoint works on Complex{Int64}
    w = adjoint(z)
    @test typeof(w) == Complex{Int64}
    @test w.re == 1
    @test w.im == -2

    # Test double adjoint returns original values
    x = w'
    @test typeof(x) == Complex{Int64}
    @test x.re == z.re
    @test x.im == z.im

    # Test arithmetic operations on Complex{Int64}
    z2 = 3 + 4im
    sum = z + z2
    @test sum.re == 4
    @test sum.im == 6

    diff = z2 - z
    @test diff.re == 2
    @test diff.im == 2
end

true
