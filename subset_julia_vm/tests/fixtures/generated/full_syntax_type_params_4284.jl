using Test

@generated function generated_return_type_4284(x::T) where T
    return T
end

@generated function generated_val_param_4284(::Val{N}) where N
    return N
end

@generated function generated_unquote_expr_4284(x)
    :(x + 1)
end

@generated function generated_unquote_return_4284(x)
    return :(x * x)
end

@generated generated_oneliner_return_type_4284(x::T) where T = T

@generated generated_oneliner_unquote_4284(x) = :(x + 2)

@testset "full @generated function syntax with type/value parameters (Issue #4284)" begin
    @test generated_return_type_4284(1) == Int64
    @test generated_return_type_4284("x") == String
    @test generated_val_param_4284(Val{3}()) == 3
    @test generated_unquote_expr_4284(2) == 3
    @test generated_unquote_return_4284(4) == 16
    @test generated_oneliner_return_type_4284(1) == Int64
    @test generated_oneliner_unquote_4284(3) == 5

    rts = Base.return_types(generated_return_type_4284, Tuple{Int64})
    @test length(rts) == 1
    @test rts[1] == Type{Int64}
    @test Base.infer_return_type(generated_return_type_4284, Tuple{Int64}) == Type{Int64}
    @test Core.Compiler.return_type(generated_return_type_4284, Tuple{Int64}) == Type{Int64}

    abstract_rts = Base.return_types(generated_return_type_4284, Tuple{Integer})
    @test length(abstract_rts) == 1
    @test abstract_rts[1] == Any
    @test Base.infer_return_type(generated_return_type_4284, Tuple{Integer}) == Any
    @test Core.Compiler.return_type(generated_return_type_4284, Tuple{Integer}) == Any
end

true
