# Regression: a user-defined method that DIRECTLY returns a bare
# abstract-numeric parameter — `f(x::Real) = x` (and `::Number`/`::Integer`/
# `::Signed`) — must preserve the concrete argument type, not widen to Float64
# or error (Issue #5242).
#
# Before the fix, `compile_expr(Var x)` reported the param's *annotation* slot
# type (`ValueType::F64` for Real/Number) as the static type of `x`, even though
# the param already loads via `LoadAny`. A direct return then emitted `ReturnF64`,
# coercing the concrete runtime value (e.g. `Int64(3)` → `Float64(3.0)`); the
# typed caller slot rejected it ("expected I64, got Float64") or `typeof` reported
# Float64. The Base equivalents `real(x::Real)=x` / `conj(x::Real)=x` already
# worked, so this was a distinct direct identity-return path — separate from the
# direct type-generic call (#5076/#5169) and the forwarded call (#5167p2/#5243).
#
# The fix reports `ValueType::Any` for params tracked in `abstract_numeric_params`
# in the `compile_expr` `Var` branch, matching the `LoadAny` representation, so the
# direct return uses `ReturnAny` and preserves the concrete runtime type —
# symmetric with the `infer_julia_type` (#5169) and `infer_expr_type` (#5243)
# guards.
#
# Use `===` / typeof to catch the TYPE, not just the value (3 == 3.0 is true).

using Test

idR(x::Real) = x
idN(x::Number) = x
idI(x::Integer) = x
idS(x::Signed) = x

@testset "direct return of ::Real param preserves concrete type" begin
    @test idR(3) === 3
    @test idR(Int8(5)) === Int8(5)
    @test idR(Int16(7)) === Int16(7)
    @test idR(Int32(9)) === Int32(9)
    @test idR(2.5) === 2.5
    @test idR(2.5f0) === 2.5f0
    @test idR(Float16(1.5)) === Float16(1.5)
    @test typeof(idR(3)) === Int64
    @test typeof(idR(Int8(5))) === Int8
    @test typeof(idR(2.5f0)) === Float32
end

@testset "direct return of ::Number param preserves concrete type" begin
    @test idN(3) === 3
    @test idN(Int8(5)) === Int8(5)
    @test idN(2.5) === 2.5
    @test idN(2.5f0) === 2.5f0
    @test typeof(idN(3)) === Int64
    @test typeof(idN(2.5f0)) === Float32
end

@testset "direct return of ::Integer / ::Signed param preserves concrete type" begin
    @test idI(3) === 3
    @test idI(Int8(5)) === Int8(5)
    @test idI(Int16(7)) === Int16(7)
    @test idS(Int16(7)) === Int16(7)
    @test typeof(idI(Int8(5))) === Int8
    @test typeof(idS(Int16(7))) === Int16
end

@testset "BigInt/BigFloat flow through abstract-numeric direct return" begin
    @test idR(big"123") == big"123"
    @test typeof(idR(big"123")) === BigInt
    @test idN(big"1.5") == big"1.5"
    @test typeof(idN(big"1.5")) === BigFloat
end

@testset "direct return matches Base real/conj and the where-form" begin
    wR(x::T) where {T<:Real} = x
    @test idR(3) === real(3) === conj(3) === wR(3)
    @test idR(Int8(5)) === real(Int8(5)) === conj(Int8(5)) === wR(Int8(5))
    @test idR(2.5f0) === real(2.5f0) === conj(2.5f0) === wR(2.5f0)
end

@testset "direct-return result usable in typed contexts" begin
    # The caller slot must accept the preserved concrete type.
    v = idR(3)
    @test v === 3
    @test idR(3) + idR(4) === 7
    @test [idR(1), idR(2), idR(3)] == [1, 2, 3]
end

true
