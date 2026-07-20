# Typed-loop inlining of untyped Float64 callees (Issue #10491): an untyped
# loop-bodied helper reached from a typed loop through a fused
# CallSpecializeF64Slots site must produce results identical to its
# fully-typed twin and upstream Julia, including F64 edge cases
# (NaN propagation, ±Inf, signed zero, rounding order).

using Test

function fstep_untyped_10491(x, y)
    r = x
    k = 0
    while k < 4
        r = r + y
        r = r * 0.5
        k = k + 1
    end
    r
end

function fstep_typed_10491(x::Float64, y::Float64)::Float64
    r = x
    k = 0
    while k < 4
        r = r + y
        r = r * 0.5
        k = k + 1
    end
    r
end

function scan_untyped_10491(N::Int64)::Int64
    cnt = 0
    x = 0.0
    a = 1
    while a <= N
        x = x + 1.0
        y = 0.0
        b = 1
        while b <= N
            y = y + 1.0
            if fstep_untyped_10491(x, y) > 1.5
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

function scan_typed_10491(N::Int64)::Int64
    cnt = 0
    x = 0.0
    a = 1
    while a <= N
        x = x + 1.0
        y = 0.0
        b = 1
        while b <= N
            y = y + 1.0
            if fstep_typed_10491(x, y) > 1.5
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

# Edge-value sweep through a typed loop: negative/zero/positive y, signed-zero
# start, rounding order preserved iteration by iteration.
function edge_sum_untyped_10491(N::Int64)::Float64
    s = 0.0
    x = -0.0
    i = 1
    while i <= N
        y = (i - 3) * 0.5
        r = fstep_untyped_10491(x, y)
        if r == r
            s = s + r
        end
        x = x + 0.25
        i = i + 1
    end
    s
end

@testset "typed-loop specialize F64 inline parity (Issue #10491)" begin
    @test scan_untyped_10491(50) == scan_typed_10491(50)
    @test scan_untyped_10491(50) == 2491
    @test edge_sum_untyped_10491(20) == 73.28125
    # direct edge cases through the same untyped helper
    @test fstep_untyped_10491(-0.0, -0.0) === -0.0
    @test fstep_untyped_10491(0.0, -0.0) === 0.0
    @test isnan(fstep_untyped_10491(NaN, 1.0))
    @test fstep_untyped_10491(Inf, 1.0) == Inf
    @test fstep_untyped_10491(-Inf, 1.0) == -Inf
    @test fstep_untyped_10491(1.0, 1e-300) == 0.0625
    @test fstep_untyped_10491(1.0, 0.1) == fstep_typed_10491(1.0, 0.1)
end

true
