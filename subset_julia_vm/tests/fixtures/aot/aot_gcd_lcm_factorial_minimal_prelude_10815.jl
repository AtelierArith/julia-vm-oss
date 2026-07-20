# vm_aot equivalence corpus widening (Issue #10815): the integer-utility
# family (gcd/lcm/factorial) defined directly in prelude_aot.jl, exercised
# through the AoT minimal-prelude codegen path.
println(gcd(48, 18))
println(lcm(4, 6))
println(factorial(5))

gcd(48, 18) == 6 && lcm(4, 6) == 12 && factorial(5) == 120
