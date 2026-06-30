using Test

ti6547_clamp_float(x::Float64) = clamp(x, 0.0, 1.0)
ti6547_clamp_int(x::Int64) = clamp(x, 1, 10)
ti6547_binomial(n::Int64, k::Int64) = binomial(n, k)
ti6547_ndigits(x::Int64) = ndigits(x)
ti6547_widen(x::Int32) = widen(x)
ti6547_copysign(x::Float64, y::Float64) = copysign(x, y)

@testset "Base numeric method-table snapshots keep call-site precision (Issue #6547)" begin
    @test Base.infer_return_type(ti6547_clamp_float, Tuple{Float64}) === Float64
    @test Base.infer_return_type(ti6547_clamp_int, Tuple{Int64}) === Int64
    @test Base.infer_return_type(ti6547_binomial, Tuple{Int64,Int64}) === Int64
    @test Base.infer_return_type(ti6547_ndigits, Tuple{Int64}) === Int64
    @test Base.infer_return_type(ti6547_widen, Tuple{Int32}) === Int64
    @test Base.infer_return_type(ti6547_copysign, Tuple{Float64,Float64}) === Float64
end

Base.infer_return_type(ti6547_clamp_float, Tuple{Float64}) === Float64 ||
    error("clamp(::Float64, ::Float64, ::Float64) wrapper should infer Float64")
Base.infer_return_type(ti6547_clamp_int, Tuple{Int64}) === Int64 ||
    error("clamp(::Int64, ::Int64, ::Int64) wrapper should infer Int64")
Base.infer_return_type(ti6547_binomial, Tuple{Int64,Int64}) === Int64 ||
    error("binomial(::Int64, ::Int64) wrapper should infer Int64")
Base.infer_return_type(ti6547_ndigits, Tuple{Int64}) === Int64 ||
    error("ndigits(::Int64) wrapper should infer Int64")
Base.infer_return_type(ti6547_widen, Tuple{Int32}) === Int64 ||
    error("widen(::Int32) wrapper should infer Int64")
Base.infer_return_type(ti6547_copysign, Tuple{Float64,Float64}) === Float64 ||
    error("copysign(::Float64, ::Float64) wrapper should infer Float64")

true
