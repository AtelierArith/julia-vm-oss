using Test

# Issue #5936: returned-Expr eval already supports these core expression heads.
# Keep them covered through generated bodies so future staging-driver work does
# not regress the compatibility fallback.

const GENERATED_QUOTE_EXPECTED_5936 = Expr(:call, :+, 1, 2)

@generated function generated_block_assignment_5936(x)
    return Expr(
        :block,
        Expr(:(=), :y, Expr(:call, :+, :x, 1)),
        Expr(:call, :*, :y, 2),
    )
end

@generated function generated_and_head_5936(::Val{B}) where B
    return Expr(:&&, B, Expr(:call, :>, 3, 2))
end

@generated function generated_or_head_5936(::Val{B}) where B
    return Expr(:||, B, Expr(:call, :>, 1, 2))
end

@generated function generated_quote_head_5936()
    return Expr(:quote, Expr(:call, :+, 1, 2))
end

@testset "generated returned block/logical/quote Expr heads (Issue #5936)" begin
    @test generated_block_assignment_5936(4) == 10
    @test generated_block_assignment_5936(7) == 16
    @test generated_and_head_5936(Val(true)) == true
    @test generated_and_head_5936(Val(false)) == false
    @test generated_or_head_5936(Val(true)) == true
    @test generated_or_head_5936(Val(false)) == false
    @test eval(Expr(:quote, GENERATED_QUOTE_EXPECTED_5936)) == GENERATED_QUOTE_EXPECTED_5936
    @test generated_quote_head_5936() == GENERATED_QUOTE_EXPECTED_5936
end

generated_block_assignment_5936(4) == 10 &&
    generated_block_assignment_5936(7) == 16 &&
    generated_and_head_5936(Val(true)) == true &&
    generated_and_head_5936(Val(false)) == false &&
    generated_or_head_5936(Val(true)) == true &&
    generated_or_head_5936(Val(false)) == false &&
    eval(Expr(:quote, GENERATED_QUOTE_EXPECTED_5936)) == GENERATED_QUOTE_EXPECTED_5936 &&
    generated_quote_head_5936() == GENERATED_QUOTE_EXPECTED_5936
