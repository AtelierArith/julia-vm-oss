# Test im broadcast with scalar * array (Issue #1904 regression test)
# Previously, `im .* array` returned Vector{Float64} with all zeros
# because call_function_sync terminated early on nested function returns.

using Test

@testset "im broadcast scalar * array" begin
    # im .* Float64 array should produce Complex{Float64} array
    result = im .* [1.0, 2.0, 3.0]
    @test length(result) == 3
    @test real(result[1]) == 0.0
    @test imag(result[1]) == 1.0
    @test real(result[2]) == 0.0
    @test imag(result[2]) == 2.0
    @test real(result[3]) == 0.0
    @test imag(result[3]) == 3.0

    # im .+ Float64 array should produce Complex{Float64} array
    result2 = im .+ [1.0, 2.0, 3.0]
    @test real(result2[1]) == 1.0
    @test imag(result2[1]) == 1.0
    @test real(result2[2]) == 2.0
    @test imag(result2[2]) == 1.0

    # Complex{Float64} scalar .* Float64 array
    c = Complex{Float64}(0.0, 1.0)
    result3 = c .* [1.0, 2.0]
    @test real(result3[1]) == 0.0
    @test imag(result3[1]) == 1.0
    @test real(result3[2]) == 0.0
    @test imag(result3[2]) == 2.0

    # Float64 array .+ im (broadcast with scalar on right)
    result4 = [1.0, 2.0] .+ im
    @test real(result4[1]) == 1.0
    @test imag(result4[1]) == 1.0
    @test real(result4[2]) == 2.0
    @test imag(result4[2]) == 1.0

    # 2D broadcast: xs' .+ im .* ys (core of Mandelbrot grid construction)
    xs = [0.0, 1.0]
    ys = [0.0, 0.5]
    iys = im .* ys
    @test real(iys[1]) == 0.0
    @test imag(iys[1]) == 0.0
    @test real(iys[2]) == 0.0
    @test imag(iys[2]) == 0.5
end

true
