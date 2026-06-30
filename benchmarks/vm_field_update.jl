# VM-only benchmark for mutable-struct field-update hot loops (Issue #6346).
#
# `step!` mutates two Float64 fields per call and is invoked once per inner
# iteration. Before #6346 any function containing `b.x = ...` failed lazy
# specialization statement-by-statement and ran on the generic by-name field
# path (GetFieldByName/SetFieldByName + dynamic arithmetic); now the field
# read/write is a typed GetField/SetField fast path and the n-ary `k * b.x * dt`
# product specializes through the typed binary-op fold.

mutable struct Body
    x::Float64
    v::Float64
end

function step!(b, dt, k)
    b.v = b.v - k * b.x * dt
    b.x = b.x + b.v * dt
    return b.x
end

function integrate(steps)
    b = Body(1.0, 0.0)
    acc = 0.0
    for _ in 1:steps
        acc += step!(b, 0.001, 4.0)
    end
    return acc
end

function run_trials(trials, steps)
    total = 0.0
    for _ in 1:trials
        total += integrate(steps)
    end
    return total
end

println(round(run_trials(200, 2000); digits=4))
