# Complex{Float64}^Integer power (Issue #9198 S6 A/B).
#
# `z^3` materializes `z` back to a boxed Complex{Float64} at the `^` boundary
# and runs DynamicPow → the #9155 `try_complex_f64_int_pow` Rust
# binary-exponentiation fast path. This is the residual dynamic pow route the
# S6 retirement A/B measures.
function dyn_pow(n::Int64)::Float64
    z = 1.001 + 0.002im
    s = 0.0 + 0.0im
    i = 0
    while i < n
        s = s + z^3
        i = i + 1
    end
    real(s)
end

println(dyn_pow(200000))
