# VM-only benchmark for Int128/UInt128 arithmetic throughput (Issue #8650 / #8676).
#
# This is the LOSS-SIDE benchmark for the I128/U128 boxing decision (#8650):
# boxing I128/U128 variants would require heap-allocating every Int128/UInt128
# value and dereferencing a pointer on every arithmetic operation. This driver
# measures how many ns/iteration a tight Int128 arithmetic loop costs; after
# boxing, the same loop pays one Box::new + one *ptr per operation instead of a
# register-to-register move.
#
# The loop is intentionally unrolled 4× and uses both add and multiply so the
# compiler cannot collapse it to a constant.  The result is printed to prevent
# dead-code elimination.

function int128_arith_loop(n::Int64)::Int128
    a::Int128 = Int128(1)
    b::Int128 = Int128(2)
    c::Int128 = Int128(3)
    d::Int128 = Int128(4)
    for _ in 1:n
        a = a + b
        b = b * c
        c = c + d
        d = d * a
    end
    return a + b + c + d
end

function uint128_arith_loop(n::Int64)::UInt128
    a::UInt128 = UInt128(1)
    b::UInt128 = UInt128(2)
    c::UInt128 = UInt128(3)
    d::UInt128 = UInt128(4)
    for _ in 1:n
        a = a + b
        b = b * c
        c = c + d
        d = d * a
    end
    return a + b + c + d
end

function run_trials(trials::Int64, n::Int64)
    s128::Int128 = Int128(0)
    u128::UInt128 = UInt128(0)
    for _ in 1:trials
        s128 += int128_arith_loop(n)
        u128 += uint128_arith_loop(n)
    end
    return s128, u128
end

s, u = run_trials(5, 100)
println(s)
println(u)
