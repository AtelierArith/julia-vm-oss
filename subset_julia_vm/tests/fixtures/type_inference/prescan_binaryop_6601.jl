function ops(a, b, s, z, x)
    p = a ^ b
    cat = s * s
    cf = z + x
    cc = z * z
    return (p, cat, cf, cc)
end

r = ops(2, 10, "ab", 1.0 + 2.0im, 3.0)
println(r[1])
println(r[2])
println(r[3])
println(r[4])
println(typeof(2 ^ 10))
println(typeof("ab" * "ab"))
println(typeof((1.0 + 2.0im) + 3.0))

true
