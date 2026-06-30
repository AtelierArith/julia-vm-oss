using Test

# Issue #6192 / #5936: eval(Expr(:comparison, ...)) should evaluate all
# chained comparison pairs, not just the first left/op/right triple.

@generated function generated_comparison_chain_6192(::Val{B}) where B
    if B
        return Expr(:comparison, 1, :<, 2, :<, 3)
    else
        return Expr(:comparison, 1, :<, 2, :>, 3)
    end
end

@testset "generated/eval Expr(:comparison) chain (Issue #6192)" begin
    @test eval(Expr(:comparison, 1, :<, 2, :<, 3)) == true
    @test eval(Expr(:comparison, 1, :<, 2, :>, 3)) == false
    @test generated_comparison_chain_6192(Val(true)) == true
    @test generated_comparison_chain_6192(Val(false)) == false
end

eval(Expr(:comparison, 1, :<, 2, :<, 3)) == true &&
    eval(Expr(:comparison, 1, :<, 2, :>, 3)) == false &&
    generated_comparison_chain_6192(Val(true)) == true &&
    generated_comparison_chain_6192(Val(false)) == false
