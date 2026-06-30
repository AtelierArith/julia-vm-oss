using Test

struct ValueParamMatrix7736{M,N,T}
    data::Tuple
end

function linear_index_7736(::ValueParamMatrix7736{M,N,T}, i::Int64, j::Int64) where {M,N,T}
    return (i - 1) * N + j
end

function get_at_7736(x::ValueParamMatrix7736{M,N,T}, i::Int64, j::Int64) where {M,N,T}
    return x.data[(i - 1) * N + j]
end

function compare_width_7736(::ValueParamMatrix7736{M,N,T}, n::Int64) where {M,N,T}
    return N == n && N > 1
end

function construct_with_type_param_7736(::ValueParamMatrix7736{M,N,T}, value) where {M,N,T}
    return T(value)
end

m = ValueParamMatrix7736{2,2,Int64}((1, 2, 3, 4))

ok = linear_index_7736(m, 2, 1) == 3 &&
     get_at_7736(m, 2, 1) == 3 &&
     compare_width_7736(m, 2) &&
     construct_with_type_param_7736(m, 7) == 7 &&
     typeof(construct_with_type_param_7736(m, 7)) == Int64

@testset "value type parameter arithmetic in method bodies (Issue #7736)" begin
    @test linear_index_7736(m, 2, 1) == 3
    @test get_at_7736(m, 2, 1) == 3
    @test compare_width_7736(m, 2)
    @test construct_with_type_param_7736(m, 7) == 7
end

ok
