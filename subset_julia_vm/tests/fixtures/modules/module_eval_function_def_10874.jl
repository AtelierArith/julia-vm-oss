# Module-body @eval function definitions (Issue #10874)
#
# A direct `@eval f(x) = x + 1` in a module body lowers to
# Stmt::EvalFunctionDef; module extraction now hoists its function into the
# module's function list (so module-body calls and qualified `M.f` calls
# resolve) while keeping the runtime DefineEvalFunction statement in the
# body — the same both-happen behavior the top-level Program path has.

using Test

module MEval10874
    @eval f(x) = x + 1
    v = f(2)
    @eval function g(x)
        x * 10
    end
    w = g(3)
end

@testset "module-body @eval function definitions" begin
    @test MEval10874.v == 3
    @test MEval10874.f(41) == 42
    @test MEval10874.w == 30
    @test MEval10874.g(5) == 50
end

true
