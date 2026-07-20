# Issue #9659: `x::Complex .* r` on a TwicePrecision-backed Float64 range must
# reproduce upstream's range broadcast fusion (julia/base/broadcast.jl:1169):
# ref/step are scaled in twice precision and elements come from the scaled
# complex lerp (`unsafe_getindex`), which differs by 1ulp from the elementwise
# `x * r[i]` on a large fraction of points (683,400 / 2,312,000 imag parts of
# the 1700×1360 Mandelbrot grid — a 195-count escape-checksum divergence).
#
# The expected bit patterns below are julia 1.12.6 ground truth; the parity
# check runs this file under both runtimes.

using Test

@testset "im .* TwicePrecision range is upstream-bit-identical (Issue #9659)" begin
    ys = range(1.2, -1.2; length=1360)
    imys = im .* ys

    @test length(imys) == 1360

    # The signature 1ulp point: imys[11]'s imag differs from ys[11] (the
    # elementwise product `im * ys[11]` keeps ys[11]'s bits exactly — that was
    # the bug).
    @test reinterpret(UInt64, ys[11]) == 0x3ff2eadd4d3211dc
    @test reinterpret(UInt64, imag(imys[11])) == 0x3ff2eadd4d3211db

    # Sampled elements (re, im) bit patterns.
    @test reinterpret(UInt64, real(imys[1])) == 0x0000000000000000
    @test reinterpret(UInt64, imag(imys[1])) == 0x3ff3333333333333
    @test reinterpret(UInt64, real(imys[680])) == 0x0000000000000000
    @test reinterpret(UInt64, imag(imys[680])) == 0x3f4cef28cd408970
    @test reinterpret(UInt64, real(imys[681])) == 0x0000000000000000
    @test reinterpret(UInt64, imag(imys[681])) == 0xbf4cef28cd408970
    @test reinterpret(UInt64, real(imys[1360])) == 0x0000000000000000
    @test reinterpret(UInt64, imag(imys[1360])) == 0xbff3333333333333

    # Whole-vector loop accumulation (explicit order, container-independent).
    s = 0.0
    for i in 1:1360
        s += imag(imys[i])
    end
    @test reinterpret(UInt64, s) == 0xbd3e800000000000

    # The Mandelbrot grid construction path: c = xs[j] + imys[i].
    xs = range(-2.0, 1.0; length=1700)
    C = xs' .+ im .* ys
    @test size(C) == (1360, 1700)
    @test reinterpret(UInt64, real(C[11, 1])) == 0xc000000000000000
    @test reinterpret(UInt64, imag(C[11, 1])) == 0x3ff2eadd4d3211db

    # Non-TwicePrecision operands keep the generic elementwise path.
    ints = 1:5
    imints = im .* ints
    @test imints[3] == 3im
    v = [0.5, 1.5]
    imv = im .* v
    @test imv[2] == 0.0 + 1.5im
end

true
