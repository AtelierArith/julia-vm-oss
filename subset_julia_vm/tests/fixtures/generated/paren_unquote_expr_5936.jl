using Test

# Issue #5936: the generated-unquote compatibility path supports `$(expr)`
# expression interpolation, not just `$ident`. This is still syntactic unquote
# lowering, not the full generated staging driver.

@generated generated_paren_short_5936(::Val{N}) where N = :($(N + 1) * 2)

@generated function generated_paren_full_5936(::Val{N}) where N
    return :($(N + 2) * 3)
end

@testset "generated parenthesized unquote expression (Issue #5936)" begin
    @test generated_paren_short_5936(Val(3)) == 8
    @test generated_paren_short_5936(Val(10)) == 22
    @test generated_paren_full_5936(Val(3)) == 15
    @test generated_paren_full_5936(Val(5)) == 21
end

generated_paren_short_5936(Val(3)) == 8 &&
    generated_paren_short_5936(Val(10)) == 22 &&
    generated_paren_full_5936(Val(3)) == 15 &&
    generated_paren_full_5936(Val(5)) == 21
