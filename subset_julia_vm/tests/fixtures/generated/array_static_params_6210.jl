using Test

# Issue #6210 / #5074: generated bodies should see static parameters bound
# through Array{T,N} signatures before the body runs.

@generated function generated_array_static_params_nt_6210(a::Array{T,N}) where {N,T}
    "N = $N, T = $T"
end

@generated function generated_array_static_params_tn_6210(a::Array{T,N}) where {T,N}
    "N = $N, T = $T"
end

const GENERATED_ARRAY_STATIC_PARAMS_MATRIX_6210 = [1.0 2.0; 3.0 4.0]

@testset "generated Array static params (Issue #6210)" begin
    @test generated_array_static_params_nt_6210(GENERATED_ARRAY_STATIC_PARAMS_MATRIX_6210) == "N = 2, T = Float64"
    @test generated_array_static_params_tn_6210(GENERATED_ARRAY_STATIC_PARAMS_MATRIX_6210) == "N = 2, T = Float64"
end

generated_array_static_params_nt_6210(GENERATED_ARRAY_STATIC_PARAMS_MATRIX_6210) == "N = 2, T = Float64" &&
    generated_array_static_params_tn_6210(GENERATED_ARRAY_STATIC_PARAMS_MATRIX_6210) == "N = 2, T = Float64"
