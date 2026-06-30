using Test

# Issue #6214 / #5074: direct generated body expressions must run with
# arguments bound to generated-time types. Returned Expr payloads still evaluate
# bare argument names against the runtime frame.

@generated function generated_direct_body_type_arg_6214(x)
    x + 1
end

@generated function generated_returned_expr_runtime_arg_6214(x)
    return :(x + 1)
end

@testset "generated direct body type arguments (Issue #6214)" begin
    @test_throws MethodError generated_direct_body_type_arg_6214(2)
    @test generated_returned_expr_runtime_arg_6214(2) == 3
end

true
