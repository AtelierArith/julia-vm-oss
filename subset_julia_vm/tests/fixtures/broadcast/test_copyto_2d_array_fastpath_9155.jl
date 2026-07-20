using Test

# Regression coverage for the 2D binary broadcast fast path (Issue #9155):
# `xs' .+ ys`-style "outer" broadcasts (a row Array combined with a column
# Array) previously re-derived each operand's shape via `size(...)` on every
# output cell inside `_copyto_fastpath_2d_binary!`'s generic loop, which
# dominated wall time once the grid reached tens of thousands of cells (the
# Mandelbrot-grid construction pattern `xs' .+ im .* ys`). The fast loop below
# hoists that shape bookkeeping outside the loop for plain-Array operands.

@testset "copyto! 2D outer broadcast fast path: row .+ col" begin
    xs = collect(1.0:4.0)
    ys = collect(1.0:3.0)
    C = xs' .+ ys
    @test size(C) == (3, 4)
    for i in 1:3, j in 1:4
        @test C[i, j] == xs[j] + ys[i]
    end
end

@testset "copyto! 2D outer broadcast fast path: -, *, /" begin
    xs = collect(1.0:4.0)
    ys = collect(1.0:3.0)
    @test (xs' .- ys) == [xs[j] - ys[i] for i in 1:3, j in 1:4]
    @test (xs' .* ys) == [xs[j] * ys[i] for i in 1:3, j in 1:4]
    @test (xs' ./ ys) == [xs[j] / ys[i] for i in 1:3, j in 1:4]
end

@testset "copyto! 2D outer broadcast fast path: same-shape 2D (no expansion)" begin
    A = [1.0 2.0; 3.0 4.0]
    B = [10.0 20.0; 30.0 40.0]
    @test (A .+ B) == [11.0 22.0; 33.0 44.0]
end

@testset "copyto! 2D outer broadcast fast path: ComplexF64 output" begin
    xs = collect(1.0:4.0)
    ys = ComplexF64[1.0im, 2.0im, 3.0im]
    C = xs' .+ ys
    @test size(C) == (3, 4)
    @test C[1, 1] == 1.0 + 1.0im
    @test C[3, 4] == 4.0 + 3.0im
end

@testset "copyto! 2D outer broadcast fast path: nested Broadcasted operand" begin
    # Mirrors the Mandelbrot-grid construction pattern `xs' .+ im .* ys`
    # (Issue #9155): `im .* ys` stays an unmaterialized Broadcasted until the
    # outer `.+` forces it. The fast path materializes it once instead of
    # re-evaluating it per output cell.
    xs = collect(1.0:4.0)
    ys = collect(1.0:3.0)
    C = xs' .+ im .* ys
    @test size(C) == (3, 4)
    for i in 1:3, j in 1:4
        @test C[i, j] == xs[j] + im * ys[i]
    end
end

true
