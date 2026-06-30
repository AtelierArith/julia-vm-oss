using Test

# Issue #5936: generated functions reached through `f(args...)` should use
# the same generated-body type environment and returned-Expr signature cache as
# direct calls.

const GENERATED_SPLAT_COUNTER_5936 = Int64[]

@generated function generated_splat_body_arg_types_5936(x)
    if x == Int64
        return :(10)
    else
        return :(30)
    end
end

@generated function generated_splat_signature_cache_5936(x)
    push!(GENERATED_SPLAT_COUNTER_5936, 1)
    return :(x)
end

@testset "generated splat call compatibility (Issue #5936)" begin
    int_args = (1,)
    @test generated_splat_body_arg_types_5936(int_args...) == 10
    @test generated_splat_signature_cache_5936(int_args...) == 1
    @test generated_splat_signature_cache_5936(int_args...) == 1
    @test length(GENERATED_SPLAT_COUNTER_5936) == 1
end

args_5936 = (1,)
generated_splat_body_arg_types_5936(args_5936...) == 10 &&
    generated_splat_signature_cache_5936(args_5936...) == 1 &&
    generated_splat_signature_cache_5936(args_5936...) == 1 &&
    length(GENERATED_SPLAT_COUNTER_5936) == 1
