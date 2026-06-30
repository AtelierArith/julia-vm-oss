# Issue #6999: supported AoT scalar operators/builtins should match upstream
# Julia when compiled through juliars -> Rust -> native binary.

println(1 + 2 * 3)
println(10 - 4)
println(6 * 7)
println(9 / 2)
println(9 ÷ 2)
println(9 % 4)
println(2 ^ 5)
println(3 < 4)
println(3 == 3)
println(true + 2)
println(abs(-7))
println(min(3, 5))
println(max(3, 5))
# Float64 whole-value stdout formatting is tracked separately in Issue #7013.
println(length([1, 2, 3]))

true
