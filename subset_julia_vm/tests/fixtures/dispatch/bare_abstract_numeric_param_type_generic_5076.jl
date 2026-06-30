# Regression: bare abstract-numeric parameter annotations (`x::Real`,
# `x::Number`, `x::Integer`, `x::Signed`, ...) must NOT widen type-generic
# results (`zero`, `one`, `oneunit`) to Float64/Int64; they must preserve the
# concrete argument type, matching upstream and the `where {T<:Real}` form
# (Issue #5076).
#
# Before the fix, `f(x::Real)=zero(x)` widened `x` to `ValueType::F64` at
# compile time (`type_helpers::julia_type_to_value_type` maps Real/Number to
# F64, Integer to I64), so `infer_julia_type` reported `Float64` and statically
# bound `zero(x)` to `zero(::Float64)`. `f(3)` then ran the Float64 body and
# errored ("expected I64, got Float64"); `f(Int8(3))` returned `0.0::Float64`.
# The fix makes `infer_julia_type` report `Any` for params already tracked in
# `abstract_numeric_params` (which already load via `LoadAny`), so type-generic
# calls dispatch on the concrete runtime value, like the untyped/`where` forms.
#
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

fR(x::Real) = zero(x)
fN(x::Number) = zero(x)
fI(x::Integer) = zero(x)
fS(x::Signed) = zero(x)
oR(x::Real) = one(x)
oN(x::Number) = one(x)
oI(x::Integer) = one(x)
uR(x::Real) = oneunit(x)
uN(x::Number) = oneunit(x)
uI(x::Integer) = oneunit(x)

@testset "zero via bare abstract annotation" begin
    @test fR(3) === 0
    @test fR(Int8(3)) === Int8(0)
    @test fR(Int16(3)) === Int16(0)
    @test fR(Int32(3)) === Int32(0)
    @test fR(3.0) === 0.0
    @test fN(3) === 0
    @test fN(Int8(3)) === Int8(0)
    @test fN(3.0) === 0.0
    @test fI(3) === 0
    @test fI(Int8(3)) === Int8(0)
    @test fS(3) === 0
    @test fS(Int8(3)) === Int8(0)
    @test typeof(fR(3)) === Int64
    @test typeof(fR(Int8(3))) === Int8
    @test typeof(fR(3.0)) === Float64
end

@testset "one via bare abstract annotation" begin
    @test oR(3) === 1
    @test oR(Int8(3)) === Int8(1)
    @test oR(Int32(3)) === Int32(1)
    @test oR(3.0) === 1.0
    @test oN(3) === 1
    @test oN(Int8(3)) === Int8(1)
    @test oI(3) === 1
    @test oI(Int8(3)) === Int8(1)
    @test typeof(oR(3)) === Int64
    @test typeof(oR(Int8(3))) === Int8
end

@testset "oneunit via bare abstract annotation" begin
    @test uR(3) === 1
    @test uR(Int8(3)) === Int8(1)
    @test uR(3.0) === 1.0
    @test uN(3) === 1
    @test uI(3) === 1
    @test uI(Int8(3)) === Int8(1)
    @test typeof(uR(Int8(3))) === Int8
end

@testset "bare abstract matches where-form and untyped" begin
    wR(x::T) where {T<:Real} = zero(x)
    nA(x) = zero(x)
    @test fR(3) === wR(3) === nA(3)
    @test fR(Int8(3)) === wR(Int8(3)) === nA(Int8(3))
    @test fR(3.0) === wR(3.0) === nA(3.0)
end

true
