# Issue #3525: Builtin operations should not collapse to unknown_builtin.
# `Expr::Builtin` is now routed through the transfer-function registry, so the
# return type of length/size/zero/first/last/haskey is preserved instead of
# being widened to `Top` ("unknown_builtin").

# Issue example: length followed by zero must keep Int64 through `+ 1`.
function f(xs)
    n = length(xs)
    z = zero(n)
    return z + 1
end

@assert f([10, 20]) == 1
@assert f([1, 2, 3, 4]) == 1

# Per-builtin typeof checks on Vector{Int}.
v = [10, 20, 30]
@assert typeof(length(v)) == Int64
@assert typeof(size(v)) == Tuple{Int64}
@assert typeof(zero(length(v))) == Int64

# Per-builtin typeof checks on Tuple.
t = (10, 20, 30)
@assert typeof(length(t)) == Int64
@assert typeof(zero(length(t))) == Int64

# first/last on tuple - lowered as BuiltinOp::TupleFirst/TupleLast.
function g(t)
    a = first(t)
    b = last(t)
    return a + b
end

@assert g((10, 20, 30)) == 40
@assert typeof(first((10, 20, 30))) == Int64
@assert typeof(last((10, 20, 30))) == Int64

# Dict builtin: haskey is BuiltinOp::HasKey, must infer Bool.
d = Dict(:a => 1, :b => 2)
@assert typeof(haskey(d, :a)) == Bool
@assert haskey(d, :a) == true
@assert haskey(d, :z) == false

# Predicate builtin: isa must infer Bool.
@assert typeof(isa(v, Vector)) == Bool
@assert isa(v, Vector) == true
@assert isa(v, Tuple) == false

# Predicate helpers return Bool.
@assert typeof(isless(1, 2)) == Bool
@assert typeof(isnan(NaN)) == Bool
@assert typeof(isinf(Inf)) == Bool
@assert typeof(isfinite(1.0)) == Bool
@assert typeof(isinteger(1.0)) == Bool
@assert typeof(iseven(2)) == Bool
@assert typeof(isodd(3)) == Bool
@assert typeof(isnothing(nothing)) == Bool
@assert typeof(ismissing(missing)) == Bool

# Additional unary math helpers infer Float64 through tfuncs.
@assert typeof(tan(1.0)) == Float64
@assert typeof(asin(0.5)) == Float64
@assert typeof(acos(0.5)) == Float64
@assert typeof(atan(1.0)) == Float64
@assert typeof(sinh(1.0)) == Float64
@assert typeof(cosh(1.0)) == Float64
@assert typeof(tanh(1.0)) == Float64
@assert typeof(asinh(1.0)) == Float64
@assert typeof(acosh(2.0)) == Float64
@assert typeof(atanh(0.5)) == Float64
@assert typeof(log2(2.0)) == Float64
@assert typeof(log10(10.0)) == Float64
@assert typeof(log1p(1.0)) == Float64
@assert typeof(expm1(1.0)) == Float64

# prod infers from array element type through tfuncs.
@assert typeof(prod([2, 3, 4])) == Int64
@assert typeof(prod(Float32[2, 3, 4])) == Float32

# Int64-result collection helpers infer through tfuncs.
@assert typeof(ndims([1, 2, 3])) == Int64
@assert typeof(count([true, false, true])) == Int64
@assert typeof(count(isodd, [1, 2, 3])) == Int64

# String-result helpers infer through tfuncs.
@assert typeof(repr(42)) == String
@assert typeof(lpad("x", 3)) == String
@assert typeof(rpad("x", 3)) == String
@assert typeof(sprint(print, 42)) == String
@assert typeof(bitstring(UInt8(3))) == String
@assert typeof(unescape_string("\\n")) == String

# gcd/lcm preserve BigInt and default to Int64 through tfuncs (Issue #5922).
@assert typeof(gcd(12, 8)) == Int64
@assert typeof(lcm(4, 6)) == Int64
@assert gcd(big(12), big(8)) == 4
@assert typeof(gcd(big(12), big(8))) == BigInt

# big() converts floats to BigFloat and integers to BigInt through tfuncs.
@assert typeof(big(1)) == BigInt
@assert typeof(big(1.5)) == BigFloat

# IOBuffer construction infers IO through tfuncs.
io = IOBuffer()
print(io, "x")
@assert String(take!(io)) == "x"

# Type-returning helpers infer DataType through tfuncs.
@assert typeof(1) == Int64
@assert promote_type(Int64, Float64) == Float64
@assert eltype([1, 2, 3]) == Int64
@assert keytype(Dict(:a => 1)) == Symbol
@assert valtype(Dict(:a => 1)) == Int64

# 2-arg isequal infers Bool; the curried 1-arg form stays callable (Issue #5662).
@assert typeof(isequal(2, 2)) == Bool
@assert filter(isequal(2), [1, 2, 3, 2]) == [2, 2]

# Int64-result integer helpers infer through tfuncs.
@assert hash(1) == hash(1)
@assert fld(7, 2) == 3
@assert cld(7, 2) == 4
@assert typeof(fld(7, 2)) == Int64

# trues/falses infer BitArray-family containers through tfuncs.
@assert trues(3) isa BitVector
@assert falses(2, 3) isa BitMatrix
@assert sum(trues(3)) == 3

true
