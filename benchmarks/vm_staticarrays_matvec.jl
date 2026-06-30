# VM-only benchmark for StaticArrays SMatrix*SVector matrix-vector arithmetic
# (Issues #7461 / #7956).
#
# `ifs_walk` iterates the 2x2 affine map `x <- W*x + b` — the IFS chaos-game
# kernel that motivated static-array arithmetic (#7461/#7949). Each iteration
# exercises `SMatrix{2,2} * SVector{2}` and `SVector{2} + SVector{2}`, so it
# tracks the hand-unrolled `.data` fast paths and the where-clause `size`/`length`
# value methods added in #7956. `floor(Int, ...)` keeps the accumulated output a
# stable integer so the harness can validate it. The map is contractive toward a
# fixed point, so the per-iteration components stay bounded.

using StaticArrays

function ifs_walk(n)
    W = @SMatrix [0.85 0.04; -0.04 0.85]
    b = @SVector [0.0, 1.6]
    x = @SVector [1.0, 1.0]
    acc = 0
    for k in 1:n
        x = W * x + b
        acc = acc + floor(Int, x[1]) + floor(Int, x[2])
    end
    return acc
end

function run_trials(trials, n)
    total = 0
    for _ in 1:trials
        total = total + ifs_walk(n)
    end
    return total
end

println(run_trials(5, 400))
