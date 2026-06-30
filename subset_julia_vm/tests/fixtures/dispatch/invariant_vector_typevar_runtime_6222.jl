# Issue #6222: runtime dispatch must not let a TypeVar reused inside an
# invariant Vector{T} slot outrank a fixed first-slot method.

using Test

invariant_vector_typevar_runtime_6222(::T, ::Vector{T}) where {T<:Real} = :diag_vec
invariant_vector_typevar_runtime_6222(::Integer, ::Vector{<:Real}) = :integer_vec_real

function invariant_vector_typevar_wrap_6222(x, y)
    invariant_vector_typevar_runtime_6222(x, y)
end

@testset "invariant Vector TypeVar runtime specificity (Issue #6222)" begin
    int_vec = [2, 3]
    real_vec = Real[2, 3.0]

    @test invariant_vector_typevar_runtime_6222(1, int_vec) === :integer_vec_real
    @test invariant_vector_typevar_runtime_6222(1, real_vec) === :integer_vec_real

    @test invariant_vector_typevar_wrap_6222(1, int_vec) === :integer_vec_real
    @test invariant_vector_typevar_wrap_6222(1, real_vec) === :integer_vec_real
end

invariant_vector_typevar_runtime_6222(1, [2, 3]) === :integer_vec_real &&
    invariant_vector_typevar_runtime_6222(1, Real[2, 3.0]) === :integer_vec_real &&
    invariant_vector_typevar_wrap_6222(1, [2, 3]) === :integer_vec_real &&
    invariant_vector_typevar_wrap_6222(1, Real[2, 3.0]) === :integer_vec_real
