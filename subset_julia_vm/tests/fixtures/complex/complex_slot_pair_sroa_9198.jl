# Complex{Float64} slot-pair SROA (Issue #9198 S2): a proven-ComplexF64 loop
# local is unboxed into two f64 re/im slots. Every case here is a shape the SROA
# pass transforms (z = z*z + c, real/imag/abs2, z.re/z.im, +=, subtraction,
# real*complex, conj, materialization at escapes: return / push! / capture).
# Values must be byte-identical to boxed Complex arithmetic (upstream Julia).
using Test

# z = z*z + c accumulation loop, read out via real/imag.
function accum(n::Int64)::Float64
    z = Complex{Float64}(0.1, 0.2)
    c = Complex{Float64}(0.1, 0.2)
    i = 0
    while i < n
        z = z * z + c
        i = i + 1
    end
    real(z) + imag(z)
end

# Mandelbrot escape-time: abs2 (as real*real + imag*imag) condition + z*z+c.
function mandel(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    c = Complex{Float64}(cr, ci)
    z = Complex{Float64}(0.0, 0.0)
    iter = 0
    while iter < maxiter && real(z) * real(z) + imag(z) * imag(z) <= 4.0
        z = z * z + c
        iter = iter + 1
    end
    iter
end

# abs2(z) reduction form.
function mandel_abs2(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    c = Complex{Float64}(cr, ci)
    z = Complex{Float64}(0.0, 0.0)
    iter = 0
    while iter < maxiter && abs2(z) <= 4.0
        z = z * z + c
        iter = iter + 1
    end
    iter
end

# += accumulation (AddAssign on a ComplexF64 local).
function accsum(n::Int64)::Float64
    s = Complex{Float64}(0.0, 0.0)
    k = 1
    while k <= n
        s += Complex{Float64}(Float64(k), Float64(-k))
        k = k + 1
    end
    real(s) + imag(s)
end

# Escape: return the boxed Complex mid-loop (materialization at boundary).
function firstbig(n::Int64)
    z = Complex{Float64}(0.4, 0.4)
    c = Complex{Float64}(0.1, 0.1)
    i = 0
    while i < n
        z = z * z + c
        if abs2(z) > 1.0
            return z
        end
        i = i + 1
    end
    z
end

# Escape: push! the SROA'd value into an array (materialization). Seeded array
# so it is a concrete ComplexF64 Vector.
function collectz(n::Int64)
    zs = [Complex{Float64}(0.0, 0.0)]
    z = Complex{Float64}(1.0, 0.0)
    c = Complex{Float64}(0.1, -0.2)
    i = 0
    while i < n
        z = z * z + c
        push!(zs, z)
        i = i + 1
    end
    zs
end

# Capture: a closure over the ComplexF64 local forces SROA to bail (stay boxed);
# capture happens after the loop so the value is the final z in both semantics.
function withcapture(n::Int64)::Float64
    z = Complex{Float64}(0.2, 0.3)
    c = Complex{Float64}(0.1, 0.1)
    i = 0
    while i < n
        z = z * z + c
        i = i + 1
    end
    g = () -> real(z) + imag(z)
    g()
end

# Subtraction, real*complex scaling, conjugation, field access.
function mixed()::Float64
    z = Complex{Float64}(1.5, -2.5)
    w = Complex{Float64}(0.5, 0.5)
    a = z - w
    b = 2.0 * z
    cc = conj(z)
    real(a) + imag(a) + real(b) + imag(b) + real(cc) + imag(cc) + z.re + z.im
end

@testset "SROA accumulation z=z*z+c" begin
    @test accum(10) == 0.27642085798435984
    @test accum(0) == 0.30000000000000004
end

@testset "SROA mandelbrot iteration counts" begin
    @test mandel(-0.5, 0.5, 50) == 50
    @test mandel(0.3, 0.6, 50) == 15
    @test mandel(2.0, 2.0, 50) == 1
    @test mandel_abs2(-0.5, 0.5, 50) == 50
    @test mandel_abs2(0.3, 0.6, 50) == 15
end

@testset "SROA += accumulation" begin
    @test accsum(5) == 15.0 - 15.0
end

@testset "SROA escape: return boxed value" begin
    r = firstbig(20)
    @test r isa Complex{Float64}
    @test real(r) == 0.09362728703444148
    @test imag(r) == 0.12303975741855343
end

@testset "SROA escape: push! into array" begin
    zs = collectz(3)
    @test length(zs) == 4
    @test zs[2] == Complex{Float64}(1.1, -0.2)
    @test real(zs[4]) == 1.3033000000000006
end

@testset "SROA escape: closure capture bails to boxed" begin
    @test withcapture(5) == 0.21740135824528822
end

@testset "SROA subtraction / scale / conj / fields" begin
    @test mixed() == -1.0
end

true
