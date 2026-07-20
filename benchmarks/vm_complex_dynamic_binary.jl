# Dynamic-dispatch Complex{Float64} binary arithmetic (Issue #9198 S6 A/B).
#
# `s = s + z*2.0 + 1.0` keeps the accumulator `s` off the S2/S3 slot-pair SROA
# path, so each `+` on two Complex{Float64} values runs through
# CallDynamicBinaryBoth and hits the #9125 `try_complex_f64_binary_op` Rust
# fast path (profiler: 1 `BinaryBothComplexF64FastHit` per iteration). This is
# the residual dynamic Complex route the S6 retirement A/B measures.
function dyn_binary(n::Int64)::Float64
    z = 1.0 + 2.0im
    s = 0.0 + 0.0im
    i = 0
    while i < n
        s = s + z * 2.0 + 1.0
        i = i + 1
    end
    real(s)
end

println(dyn_binary(200000))
