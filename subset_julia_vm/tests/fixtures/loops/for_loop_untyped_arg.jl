# Issue #8205: a function whose argument `n` is *untyped* but called with a
# concrete `Int64` is specialized at runtime. The specialized body must be
# peephole-fused (like the main-compiler body) so the F64 for-loop produces the
# same result as its fully typed twin. This guards the correctness of the
# install_specialized_body peephole pass.
function fsum_untyped(n)
    s = 0.0
    a = 1.5
    b = 2.0
    for i in 0:(n - 1)
        s = s + a * b - a / b + (a - b) * (a + b)
    end
    s
end

function fsum_typed(n::Int64)::Float64
    s::Float64 = 0.0
    a::Float64 = 1.5
    b::Float64 = 2.0
    for i in 0:(n - 1)
        s = s + a * b - a / b + (a - b) * (a + b)
    end
    s
end

println(fsum_untyped(1000) == fsum_typed(1000))
println(fsum_untyped(1000))
println(fsum_untyped(0) == 0.0)
"true\n500.0\ntrue\n"
