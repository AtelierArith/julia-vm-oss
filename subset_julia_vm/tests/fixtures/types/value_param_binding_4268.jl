using Test

f(::Val{N}) where N = N
f_tuple(::Val{N}) where N = ntuple(i -> i, N)
g(::Type{T}, x::T) where T = x
type_param_return_4268(::Type{T}) where T = T
h(xs::NTuple{N,T}) where {N,T} = N
h2(xs::NTuple{N,T}) where {N,T} = T
ident_ntuple_4268(i) = i
captured_ntuple_4268(a) = ntuple(i -> i + a, 3)

function local_captured_ntuple_4268(a)
    f = i -> i + a
    ntuple(f, 3)
end

@testset "Val and NTuple value-parameter binding (Issue #4268)" begin
    @test f(Val{3}()) == 3
    @test f(Val(3)) == 3
    @test typeof(f(Val(3))) == Int64
    @test f(Val{UInt8(1)}()) == UInt8(1)
    @test typeof(f(Val{UInt8(1)}())) === UInt8
    @test f(Val{Int32(2)}()) == Int32(2)
    @test typeof(f(Val{Int32(2)}())) === Int32
    @test f(Val{Float32(1.5)}()) == Float32(1.5)
    @test typeof(f(Val{Float32(1.5)}())) === Float32
    @test f(Val{1.5}()) == 1.5
    @test typeof(f(Val{1.5}())) === Float64
    @test f(Val{Inf}()) == Inf
    @test typeof(f(Val{Inf}())) === Float64
    @test f(Val{-Inf}()) == -Inf
    @test typeof(f(Val{-Inf}())) === Float64
    # Issue #8353: the explicit Val constructor must retain the value
    # parameter in the constructed instance's type, not fall back to Val{Any}.
    @test typeof(Val{-Inf}()) == Val{-Inf}
    @test isnan(f(Val{NaN}()))
    @test typeof(f(Val{NaN}())) === Float64
    @test f(Val{'x'}()) == 'x'
    @test typeof(f(Val{'x'}())) === Char
    @test typeof(Val{'x'}()) == Val{'x'}
    @test f(Val{'\n'}()) == '\n'
    @test typeof(f(Val{'\n'}())) === Char
    @test Int(f(Val{'\n'}())) == 10
    @test f(Val{'\''}()) == '\''
    @test f(Val{'\\'}()) == '\\'
    @test f(Val{'\x41'}()) == 'A'
    @test typeof(f(Val{'\x41'}())) === Char
    @test f(Val{'\u03B1'}()) == '\u03B1'
    @test typeof(f(Val{'\u03B1'}())) === Char
    @test Int(f(Val{'\u03B1'}())) == 945
    @test f(Val{(1, 2)}()) == (1, 2)
    @test typeof(f(Val{(1, 2)}())) == Tuple{Int64, Int64}
    @test f(Val{()}()) == ()
    @test typeof(f(Val{()}())) == Tuple{}
    @test f(Val{(1,)}()) == (1,)
    @test typeof(f(Val{(1,)}())) == Tuple{Int64}
    @test f(Val{(true, :x)}()) == (true, :x)
    @test typeof(f(Val{(true, :x)}())) == Tuple{Bool, Symbol}
    bits_tuple_4268 = f(Val{(UInt8(1), Int32(2))}())
    @test bits_tuple_4268[1] == UInt8(1)
    @test bits_tuple_4268[2] == Int32(2)
    @test typeof(bits_tuple_4268) == Tuple{UInt8, Int32}
    @test f(Val{(1, (2, 3))}()) == (1, (2, 3))
    @test f_tuple(Val{3}()) == (1, 2, 3)
    @test typeof(f_tuple(Val{3}())[1]) == Int64
    @test g(Int64, 2) == 2
    # Issue #4846: direct `x::T` return through a `Type{T}` signature must be
    # inferred precisely (regression guard; fixed via #5003/#5012 base).
    @test Base.infer_return_type(g, Tuple{Type{Int64}, Int64}) == Int64
    # Issue #4268: a method that directly returns the `where` type parameter `T`
    # bound from a `::Type{T}` argument returns the type object at runtime
    # (`typeof == DataType`), and `Base.infer_return_type` must be the precise
    # `Type{T}` rather than the widened `DataType`.
    @test type_param_return_4268(Int64) === Int64
    @test typeof(type_param_return_4268(Int64)) === DataType
    @test type_param_return_4268(Float64) === Float64
    @test type_param_return_4268(String) === String
    @test Base.infer_return_type(type_param_return_4268, Tuple{Type{Int64}}) == Type{Int64}
    @test Base.infer_return_type(type_param_return_4268, Tuple{Type{Float64}}) == Type{Float64}
    @test Base.infer_return_type(type_param_return_4268, Tuple{Type{String}}) == Type{String}
    @test ntuple(ident_ntuple_4268, 3) == (1, 2, 3)
    @test typeof(ntuple(ident_ntuple_4268, 3)[1]) == Int64
    @test h((1, 2, 3)) == 3
    @test h2((1, 2, 3)) == Int64
    @test captured_ntuple_4268(10) == (11, 12, 13)
    @test typeof(captured_ntuple_4268(10)) == Tuple{Int64, Int64, Int64}
    @test local_captured_ntuple_4268(10) == (11, 12, 13)
    @test typeof(local_captured_ntuple_4268(10)) == Tuple{Int64, Int64, Int64}
    @test Base.infer_return_type(captured_ntuple_4268, Tuple{Int64}) == Tuple{Int64, Int64, Int64}
    @test Base.infer_return_type(local_captured_ntuple_4268, Tuple{Int64}) == Tuple{Int64, Int64, Int64}
end

true
