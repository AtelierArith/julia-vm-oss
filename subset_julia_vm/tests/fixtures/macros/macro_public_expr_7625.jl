using Test

module MacroPublicExpr7625
macro m()
    esc(Expr(:public, :foo))
end

@m
end

@testset "macro expansion accepts Expr(:public, ...) (Issue #7625)" begin
    @test !isdefined(MacroPublicExpr7625, :foo)
end

true
