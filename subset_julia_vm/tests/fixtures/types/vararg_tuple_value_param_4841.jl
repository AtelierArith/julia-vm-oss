# Issue #4841: explicit `Tuple{Vararg{T,N}} where {T,N}` parameter
# signatures must bind both `T` (element type) and `N` (length value
# parameter) via the same dispatch path as the synonymous
# `NTuple{N,T} where {T,N}` alias form. Without the fix, sjulia
# reported `Dispatch(NoMethodFound)` because the `Vararg{T,N}` inner
# was parsed as a bare `JuliaType::Struct("Vararg{T,N}")` and the
# enclosing `Tuple{...}` became a one-element `TupleOf`, so the
# element-wise tuple matcher refused any multi-element call site.
#
# Fix: in `JuliaType::from_name`, translate
# `Tuple{Vararg{T,N}}` into the canonical `NTuple{N,T}` spelling so
# the existing NTuple infrastructure (dispatch matching, val-parameter
# detection in compile/mod.rs, runtime length/type binding in
# vm/mod.rs) picks it up unchanged.

using Test

v_4841(xs::Tuple{Vararg{T,N}}) where {T,N} = (T, N)
w_4841(xs::Tuple{Vararg{T,3}}) where T = T
u_4841(xs::Tuple{Vararg{Int64,N}}) where N = N
h_4841(xs::NTuple{N,T}) where {N,T} = (N, T)

@testset "Tuple{Vararg{T,N}} binds both T and N (Issue #4841)" begin
    @test v_4841((1, 2, 3)) == (Int64, 3)
    @test v_4841((Int32(1), Int32(2))) == (Int32, 2)
    @test v_4841((1.5, 2.5)) == (Float64, 2)
end

@testset "Tuple{Vararg{T,3}} with concrete N binds T (Issue #4841)" begin
    @test w_4841((1, 2, 3)) == Int64
    @test w_4841((1.0, 2.0, 3.0)) == Float64
end

@testset "Tuple{Vararg{Int64,N}} with concrete T binds N (Issue #4841)" begin
    @test u_4841((1, 2, 3)) == 3
    @test u_4841((1, 2)) == 2
    @test u_4841((1,)) == 1
end

@testset "NTuple{N,T} alias form still works (Issue #4841 regression guard)" begin
    # The NTuple{N,T} spelling already worked before #4841; confirm the
    # canonicalization step did not regress it.
    @test h_4841((1, 2, 3)) == (3, Int64)
    @test h_4841((Int32(1), Int32(2))) == (2, Int32)
end

true
