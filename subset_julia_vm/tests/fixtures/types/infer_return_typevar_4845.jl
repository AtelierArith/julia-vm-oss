using Test

# Direct TypeVar / value-parameter returns through reflection inference
# (Issue #4845). A method whose body directly returns a `where`-bound type
# variable must have `Base.infer_return_type` resolve that variable from the
# concrete call signature instead of widening to `Any`.
#
# Scope (slice): value parameters whose carrier type is recoverable from the
# static type argument (`Int64`, `Float64`, `Symbol`, `Bool`, and typed narrow
# integer carriers such as `UInt8` / `Int32`), the `NTuple{N,T}` length
# value-parameter `N` (always `Int64`), and the `NTuple{N,T}` element
# type-parameter `T` (returns `Type{T}`).
f(::Val{N}) where N = N
h(xs::NTuple{N,T}) where {N,T} = N
h2(xs::NTuple{N,T}) where {N,T} = T

@testset "infer_return_type direct TypeVar / value-param returns (Issue #4845)" begin
    # Value-parameter `Val{N}`: returns the value, so the inferred return type
    # is the carrier type of the value parameter.
    @test Base.infer_return_type(f, Tuple{Val{3}}) == Int64
    @test Base.infer_return_type(f, Tuple{Val{1.5}}) == Float64
    @test Base.infer_return_type(f, Tuple{Val{:x}}) == Symbol
    @test Base.infer_return_type(f, Tuple{Val{true}}) == Bool
    @test Base.infer_return_type(f, Tuple{Val{UInt8(1)}}) == UInt8
    @test Base.infer_return_type(f, Tuple{Val{Int32(2)}}) == Int32
    @test Val{UInt8(1)} == Val{0x01}
    @test typeof(Val{UInt8(1)}.parameters[1]) == UInt8
    t = Val{UInt8(1)}
    @test typeof(t.parameters[1]) == UInt8

    # `NTuple{N,T}` length value-parameter `N`: returns the Int length value.
    @test Base.infer_return_type(h, Tuple{Tuple{Int64,Int64,Int64}}) == Int64
    @test Base.infer_return_type(h, Tuple{Tuple{Int64,Int64}}) == Int64

    # `NTuple{N,T}` element type-parameter `T`: returns the type object, so the
    # inferred return type is `Type{T}`.
    @test Base.infer_return_type(h2, Tuple{Tuple{Int64,Int64}}) == Type{Int64}
    @test Base.infer_return_type(h2, Tuple{Tuple{Float64,Float64}}) == Type{Float64}
end

true
