using Test

# Issue #7722 / #5074: generated bodies run with argument names bound to
# generated-time type objects. Directly returning those bindings must not be
# compiled as if the runtime argument values were still in the frame.

@generated function generated_arg_type_return_7722(x)
    return x
end

@generated function generated_vararg_types_return_7722(xs...)
    return xs
end

@generated function generated_ntuple_unroll_7722(::Val{N}) where N
    ex = Expr(:tuple)
    for i in 1:N
        push!(ex.args, i * 2)
    end
    return ex
end

@testset "generated stage type args and ntuple unroll (Issue #7722)" begin
    @test generated_arg_type_return_7722(1) == Int64
    @test generated_arg_type_return_7722(1.0) == Float64
    @test generated_vararg_types_return_7722(1, 2.0) == (Int64, Float64)
    @test generated_ntuple_unroll_7722(Val(4)) == (2, 4, 6, 8)
end

generated_arg_type_return_7722(1) == Int64 &&
    generated_arg_type_return_7722(1.0) == Float64 &&
    generated_vararg_types_return_7722(1, 2.0) == (Int64, Float64) &&
    generated_ntuple_unroll_7722(Val(4)) == (2, 4, 6, 8)
