# Benchmark driver for the array-of-struct field-read hot loop (Issue #9188):
# `p = c.items[i]; s += p.x` for a struct field declared `Vector{T}`. This is
# the pattern behind the slow Aizawa attractor sample (#9154): `plt.series[i]`
# followed by `.x`/`.y`/`.z` field reads inside the plotting hot loop.

struct Pt9188Bench
    x::Float64
    y::Float64
    z::Float64
end

struct Container9188Bench
    items::Vector{Pt9188Bench}
end

function build_9188bench(n::Int64)
    return Container9188Bench([Pt9188Bench(Float64(i), Float64(i) * 2.0, Float64(i) * 3.0) for i in 1:n])
end

function sum_fields_9188bench(c::Container9188Bench, reps::Int64)
    total = 0.0
    for _ in 1:reps
        s = 0.0
        for i in eachindex(c.items)
            p = c.items[i]
            s += p.x + p.y + p.z
        end
        total += s
    end
    return total
end

const N_9188BENCH = 20000
const REPS_9188BENCH = 50

c_9188bench = build_9188bench(N_9188BENCH)
println(sum_fields_9188bench(c_9188bench, REPS_9188BENCH))
