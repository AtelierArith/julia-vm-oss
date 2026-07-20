using Test

module EvalNamespaceBoundary11072
inside_11072 = 20
end

outside_only_11072 = 10

function module_eval_missing_does_not_fall_back_to_main_11072()
    try
        @eval EvalNamespaceBoundary11072 outside_only_11072
        false
    catch err
        err isa UndefVarError
    end
end

@testset "module-target eval keeps its own namespace (Issue #11072)" begin
    @test module_eval_missing_does_not_fall_back_to_main_11072()
    @test (@eval EvalNamespaceBoundary11072 inside_11072) == 20
    @test (@eval EvalNamespaceBoundary11072 begin
        created_11072 = inside_11072 + 2
        created_11072
    end) == 22
    @test (@eval EvalNamespaceBoundary11072 created_11072) == 22
end

true
