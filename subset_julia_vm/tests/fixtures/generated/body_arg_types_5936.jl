using Test

# Issue #5936: generated-function body arguments are the concrete argument
# types, not the runtime values. Returned staged Expr evaluation still runs
# against the original runtime call frame.

@generated function generated_body_arg_types_5936(x)
    if x == Int64
        return :(10)
    elseif x == Type{Int64}
        return :(20)
    else
        return :(30)
    end
end

@generated function generated_vararg_body_arg_types_5936(xs...)
    if xs == (Int64, Float64)
        return :(12)
    else
        return :(99)
    end
end

@testset "generated body argument types (Issue #5936)" begin
    @test generated_body_arg_types_5936(1) == 10
    @test generated_body_arg_types_5936(Int64) == 20
    @test generated_body_arg_types_5936(1.0) == 30
    @test generated_vararg_body_arg_types_5936(1, 2.0) == 12
end

generated_body_arg_types_5936(1) == 10 &&
    generated_body_arg_types_5936(Int64) == 20 &&
    generated_body_arg_types_5936(1.0) == 30 &&
    generated_vararg_body_arg_types_5936(1, 2.0) == 12
