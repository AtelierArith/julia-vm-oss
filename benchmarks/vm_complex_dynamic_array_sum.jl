# sum() reduction over a Complex{Float64} array (Issue #9198 S6 A/B).
#
# The reduction `+` inside `sum` runs on dynamically-typed Complex{Float64}
# operands and hits the #9125 fast path (profiler: length-1 hits per sum). The
# array itself is built with the general contiguous isbits storage (S4/S5).
function array_sum(reps::Int64)::Float64
    a = [k + 0.5im for k in 1.0:1000.0]
    acc = 0.0
    r = 0
    while r < reps
        s = sum(a)
        acc = acc + real(s)
        r = r + 1
    end
    acc
end

println(array_sum(400))
