using Test

# Nested NTuple value-parameter binding (Issue #4842).
# In `NTuple{N, NTuple{M, T}}` the inner length value parameter `M` and the
# element type parameter `T` must be bound in the method frame, not just the
# outer length `N`.
hn_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = (N, M, T)
hn_inner_len_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = M
hn_inner_elem_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = T
hn_outer_len_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = N
hn_sum_first_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = sum(map(t -> t[1], xs))

# Triple nesting: every length parameter must bind.
deep_4842(xs::NTuple{A,NTuple{B,NTuple{C,T}}}) where {A,B,C,T} = (A, B, C, T)

@testset "Nested NTuple value-parameter binding (Issue #4842)" begin
    @test hn_4842(((1, 2, 3), (4, 5, 6))) == (2, 3, Int64)
    @test hn_outer_len_4842(((1, 2, 3), (4, 5, 6))) == 2
    @test hn_inner_len_4842(((1, 2, 3), (4, 5, 6))) == 3
    @test hn_inner_elem_4842(((1, 2, 3), (4, 5, 6))) == Int64
    @test hn_4842(((1.0, 2.0), (3.0, 4.0), (5.0, 6.0))) == (3, 2, Float64)
    @test hn_sum_first_4842(((10, 1), (20, 2), (30, 3))) == 60
    @test deep_4842((((1, 2), (3, 4)),)) == (1, 2, 2, Int64)
end

true
