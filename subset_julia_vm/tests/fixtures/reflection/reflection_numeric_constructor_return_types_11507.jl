# A builtin numeric DataType (Int64, Float64, UInt8, BigInt, BigFloat, ...)
# used as a constructor/conversion callable, called with a single concrete
# numeric (or complex) argument type, constructs exactly that type upstream
# (mirroring `(::Type{T})(x::Number) where T<:Number = convert(T, x)::T`).
# Some of these conversions (Int8/16/32/128, UInt8/16/32/128, Float16/32) are
# implemented as thin pure-Julia wrapper functions over an internal intrinsic
# (e.g. `UInt8(x) = _to_uint8(x)`), whose own inferred return type is the
# imprecise `Any`; others (Int64, Float64, Bool, BigInt, BigFloat) have no
# pure-Julia wrapper at all and previously fell through
# builtin_reflection_return_type straight to Union{}/an empty vector. Both
# shapes are covered by a single structural override (Issue #11507).
using Test

f11507(x::Int) = x + 1

@testset "numeric constructor reflection (Issue #11507)" begin
    @test Base.infer_return_type(Int64, Tuple{Float64}) == Int64
    @test Base.return_types(Int64, Tuple{Float64}) == Any[Int64]

    @test Base.infer_return_type(Float64, Tuple{Int64}) == Float64
    @test Base.infer_return_type(UInt8, Tuple{Int64}) == UInt8
    @test Base.infer_return_type(Int8, Tuple{Int64}) == Int8
    @test Base.infer_return_type(Int16, Tuple{Int64}) == Int16
    @test Base.infer_return_type(Int32, Tuple{Int64}) == Int32
    @test Base.infer_return_type(Int128, Tuple{Int64}) == Int128
    @test Base.infer_return_type(UInt16, Tuple{Int64}) == UInt16
    @test Base.infer_return_type(UInt32, Tuple{Int64}) == UInt32
    @test Base.infer_return_type(UInt64, Tuple{Int64}) == UInt64
    @test Base.infer_return_type(UInt128, Tuple{Int64}) == UInt128
    @test Base.infer_return_type(Float16, Tuple{Int64}) == Float16
    @test Base.infer_return_type(Float32, Tuple{Int64}) == Float32
    @test Base.infer_return_type(Bool, Tuple{Int64}) == Bool
    @test Base.infer_return_type(BigInt, Tuple{Int64}) == BigInt
    @test Base.infer_return_type(BigFloat, Tuple{Float64}) == BigFloat
    @test Base.infer_return_type(Int64, Tuple{Complex{Int64}}) == Int64

    # Arbitrary-precision arguments are concrete numeric arguments too.
    @test Base.infer_return_type(Int64, Tuple{BigInt}) == Int64
    @test Base.infer_return_type(Float64, Tuple{BigFloat}) == Float64
    @test Base.infer_return_type(Float64, Tuple{BigInt}) == Float64
    @test Base.infer_return_type(BigInt, Tuple{BigInt}) == BigInt

    # Wrong arity dispatches to no constructor.
    @test Base.infer_return_type(Int64, Tuple{}) == Union{}
    @test Base.infer_return_type(Int64, Tuple{Int64, Int64}) == Union{}
    # Non-numeric argument has no matching conversion method.
    @test Base.infer_return_type(Int64, Tuple{String}) == Union{}

    # Ordinary function reflection is untouched.
    @test Base.infer_return_type(f11507, Tuple{Int64}) == Int64
    @test Base.infer_return_type(+, Tuple{Int64, Float64}) == Float64
end

true
