# Regression for Matrix{T}(undef,...) and Vector{T}(undef,...) constructors
# (Issue #7890)

using Test

@testset "Matrix and Vector undef constructors" begin
    # Matrix{T}(undef, m, n)
    m_f64 = Matrix{Float64}(undef, 2, 3)
    @test typeof(m_f64) == Matrix{Float64}
    @test eltype(m_f64) == Float64
    @test size(m_f64) == (2, 3)
    @test length(m_f64) == 6

    # Matrix{T}(undef, m, n) with integer element type
    m_i64 = Matrix{Int64}(undef, 3, 2)
    @test typeof(m_i64) == Matrix{Int64}
    @test size(m_i64) == (3, 2)

    # Matrix with Bool element type
    m_bool = Matrix{Bool}(undef, 2, 2)
    @test typeof(m_bool) == Matrix{Bool}
    @test size(m_bool) == (2, 2)

    # Matrix with Complex element type
    m_complex = Matrix{Complex{Float64}}(undef, 2, 2)
    @test typeof(m_complex) == Matrix{Complex{Float64}}
    @test size(m_complex) == (2, 2)

    # Vector{T}(undef, n) (already supported, keep parity)
    v_f64 = Vector{Float64}(undef, 4)
    @test typeof(v_f64) == Vector{Float64}
    @test length(v_f64) == 4

    # Can write to undef Matrix
    m_f64[1, 1] = 1.0
    m_f64[2, 3] = 6.0
    @test m_f64[1, 1] == 1.0
    @test m_f64[2, 3] == 6.0
end

true
