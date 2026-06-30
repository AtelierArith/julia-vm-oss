using Test

# Issue #4843: reflection-time return-type inference lost the fixed Vararg and
# nested NTuple return shapes. A method whose return value is a direct tuple of
# `where`-bound parameters — both type parameters (`T`) and value/length
# parameters (`N`, `M`) — used to infer to `Any` because the type variables in a
# `Tuple{Vararg{T,N}}` / `NTuple{N,NTuple{M,T}}` parameter annotation were never
# bound to the concrete argument type before re-running the body.
#
# After the fix, reflection unifies the parameter annotation against the concrete
# argument type, binding each `where` type parameter to its `DataType` and each
# length parameter to its `Int64` length, so the returned tuple recovers the
# precise shape upstream Julia reports.

vararg_pair_4843(xs::Tuple{Vararg{T,N}}) where {T,N} = (T, N)
nested_ntuple_4843(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = (N, M, T)
ntuple_elem_4843(xs::NTuple{N,T}) where {N,T} = (T, N)
ntuple_len_only_4843(xs::NTuple{N,Int64}) where {N} = N

@testset "Issue #4843 Vararg/NTuple where-param return shape" begin
    # Fixed Vararg: T -> DataType, N -> Int64 length.
    @test Base.infer_return_type(vararg_pair_4843, Tuple{Tuple{Int64,Int64}}) ===
          Tuple{DataType,Int64}

    # Nested NTuple: N (outer length) and M (inner length) -> Int64, T -> DataType.
    @test Base.infer_return_type(nested_ntuple_4843, Tuple{NTuple{2,NTuple{3,Float64}}}) ===
          Tuple{Int64,Int64,DataType}

    # NTuple{N,T} element + length parameters.
    @test Base.infer_return_type(ntuple_elem_4843, Tuple{Tuple{Float64,Float64,Float64}}) ===
          Tuple{DataType,Int64}

    # Length parameter only, fixed element type.
    @test Base.infer_return_type(ntuple_len_only_4843, Tuple{Tuple{Int64,Int64}}) === Int64
end

true
