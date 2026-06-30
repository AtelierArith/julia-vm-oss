using Test

# Issue #6166 / #5936: a generated function reached through a function-valued
# alias should still run its generated body with concrete argument type objects,
# not runtime values.

@generated function generated_alias_call_6166(x)
    if x == Int64
        return :(10)
    else
        return :(30)
    end
end

@testset "generated alias call (Issue #6166)" begin
    alias = generated_alias_call_6166
    @test generated_alias_call_6166(1) == 10
    @test alias(1) == 10
end

alias_6166 = generated_alias_call_6166
generated_alias_call_6166(1) == 10 && alias_6166(1) == 10
