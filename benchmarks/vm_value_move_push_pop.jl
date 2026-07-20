# VM-only benchmark for generic Value-move bandwidth (Issue #8650 / #8676).
#
# This is the GAIN-SIDE benchmark for the I128/U128 boxing decision (#8650):
# shrinking Value from 64→56 bytes (by boxing I128/U128 to drop the 16-byte
# alignment) reduces every stack push, slot store, Vec push, and argument-copy
# cost by 12.5%.  This driver exercises the push!/pop! hot path on a large
# Vector of Int64 values — the 64-byte Value enum means every element move
# is a 64-byte memcpy; at 56 bytes it would be 56 bytes.
#
# Uses Int64 (not Int128) intentionally: this is the "generic move" baseline.
# After boxing, the exact same I64 path should benefit from smaller Value size.
# The result is printed to prevent dead-code elimination.

function value_move_push_pop(n::Int64)::Int64
    v = Vector{Int64}(undef, 0)
    for i in 1:n
        push!(v, i)
    end
    s::Int64 = 0
    for i in 1:n
        s += pop!(v)
    end
    return s
end

function value_move_array_copy(n::Int64)::Int64
    a = [i for i in 1:n]
    b = copy(a)
    s::Int64 = 0
    for i in 1:n
        s += a[i] + b[n - i + 1]
    end
    return s
end

function run_trials(trials::Int64, n::Int64)
    s1::Int64 = 0
    s2::Int64 = 0
    for _ in 1:trials
        s1 += value_move_push_pop(n)
        s2 += value_move_array_copy(n)
    end
    return s1, s2
end

r1, r2 = run_trials(5, 128)
println(r1)
println(r2)
