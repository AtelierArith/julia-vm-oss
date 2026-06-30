# Regression: a bare abstract-numeric parameter (`x::Real`, `x::Number`,
# `x::Integer`, ...) FORWARDED into another user function whose body performs a
# type-generic call (`zero`, `one`, `oneunit`) must preserve the concrete
# argument type — matching upstream Julia 1.12 and the untyped/`where {T<:Real}`
# forms (Issue #5167 part 2; follow-up to #5076/#5169).
#
# Before the fix `g(y)=zero(y); f(x::Real)=g(x); f(3)` returned `0.0::Float64`
# instead of `0::Int64`. Root cause: `f`'s param `x::Real` is stored in the
# compiler's `locals` as `ValueType::F64` (Real/Number → F64). `f` always loads
# `x` via `LoadAny`, so the value reaching `g` is the correct concrete `Int64`,
# and `g`'s body `zero(y)` correctly produced `0::Int64` at runtime. But the
# call site `g(x)` in `f` re-inferred `g`'s return type with the *static* arg
# ValueType `F64` (via `infer_expr_type(x)` → F64 →
# `infer_function_return_type_v2_with_arg_types(g, [F64])` → `zero(::Float64)`
# → F64), so `f` coerced `g(x)`'s runtime `Int64` result to `Float64` on return.
# The fix makes `infer_expr_type` report `Any` for `abstract_numeric_params`
# (mirroring `infer_julia_type` / the `LoadAny` representation), so the
# speculative re-inference is skipped and the forwarded type-generic call
# dispatches on the concrete runtime value.
#
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

gz(y) = zero(y)
go(y) = one(y)
gu(y) = oneunit(y)

fzR(x::Real) = gz(x)
foR(x::Real) = go(x)
fuR(x::Real) = gu(x)
fzN(x::Number) = gz(x)
fzI(x::Integer) = gz(x)

@testset "zero forwarded through ::Real" begin
    @test fzR(3) === 0
    @test fzR(Int8(3)) === Int8(0)
    @test fzR(Int16(3)) === Int16(0)
    @test fzR(Int32(3)) === Int32(0)
    @test fzR(Int64(3)) === Int64(0)
    @test fzR(2.0f0) === 0.0f0
    @test fzR(2.0) === 0.0
end

@testset "one forwarded through ::Real" begin
    @test foR(3) === 1
    @test foR(Int8(3)) === Int8(1)
    @test foR(Int16(3)) === Int16(1)
    @test foR(2.0f0) === 1.0f0
    @test foR(2.0) === 1.0
end

@testset "oneunit forwarded through ::Real" begin
    @test fuR(3) === 1
    @test fuR(Int8(3)) === Int8(1)
    @test fuR(2.0f0) === 1.0f0
    @test fuR(2.0) === 1.0
end

@testset "zero forwarded through ::Number / ::Integer" begin
    @test fzN(Int8(7)) === Int8(0)
    @test fzN(2.0f0) === 0.0f0
    @test fzI(Int16(7)) === Int16(0)
    @test fzI(Int32(7)) === Int32(0)
end

# Two-hop forwarding: ::Real → untyped → untyped type-generic call.
k1(y) = zero(y)
k2(z) = k1(z)
k3(x::Real) = k2(x)

@testset "zero forwarded through two hops" begin
    @test k3(Int8(9)) === Int8(0)
    @test k3(Int16(9)) === Int16(0)
    @test k3(2.0f0) === 0.0f0
    @test k3(2.0) === 0.0
end

# Cross-check: the bare ::Real forward agrees with the untyped and
# `where {T<:Real}` forms (both already correct before the fix).
nz(x) = gz(x)
wz(x::T) where {T<:Real} = gz(x)

@testset "bare ::Real forward matches untyped and where forms" begin
    for v in (3, Int8(3), Int16(3), Int32(3), 2.0f0, 2.0)
        @test fzR(v) === nz(v) === wz(v)
    end
end

true
