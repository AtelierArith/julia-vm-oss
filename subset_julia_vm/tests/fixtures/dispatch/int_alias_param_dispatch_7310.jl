# Issue #7310: a parameter annotated `::Int` (the native word-size integer
# alias) must dispatch against integer literal / runtime arguments. The VM's
# integer carrier is uniformly Int64, so `::Int` always aliases `Int64`,
# independent of the host pointer width. On 32-bit targets (wasm32) `::Int`
# used to alias `Int32` and never matched the `Int64` literals it was compared
# with, producing a spurious `MethodError`.
using Test

# Direct `::Int` parameter, called with an integer literal.
function p(a::Float64, n::Int)
    return a + n
end
@test p(1.0, 2) == 3.0

# `::Int` as a trailing default argument (the original report's MWE shape).
function f(a::Float64, b::Float64, x::Float64, maxiter::Int=200, eps::Float64=1e-10)
    return a + b + x + maxiter + eps
end
@test f(1.0, 2.0, 3.0) == 206.0000000001
@test f(1.0, 2.0, 3.0, 5) == 11.0000000001

# Single `::Int` default parameter, called with no arguments.
function t(n::Int=200)
    return n + 1
end
@test t() == 201

# `::UInt` alias must likewise accept an unsigned-int value.
function g(n::UInt)
    return n + UInt(1)
end
@test g(UInt(41)) == UInt(42)

# `Int`/`UInt` as a value (the type object) is the 64-bit alias.
@test Int === Int64
@test UInt === UInt64

true
