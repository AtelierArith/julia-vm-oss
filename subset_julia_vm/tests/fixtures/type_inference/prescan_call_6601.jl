function calls(z, n)
    e = exp(z)
    a = abs(z)
    zs = zeros(n)
    return (typeof(e), a, length(zs))
end

r = calls(1.0 + 2.0im, 3)
println(r[1])
println(r[2])
println(r[3])
println(typeof(exp(1.0 + 2.0im)))
println(typeof(abs(1.0 + 2.0im)))
println(typeof(zeros(3)))

true
